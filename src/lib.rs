//! AwningCMS: a self-hosted CMS for small business websites.
//!
//! The library exposes the HTTP application so that it can be started by the
//! `awningcms` binary or embedded by another program.

pub mod auth;
pub mod builder;
pub mod content;
pub mod domains;
pub mod error;
pub mod extranet;
pub mod public;
pub mod structure;
pub mod validation;
pub mod visitors;

use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, get_service, post, put},
    Json, Router,
};
use error::{ApiError, ApiResult};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub origin: Arc<String>,
    pub primary_site_origin: Arc<String>,
    pub setup_token_hash: Arc<String>,
    pub dummy_password_hash: Arc<String>,
    pub password_slots: Arc<Semaphore>,
    pub domain_agent: Arc<domains::DomainAgent>,
}

pub fn app(state: AppState, static_dir: &str) -> Router {
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/logout", post(auth::logout))
        .route(
            "/sites/{site_id}/extranet-access",
            get(extranet::account_status).post(extranet::invite),
        )
        .route(
            "/sites/{site_id}/extranet-access/password",
            post(extranet::reset_password),
        )
        .route(
            "/sites/{site_id}/extranet-access/session",
            post(extranet::support_session),
        )
        .route("/domain-assistant", get(domains::assistant_status))
        .route("/sites/{site_id}/domain", get(domains::site_domain))
        .route("/sites/{site_id}/domain/suggest", post(domains::suggest))
        .route("/sites/{site_id}/domain/check", post(domains::check))
        .route("/sites/{site_id}/domain/connect", post(domains::connect))
        .route("/sites", get(content::sites).post(content::create_site))
        .route(
            "/sites/{site_id}",
            get(content::site).put(content::update_site),
        )
        .route("/sites/{site_id}/dashboard", get(content::dashboard))
        .route(
            "/sites/{site_id}/design/{target}",
            get(builder::design_read).put(builder::design_save),
        )
        .route(
            "/sites/{site_id}/design/{target}/publish",
            post(builder::design_publish),
        )
        .route(
            "/sites/{site_id}/design/{target}/preview",
            post(builder::design_preview),
        )
        .route(
            "/sites/{site_id}/contents/{id}/layout",
            get(builder::layout_read).put(builder::layout_save),
        )
        .route(
            "/sites/{site_id}/contents/{id}/layout/preview",
            post(builder::layout_preview),
        )
        .route("/sites/{site_id}/users", get(extranet::team_list))
        .route(
            "/sites/{site_id}/users/invitations",
            post(extranet::team_invite),
        )
        .route(
            "/sites/{site_id}/users/invitations/{id}",
            delete(extranet::team_revoke),
        )
        .route("/sites/{site_id}/users/{id}", put(extranet::team_update))
        .route(
            "/sites/{site_id}/users/{id}/password",
            post(extranet::team_reset_password),
        )
        .route("/sites/{site_id}/structure", get(structure::read))
        .route("/sites/{site_id}/template-preview", get(structure::preview))
        .route(
            "/sites/{site_id}/contents",
            get(content::list).post(content::create),
        )
        .route(
            "/sites/{site_id}/contents/{id}",
            get(content::read)
                .put(content::update)
                .delete(content::delete),
        )
        .route(
            "/sites/{site_id}/contents/{id}/publish",
            post(content::publish),
        )
        .route(
            "/sites/{site_id}/contents/{id}/unpublish",
            post(content::unpublish),
        )
        .route(
            "/sites/{site_id}/settings",
            get(content::settings).put(content::save_settings),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));
    let api = Router::new()
        .route("/bootstrap", get(auth::bootstrap))
        .route("/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/public/site", get(content::primary_site))
        .route("/public/contents", get(content::public_list))
        .route("/public/contents/{slug}", get(content::public_read))
        .route("/public/sites/{site_slug}", get(content::public_site))
        .route(
            "/public/sites/{site_slug}/contents",
            get(content::site_public_list),
        )
        .route(
            "/public/sites/{site_slug}/contents/{slug}",
            get(content::site_public_read),
        )
        .merge(protected)
        .layer(DefaultBodyLimit::max(512 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), origin_guard));
    let admin = ServeFile::new(format!("{static_dir}/admin.html"));
    Router::new()
        .route("/healthz", get(health))
        .route("/internal/tls-allow", get(domains::tls_allow))
        .nest("/api", api)
        .nest("/extranet/api", extranet::router(state.clone()))
        .nest("/mon-compte/api", visitors::router(state.clone()))
        .route("/admin", get_service(admin.clone()))
        .route("/admin/", get_service(admin.clone()))
        .route("/admin/{*path}", get_service(admin))
        .route("/", get(public::home))
        .route("/a-propos", get(public::about))
        .route("/blog", get(public::blog))
        .route("/mon-compte", get(visitors::page))
        .route("/mon-compte/", get(visitors::page))
        .route("/extranet", get(public::extranet))
        .route("/extranet/", get(public::extranet))
        // Explicit UI deep links keep unknown /extranet/api routes as 404s.
        .route("/extranet/login", get(public::extranet))
        .route("/extranet/setup", get(public::extranet))
        .route("/extranet/settings", get(public::extranet))
        .route("/extranet/pages", get(public::extranet))
        .route("/extranet/contents/new", get(public::extranet))
        .route("/extranet/contents/{id}", get(public::extranet))
        .route("/pages/{slug}", get(public::page))
        .route("/articles/{slug}", get(public::article))
        .route("/s/{site_slug}", get(public::site_home))
        .route("/s/{site_slug}/a-propos", get(public::site_about))
        .route("/s/{site_slug}/blog", get(public::site_blog))
        .route("/s/{site_slug}/mon-compte", get(public::site_account))
        .route("/s/{site_slug}/mon-compte/", get(public::site_account))
        .route("/s/{site_slug}/extranet", get(public::site_extranet))
        .route("/s/{site_slug}/extranet/", get(public::site_extranet))
        .route("/s/{site_slug}/pages/{slug}", get(public::site_page))
        .route("/s/{site_slug}/articles/{slug}", get(public::site_article))
        .nest_service("/assets", ServeDir::new(format!("{static_dir}/assets")))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Page introuvable") })
        .layer(middleware::from_fn(response_headers))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok(Json(serde_json::json!({"status":"ok"})))
}

async fn origin_guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if origin != Some(state.origin.as_str()) {
            return ApiError::forbidden("Origine de la requête refusée.").into_response();
        }
        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if content_type.split(';').next().unwrap_or("").trim() != "application/json" {
            return ApiError(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Une requête JSON est nécessaire.".into(),
            )
            .into_response();
        }
    }
    next.run(request).await
}

async fn response_headers(request: Request, next: Next) -> Response {
    let no_cache = request.uri().path().starts_with("/api")
        || request.uri().path().starts_with("/admin")
        || request.uri().path().starts_with("/extranet")
        || request.uri().path().starts_with("/mon-compte");
    let mut response = next.run(request).await;
    let is_template_preview = response
        .extensions()
        .get::<structure::PreviewResponse>()
        .is_some();
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    let content_security_policy = if is_template_preview {
        "default-src 'self'; script-src 'none'; style-src 'self'; img-src 'self' https: data:; connect-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'; form-action 'none'; sandbox allow-same-origin"
    } else {
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' https: data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
    };
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(content_security_policy),
    );
    if no_cache {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    fn test_app() -> Router {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://localhost/cms_test")
            .unwrap();
        app(
            AppState {
                pool,
                origin: Arc::new("https://cms.example.com".into()),
                primary_site_origin: Arc::new("https://www.example.com".into()),
                setup_token_hash: Arc::new(auth::digest("test-token")),
                dummy_password_hash: Arc::new(String::new()),
                password_slots: Arc::new(Semaphore::new(2)),
                domain_agent: Arc::new(domains::DomainAgent::disabled()),
            },
            "static",
        )
    }

    #[tokio::test]
    async fn rejects_cross_origin_and_non_json_before_database_access() {
        for origin in [None, Some("https://attacker.example"), Some("null")] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(origin) = origin {
                request = request.header(header::ORIGIN, origin);
            }
            let response = test_app()
                .oneshot(request.body(Body::from("{}")).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/login")
            .header(header::ORIGIN, "https://cms.example.com")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from("{}"))
            .unwrap();
        let response = test_app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn protects_site_management_without_session_and_does_not_cache_api() {
        let request = Request::builder()
            .uri("/api/sites")
            .body(Body::empty())
            .unwrap();
        let response = test_app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert!(response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'"));
    }
}
