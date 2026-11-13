//! Public visitor accounts are scoped to the current verified site. They never
//! grant staff access and use separate tables and a separate host-only cookie.
use crate::{
    auth,
    error::{ApiError, ApiResult},
    public, validation, AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

const COOKIE: &str = "__Host-visitor_session";

#[derive(Clone)]
struct Tenant {
    site_id: Uuid,
    name: String,
    slug: String,
}

#[derive(Clone, Serialize, FromRow)]
struct Visitor {
    id: Uuid,
    site_id: Uuid,
    email: String,
    name: String,
}

pub fn router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/me", get(me))
        .route("/profile", put(profile))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));
    Router::new()
        .route("/bootstrap", get(bootstrap))
        .route("/register", post(register))
        .route("/login", post(login))
        // Logout is idempotent, including after expiration of the session.
        .route("/logout", post(logout))
        .merge(protected)
        .fallback(|| async { ApiError::missing().into_response() })
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(state, tenant_guard))
}

pub async fn page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match public::render_account(&state, &headers).await {
        Ok(html) => private_response(Html(html).into_response()),
        Err(error) => private_response(error.into_response()),
    }
}

fn private_response(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    response
}

async fn tenant_guard(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let (site, origin) = match public::resolve_extranet_context(&state, request.headers()).await {
        Ok(context) => context,
        Err(error) => return private_response(error.into_response()),
    };
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        if request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            != Some(origin.as_str())
        {
            return private_response(
                ApiError::forbidden("Origine de la requête refusée.").into_response(),
            );
        }
        if request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            != "application/json"
        {
            return private_response(
                ApiError(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "Une requête JSON est nécessaire.".into(),
                )
                .into_response(),
            );
        }
    }
    request.extensions_mut().insert(Tenant {
        site_id: site.id,
        name: site.site_name,
        slug: site.slug,
    });
    private_response(next.run(request).await)
}

fn token(headers: &HeaderMap) -> Option<String> {
    let mut result = None;
    for value in headers.get_all(header::COOKIE) {
        for part in value.to_str().ok()?.split(';') {
            let Some((name, value)) = part.trim().split_once('=') else {
                continue;
            };
            if name == COOKIE {
                if result.is_some()
                    || value.len() != 64
                    || !value.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return None;
                }
                result = Some(value.to_string());
            }
        }
    }
    result
}

fn cookie(value: &str, clear: bool) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={}",
        if clear { 0 } else { 86400 }
    ))
    .expect("generated visitor cookie")
}

async fn require_auth(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let tenant = request
        .extensions()
        .get::<Tenant>()
        .expect("tenant guard precedes auth");
    let Some(value) = token(request.headers()) else {
        return ApiError::unauthorized().into_response();
    };
    let visitor = sqlx::query_as::<_, Visitor>("SELECT a.id,a.site_id,a.email,a.name FROM cms_visitor_sessions s JOIN cms_visitor_accounts a ON a.id=s.account_id AND a.site_id=s.site_id WHERE s.token_hash=$1 AND s.site_id=$2 AND s.expires_at>now()")
        .bind(auth::digest(&value)).bind(tenant.site_id).fetch_optional(&state.pool).await;
    match visitor {
        Ok(Some(visitor)) => {
            request.extensions_mut().insert(visitor);
            next.run(request).await
        }
        Ok(None) => ApiError::unauthorized().into_response(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

async fn session(connection: &mut PgConnection, site: Uuid, account: Uuid) -> ApiResult<String> {
    // Serialize only this account's session creation, never all visitors of a site.
    sqlx::query("SELECT id FROM cms_visitor_accounts WHERE id=$1 AND site_id=$2 FOR UPDATE")
        .bind(account)
        .bind(site)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM cms_visitor_sessions WHERE expires_at<=now()")
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM cms_visitor_sessions WHERE token_hash IN (SELECT token_hash FROM cms_visitor_sessions WHERE site_id=$1 AND account_id=$2 ORDER BY created_at DESC OFFSET 9)")
        .bind(site).bind(account).execute(&mut *connection).await?;
    let value = auth::random_token();
    sqlx::query("INSERT INTO cms_visitor_sessions(token_hash,site_id,account_id,expires_at) VALUES($1,$2,$3,now()+interval '24 hours')")
        .bind(auth::digest(&value)).bind(site).bind(account).execute(connection).await?;
    Ok(value)
}

fn signed_in(visitor: Visitor, value: String, status: StatusCode) -> Response {
    (
        status,
        [(header::SET_COOKIE, cookie(&value, false))],
        Json(visitor),
    )
        .into_response()
}

async fn bootstrap(Extension(tenant): Extension<Tenant>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"site":{"id":tenant.site_id,"name":tenant.name,"slug":tenant.slug},"registration_open":true}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterInput {
    name: String,
    email: String,
    password: String,
}

async fn register(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    Json(input): Json<RegisterInput>,
) -> ApiResult<Response> {
    auth::rate_limit(&state, "visitor:register:global", 30, 300).await?;
    auth::rate_limit(
        &state,
        &format!("visitor:register:{}", tenant.site_id),
        20,
        300,
    )
    .await?;
    let name = input.name.trim().to_string();
    validation::bounded(&name, "Le nom", 1, 100)?;
    let email = validation::email(&input.email)?;
    validation::password(&input.password)?;
    let hash = auth::hash_password(&state, input.password).await?;
    let visitor = Visitor {
        id: Uuid::new_v4(),
        site_id: tenant.site_id,
        email,
        name,
    };
    let mut tx = state.pool.begin().await?;
    let inserted = sqlx::query("INSERT INTO cms_visitor_accounts(id,site_id,email,name,password_hash) VALUES($1,$2,$3,$4,$5) ON CONFLICT(site_id,email) DO NOTHING")
        .bind(visitor.id).bind(visitor.site_id).bind(&visitor.email).bind(&visitor.name).bind(hash).execute(&mut *tx).await?;
    if inserted.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "Impossible de créer ce compte. Si vous êtes déjà inscrit, connectez-vous.",
        ));
    }
    let value = session(&mut tx, visitor.site_id, visitor.id).await?;
    tx.commit().await?;
    Ok(signed_in(visitor, value, StatusCode::CREATED))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginInput {
    email: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    Json(input): Json<LoginInput>,
) -> ApiResult<Response> {
    auth::rate_limit(&state, "visitor:login:global", 90, 60).await?;
    let email = validation::email(&input.email)?;
    validation::bounded(&input.password, "Le mot de passe", 1, 256)?;
    auth::rate_limit(
        &state,
        &format!("visitor:login:{}:{}", tenant.site_id, auth::digest(&email)),
        8,
        300,
    )
    .await?;
    let row: Option<(Uuid, String, String, String)> = sqlx::query_as("SELECT id,email,name,password_hash FROM cms_visitor_accounts WHERE site_id=$1 AND email=$2")
        .bind(tenant.site_id).bind(&email).fetch_optional(&state.pool).await?;
    let hash = row
        .as_ref()
        .map(|r| r.3.clone())
        .unwrap_or_else(|| state.dummy_password_hash.to_string());
    let valid = auth::verify_password(&state, input.password, hash).await?;
    let Some((id, email, name, _)) = row.filter(|_| valid) else {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "E-mail ou mot de passe incorrect.".into(),
        ));
    };
    let mut tx = state.pool.begin().await?;
    let value = session(&mut tx, tenant.site_id, id).await?;
    tx.commit().await?;
    Ok(signed_in(
        Visitor {
            id,
            site_id: tenant.site_id,
            email,
            name,
        },
        value,
        StatusCode::OK,
    ))
}

async fn me(Extension(visitor): Extension<Visitor>) -> Json<Visitor> {
    Json(visitor)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileInput {
    name: String,
}

async fn profile(
    State(state): State<AppState>,
    Extension(visitor): Extension<Visitor>,
    Json(input): Json<ProfileInput>,
) -> ApiResult<Json<Visitor>> {
    let name = input.name.trim().to_string();
    validation::bounded(&name, "Le nom", 1, 100)?;
    let visitor = sqlx::query_as::<_, Visitor>("UPDATE cms_visitor_accounts SET name=$1,updated_at=now() WHERE id=$2 AND site_id=$3 RETURNING id,site_id,email,name")
        .bind(name).bind(visitor.id).bind(visitor.site_id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::unauthorized)?;
    Ok(Json(visitor))
}

async fn logout(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if let Some(value) = token(&headers) {
        sqlx::query("DELETE FROM cms_visitor_sessions WHERE token_hash=$1 AND site_id=$2")
            .bind(auth::digest(&value))
            .bind(tenant.site_id)
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie("", true))],
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staff_cookies_cannot_authenticate_a_visitor() {
        let value = "a".repeat(64);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!(
                "__Host-cms_session={value}; __Host-extranet_session={value}"
            ))
            .unwrap(),
        );
        assert!(token(&headers).is_none());
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{COOKIE}={value}")).unwrap(),
        );
        assert_eq!(token(&headers), Some(value.clone()));
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{COOKIE}={value}")).unwrap(),
        );
        assert!(token(&headers).is_none());
    }
    #[test]
    fn visitor_cookie_is_secure_host_only_and_expiring() {
        let value = cookie("abc", false);
        let value = value.to_str().unwrap();
        assert!(value.starts_with("__Host-visitor_session="));
        for attribute in [
            "Path=/",
            "HttpOnly",
            "Secure",
            "SameSite=Strict",
            "Max-Age=86400",
        ] {
            assert!(value.contains(attribute));
        }
        assert!(!value.contains("Domain="));
        assert!(cookie("", true).to_str().unwrap().contains("Max-Age=0"));
    }
}
