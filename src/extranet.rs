//! Client administration is bound to a verified request host. Its accounts,
//! cookies and sessions are separate from the sole central owner's account.
use crate::{
    auth, builder, content,
    error::{ApiError, ApiResult},
    public, structure, validation, AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

const COOKIE: &str = "__Host-extranet_session";
#[derive(Clone)]
struct Tenant {
    site_id: Uuid,
    name: String,
    slug: String,
}
#[derive(Clone, Serialize, FromRow)]
pub struct User {
    id: Uuid,
    email: String,
    name: String,
    site_id: Uuid,
    role: String,
}

pub fn router(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/sites", get(sites))
        .route("/sites/{site_id}", get(content::site))
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
        .route("/sites/{site_id}/users", get(team_list))
        .route("/sites/{site_id}/users/invitations", post(team_invite))
        .route(
            "/sites/{site_id}/users/invitations/{id}",
            delete(team_revoke),
        )
        .route("/sites/{site_id}/users/{id}", put(team_update))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));
    Router::new()
        .route("/bootstrap", get(bootstrap))
        .route("/activate", post(activate))
        .route("/access", post(access))
        .route("/team-invitation/inspect", post(team_invitation_info))
        .route("/team-invitation", post(team_accept))
        .route("/auth/login", post(login))
        .merge(protected)
        .fallback(|| async { ApiError::missing().into_response() })
        .layer(DefaultBodyLimit::max(512 * 1024))
        .layer(middleware::from_fn_with_state(state, tenant_guard))
}

async fn tenant_guard(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let (site, origin) = match public::resolve_extranet_context(&state, request.headers()).await {
        Ok(context) => context,
        Err(error) => return error.into_response(),
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
            return ApiError::forbidden("Origine de la requête refusée.").into_response();
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
            return ApiError(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Une requête JSON est nécessaire.".into(),
            )
            .into_response();
        }
    }
    // Match the UUID in the actual routed path, never a site query supplied by
    // the browser. Unknown/encoded UUIDs fail closed before any content handler.
    let segments: Vec<_> = request.uri().path().split('/').collect();
    for pair in segments.windows(2) {
        if pair[0] == "sites" && Uuid::parse_str(pair[1]).ok() != Some(site.id) {
            return ApiError::forbidden("Cet accès est limité à votre site.").into_response();
        }
    }
    request.extensions_mut().insert(Tenant {
        site_id: site.id,
        name: site.site_name,
        slug: site.slug,
    });
    next.run(request).await
}
fn token(headers: &HeaderMap) -> Option<String> {
    let mut result = None;
    for value in headers.get_all(header::COOKIE) {
        for part in value.to_str().ok()?.split(';') {
            let Some((name, value)) = part.trim().split_once('=') else {
                continue;
            };
            if name == COOKIE {
                if result.is_some() || !valid_token(value) {
                    return None;
                }
                result = Some(value.to_string());
            }
        }
    }
    result
}
fn valid_token(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn cookie(value: &str, clear: bool) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={}",
        if clear { 0 } else { 28800 }
    ))
    .expect("generated cookie")
}
async fn require_auth(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let tenant = request
        .extensions()
        .get::<Tenant>()
        .expect("tenant guard precedes auth");
    let Some(value) = token(request.headers()) else {
        return ApiError::unauthorized().into_response();
    };
    let user = sqlx::query_as::<_, User>("SELECT COALESCE(a.id,u.id) id,COALESCE(a.email,u.email) email,COALESCE(a.name,u.name) name,s.site_id,CASE WHEN s.owner_id IS NULL THEN a.role ELSE 'owner_support' END role FROM cms_extranet_sessions s LEFT JOIN cms_extranet_accounts a ON a.id=s.account_id AND a.site_id=s.site_id AND a.active LEFT JOIN cms_users u ON u.id=s.owner_id WHERE s.token_hash=$1 AND s.site_id=$2 AND s.expires_at>now() AND (a.id IS NOT NULL OR u.id IS NOT NULL)")
        .bind(auth::digest(&value)).bind(tenant.site_id).fetch_optional(&state.pool).await;
    match user {
        Ok(Some(user)) => {
            if !permits(&user.role, request.method(), request.uri().path()) {
                return ApiError::forbidden("Votre rôle ne permet pas cette action.")
                    .into_response();
            }
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(None) => ApiError::unauthorized().into_response(),
        Err(error) => ApiError::from(error).into_response(),
    }
}
// Keep permissions on the server: hiding controls is only a convenience.
fn permits(role: &str, method: &Method, path: &str) -> bool {
    if matches!(role, "admin" | "owner_support") {
        return true;
    }
    if !matches!(role, "editor" | "viewer") {
        return false;
    }
    let parts: Vec<_> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.windows(3).any(|p| p[0] == "sites" && p[2] == "users") {
        return false;
    }
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    if parts.ends_with(&["auth", "logout"]) && method == Method::POST {
        return true;
    }
    if let Some(site_index) = parts.iter().position(|part| *part == "sites") {
        let tail = parts.get(site_index + 2..).unwrap_or_default();
        // Preview accepts document data but never changes stored content. It
        // remains useful to read-only staff and is protected like every API.
        if method == Method::POST
            && (matches!(tail, ["design", "home" | "template", "preview"])
                || matches!(tail, ["contents", _, "layout", "preview"]))
        {
            return true;
        }
        if role == "editor" && (tail == ["design", "home"] || tail == ["design", "home", "publish"])
        {
            return true;
        }
    }
    role == "editor"
        && parts
            .windows(3)
            .any(|p| p[0] == "sites" && p[2] == "contents")
}
async fn lock_site(connection: &mut PgConnection, id: Uuid) -> ApiResult<()> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM cms_sites WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(())
}
async fn audit(
    connection: &mut PgConnection,
    site: Uuid,
    owner: Option<Uuid>,
    event: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO cms_extranet_audit(site_id,owner_id,event) VALUES($1,$2,$3)")
        .bind(site)
        .bind(owner)
        .bind(event)
        .execute(connection)
        .await?;
    Ok(())
}
async fn session(
    connection: &mut PgConnection,
    site: Uuid,
    account: Option<Uuid>,
    owner: Option<Uuid>,
) -> ApiResult<String> {
    sqlx::query("DELETE FROM cms_extranet_sessions WHERE expires_at<=now()")
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM cms_extranet_sessions WHERE token_hash IN (SELECT token_hash FROM cms_extranet_sessions WHERE site_id=$1 ORDER BY created_at DESC OFFSET 49)").bind(site).execute(&mut *connection).await?;
    let value = auth::random_token();
    sqlx::query("INSERT INTO cms_extranet_sessions(token_hash,site_id,account_id,owner_id,expires_at) VALUES($1,$2,$3,$4,now()+interval '8 hours')")
        .bind(auth::digest(&value)).bind(site).bind(account).bind(owner).execute(connection).await?;
    Ok(value)
}
fn signed_in(user: User, value: String, status: StatusCode) -> Response {
    (
        status,
        [(header::SET_COOKIE, cookie(&value, false))],
        Json(user),
    )
        .into_response()
}
async fn bootstrap(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
) -> ApiResult<Json<serde_json::Value>> {
    let configured: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE site_id=$1)")
            .bind(tenant.site_id)
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(
        serde_json::json!({"initialized":true,"extranet":true,"site":{"id":tenant.site_id,"name":tenant.name,"slug":tenant.slug},"activation_required":!configured}),
    ))
}
async fn sites(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
) -> ApiResult<Json<Vec<content::Site>>> {
    let Json(site) = content::site(State(state), Path(tenant.site_id)).await?;
    Ok(Json(vec![site]))
}
async fn me(Extension(user): Extension<User>) -> Json<User> {
    Json(user)
}
async fn logout(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if let Some(value) = token(&headers) {
        sqlx::query("DELETE FROM cms_extranet_sessions WHERE token_hash=$1 AND site_id=$2")
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
    auth::rate_limit(&state, "extranet:login:global", 60, 60).await?;
    let email = validation::email(&input.email)?;
    validation::bounded(&input.password, "Le mot de passe", 1, 256)?;
    auth::rate_limit(
        &state,
        &format!("extranet:login:{}:{}", tenant.site_id, auth::digest(&email)),
        8,
        300,
    )
    .await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, tenant.site_id).await?;
    let row: Option<(Uuid, String, String, String, String)> = sqlx::query_as("SELECT id,email,name,password_hash,role FROM cms_extranet_accounts WHERE site_id=$1 AND email=$2 AND active FOR UPDATE")
        .bind(tenant.site_id).bind(&email).fetch_optional(&mut *tx).await?;
    let hash = row
        .as_ref()
        .map(|r| r.3.clone())
        .unwrap_or_else(|| state.dummy_password_hash.to_string());
    let valid = auth::verify_password(&state, input.password, hash).await?;
    let Some((id, email, name, _, role)) = row.filter(|_| valid) else {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "E-mail ou mot de passe incorrect.".into(),
        ));
    };
    let value = session(&mut tx, tenant.site_id, Some(id), None).await?;
    tx.commit().await?;
    Ok(signed_in(
        User {
            id,
            email,
            name,
            site_id: tenant.site_id,
            role,
        },
        value,
        StatusCode::OK,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivationInput {
    token: String,
    name: String,
    email: String,
    password: String,
}
async fn activate(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    Json(input): Json<ActivationInput>,
) -> ApiResult<Response> {
    auth::rate_limit(
        &state,
        &format!("extranet:activate:{}", tenant.site_id),
        10,
        300,
    )
    .await?;
    if !valid_token(&input.token) {
        return Err(ApiError::forbidden("Lien d’activation invalide ou expiré."));
    }
    let name = input.name.trim().to_string();
    validation::bounded(&name, "Le nom", 1, 100)?;
    let email = validation::email(&input.email)?;
    validation::password(&input.password)?;
    // Check token before spending time hashing; check and consume again under
    // the site lock to prevent concurrent activation or replacement races.
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_extranet_invites WHERE token_hash=$1 AND site_id=$2 AND expires_at>now())").bind(auth::digest(&input.token)).bind(tenant.site_id).fetch_one(&state.pool).await?;
    if !valid {
        return Err(ApiError::forbidden("Lien d’activation invalide ou expiré."));
    }
    let hash = auth::hash_password(&state, input.password).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, tenant.site_id).await?;
    let owner: Option<Uuid> = sqlx::query_scalar("DELETE FROM cms_extranet_invites WHERE token_hash=$1 AND site_id=$2 AND expires_at>now() RETURNING created_by").bind(auth::digest(&input.token)).bind(tenant.site_id).fetch_optional(&mut *tx).await?;
    let owner =
        owner.ok_or_else(|| ApiError::forbidden("Lien d’activation invalide ou expiré."))?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO cms_extranet_accounts(id,site_id,email,name,password_hash,is_primary) VALUES($1,$2,$3,$4,$5,true)").bind(id).bind(tenant.site_id).bind(&email).bind(&name).bind(hash).execute(&mut *tx).await?;
    let value = session(&mut tx, tenant.site_id, Some(id), None).await?;
    audit(&mut tx, tenant.site_id, Some(owner), "account_activated").await?;
    tx.commit().await?;
    Ok(signed_in(
        User {
            id,
            email,
            name,
            site_id: tenant.site_id,
            role: "admin".into(),
        },
        value,
        StatusCode::CREATED,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenInput {
    token: String,
}
async fn access(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    Json(input): Json<TokenInput>,
) -> ApiResult<Response> {
    auth::rate_limit(
        &state,
        &format!("extranet:access:{}", tenant.site_id),
        30,
        60,
    )
    .await?;
    if !valid_token(&input.token) {
        return Err(ApiError::forbidden("Lien d’accès invalide ou expiré."));
    }
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, tenant.site_id).await?;
    let owner: Option<Uuid> = sqlx::query_scalar("DELETE FROM cms_extranet_access_grants WHERE token_hash=$1 AND site_id=$2 AND expires_at>now() RETURNING owner_id").bind(auth::digest(&input.token)).bind(tenant.site_id).fetch_optional(&mut *tx).await?;
    let owner = owner.ok_or_else(|| ApiError::forbidden("Lien d’accès invalide ou expiré."))?;
    let (email, name): (String, String) =
        sqlx::query_as("SELECT email,name FROM cms_users WHERE id=$1")
            .bind(owner)
            .fetch_one(&mut *tx)
            .await?;
    let value = session(&mut tx, tenant.site_id, None, Some(owner)).await?;
    audit(&mut tx, tenant.site_id, Some(owner), "support_entered").await?;
    tx.commit().await?;
    Ok(signed_in(
        User {
            id: owner,
            email,
            name,
            site_id: tenant.site_id,
            role: "owner_support".into(),
        },
        value,
        StatusCode::OK,
    ))
}

// These management handlers are mounted only inside central owner auth.
async fn extranet_url(state: &AppState, id: Uuid) -> ApiResult<String> {
    let primary: bool = sqlx::query_scalar("SELECT is_primary FROM cms_sites WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::missing)?;
    if primary {
        return Ok(format!("{}/extranet", state.primary_site_origin));
    }
    let domain: Option<String> = sqlx::query_scalar(
        "SELECT domain FROM cms_domain_bindings WHERE site_id=$1 AND status='connected'",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let domain = domain.ok_or_else(|| {
        ApiError::conflict("Connectez le domaine du site avant de créer son accès extranet.")
    })?;
    let binding = crate::domains::binding_for_host(state, &domain).await?;
    if !binding.is_some_and(|b| b.site_id == id) {
        return Err(ApiError::conflict("Le domaine n’est plus autorisé."));
    }
    Ok(format!("https://{domain}/extranet"))
}
#[derive(Serialize, FromRow)]
struct AccountInfo {
    email: String,
    name: String,
}
pub async fn account_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let account: Option<AccountInfo> = sqlx::query_as(
        "SELECT email,name FROM cms_extranet_accounts WHERE site_id=$1 AND is_primary",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let url = match extranet_url(&state, id).await {
        Ok(url) => Some(url),
        Err(ApiError(StatusCode::CONFLICT, _)) => None,
        Err(error) => return Err(error),
    };
    Ok(Json(
        serde_json::json!({"configured":account.is_some(),"account":account,"extranet_url":url}),
    ))
}
pub async fn invite(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(owner): Extension<auth::CurrentUser>,
) -> ApiResult<Json<serde_json::Value>> {
    let url = extranet_url(&state, id).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, id).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE site_id=$1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if exists {
        return Err(ApiError::conflict(
            "Cet accès existe déjà. Vous pouvez réinitialiser son mot de passe.",
        ));
    }
    let value = auth::random_token();
    let expires: DateTime<Utc> = sqlx::query_scalar("INSERT INTO cms_extranet_invites(token_hash,site_id,created_by,expires_at) VALUES($1,$2,$3,now()+interval '24 hours') ON CONFLICT(site_id) DO UPDATE SET token_hash=excluded.token_hash,created_by=excluded.created_by,expires_at=excluded.expires_at RETURNING expires_at")
        .bind(auth::digest(&value)).bind(id).bind(owner.id).fetch_one(&mut *tx).await?;
    audit(&mut tx, id, Some(owner.id), "activation_invited").await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({"activation_url":format!("{url}#activation={value}"),"expires_at":expires}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordInput {
    password: String,
}
pub async fn reset_password(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(owner): Extension<auth::CurrentUser>,
    Json(input): Json<PasswordInput>,
) -> ApiResult<StatusCode> {
    validation::password(&input.password)?;
    let hash = auth::hash_password(&state, input.password).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, id).await?;
    let result = sqlx::query(
        "UPDATE cms_extranet_accounts SET password_hash=$2,updated_at=now() WHERE site_id=$1 AND is_primary",
    )
    .bind(id)
    .bind(hash)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    sqlx::query("DELETE FROM cms_extranet_sessions WHERE site_id=$1 AND (owner_id IS NOT NULL OR account_id IN (SELECT id FROM cms_extranet_accounts WHERE site_id=$1 AND is_primary))")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cms_extranet_access_grants WHERE site_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(&mut tx, id, Some(owner.id), "password_reset").await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn support_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(owner): Extension<auth::CurrentUser>,
) -> ApiResult<Json<serde_json::Value>> {
    let url = extranet_url(&state, id).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, id).await?;
    // One outstanding handoff per owner and site, consumed atomically once.
    sqlx::query("DELETE FROM cms_extranet_access_grants WHERE expires_at<=now() OR (owner_id=$1 AND site_id=$2)").bind(owner.id).bind(id).execute(&mut *tx).await?;
    let value = auth::random_token();
    sqlx::query("INSERT INTO cms_extranet_access_grants(token_hash,site_id,owner_id,expires_at) VALUES($1,$2,$3,now()+interval '60 seconds')").bind(auth::digest(&value)).bind(id).bind(owner.id).execute(&mut *tx).await?;
    audit(&mut tx, id, Some(owner.id), "support_grant").await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({"url":format!("{url}#access={value}")}),
    ))
}

#[derive(Clone, Copy)]
enum TeamActor {
    Owner(Uuid),
    Account(Uuid),
}
impl TeamActor {
    fn id(self) -> Uuid {
        match self {
            Self::Owner(id) | Self::Account(id) => id,
        }
    }
    fn owner(self) -> Option<Uuid> {
        if let Self::Owner(id) = self {
            Some(id)
        } else {
            None
        }
    }
    fn account(self) -> Option<Uuid> {
        if let Self::Account(id) = self {
            Some(id)
        } else {
            None
        }
    }
    async fn check(self, connection: &mut PgConnection, site: Uuid) -> ApiResult<()> {
        let allowed = match self {
            Self::Owner(id) => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM cms_users WHERE id=$1)")
                .bind(id).fetch_one(connection).await?,
            Self::Account(id) => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE id=$1 AND site_id=$2 AND active AND role='admin')")
                .bind(id).bind(site).fetch_one(connection).await?,
        };
        if !allowed {
            return Err(ApiError::forbidden(
                "Seul un administrateur peut gérer l’équipe.",
            ));
        }
        Ok(())
    }
}
fn team_actor(
    owner: Option<Extension<auth::CurrentUser>>,
    user: Option<Extension<User>>,
    site: Uuid,
) -> ApiResult<TeamActor> {
    if let Some(Extension(owner)) = owner {
        return Ok(TeamActor::Owner(owner.id));
    }
    if let Some(Extension(user)) = user.filter(|Extension(u)| u.site_id == site) {
        return match user.role.as_str() {
            "owner_support" => Ok(TeamActor::Owner(user.id)),
            "admin" => Ok(TeamActor::Account(user.id)),
            _ => Err(ApiError::forbidden(
                "Seul un administrateur peut gérer l’équipe.",
            )),
        };
    }
    Err(ApiError::unauthorized())
}
fn validate_role(role: &str) -> ApiResult<()> {
    if matches!(role, "admin" | "editor" | "viewer") {
        Ok(())
    } else {
        Err(ApiError::invalid("Choisissez un rôle valide."))
    }
}
async fn team_audit(
    connection: &mut PgConnection,
    site: Uuid,
    actor: TeamActor,
    target: Option<Uuid>,
    event: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO cms_extranet_audit(site_id,owner_id,actor_account_id,target_account_id,event) VALUES($1,$2,$3,$4,$5)")
        .bind(site).bind(actor.owner()).bind(actor.account()).bind(target).bind(event).execute(connection).await?;
    Ok(())
}
#[derive(Serialize, FromRow)]
struct TeamMember {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    active: bool,
    is_primary: bool,
    created_at: DateTime<Utc>,
}
#[derive(Serialize, FromRow)]
struct TeamInvitation {
    id: Uuid,
    email: String,
    name: String,
    role: String,
    expires_at: DateTime<Utc>,
}
pub async fn team_list(
    State(state): State<AppState>,
    Path(site): Path<Uuid>,
    owner: Option<Extension<auth::CurrentUser>>,
    user: Option<Extension<User>>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = team_actor(owner, user, site)?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    actor.check(&mut tx, site).await?;
    let users: Vec<TeamMember> = sqlx::query_as("SELECT id,email,name,role,active,is_primary,created_at FROM cms_extranet_accounts WHERE site_id=$1 ORDER BY is_primary DESC,created_at,id")
        .bind(site).fetch_all(&mut *tx).await?;
    let invitations: Vec<TeamInvitation> = sqlx::query_as("SELECT id,email,name,role,expires_at FROM cms_extranet_team_invites WHERE site_id=$1 AND expires_at>now() ORDER BY created_at,id")
        .bind(site).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({"users":users,"invitations":invitations,"current_user_id":actor.id()}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamInviteInput {
    email: String,
    #[serde(default)]
    name: String,
    role: String,
}
pub async fn team_invite(
    State(state): State<AppState>,
    Path(site): Path<Uuid>,
    owner: Option<Extension<auth::CurrentUser>>,
    user: Option<Extension<User>>,
    Json(input): Json<TeamInviteInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor = team_actor(owner, user, site)?;
    let email = validation::email(&input.email)?;
    let name = input.name.trim();
    validation::bounded(name, "Le nom", 0, 100)?;
    validate_role(&input.role)?;
    auth::rate_limit(&state, &format!("extranet:team-invite:{site}"), 50, 3600).await?;
    let url = extranet_url(&state, site).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    actor.check(&mut tx, site).await?;
    let initialized: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE site_id=$1 AND is_primary)",
    )
    .bind(site)
    .fetch_one(&mut *tx)
    .await?;
    if !initialized {
        return Err(ApiError::conflict(
            "Activez d’abord le compte principal de cet extranet.",
        ));
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE site_id=$1 AND email=$2)",
    )
    .bind(site)
    .bind(&email)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(ApiError::conflict(
            "Ce membre existe déjà. Vous pouvez modifier son rôle ou réactiver son accès.",
        ));
    }
    let value = auth::random_token();
    let expires: DateTime<Utc> = sqlx::query_scalar("INSERT INTO cms_extranet_team_invites(id,site_id,email,name,role,token_hash,owner_id,account_id,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,now()+interval '24 hours') ON CONFLICT(site_id,email) DO UPDATE SET name=excluded.name,role=excluded.role,token_hash=excluded.token_hash,owner_id=excluded.owner_id,account_id=excluded.account_id,expires_at=excluded.expires_at RETURNING expires_at")
        .bind(Uuid::new_v4()).bind(site).bind(&email).bind(name).bind(&input.role).bind(auth::digest(&value)).bind(actor.owner()).bind(actor.account()).fetch_one(&mut *tx).await?;
    team_audit(&mut tx, site, actor, None, "team_invited").await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({"activation_url":format!("{url}#invitation={value}"),"expires_at":expires}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamUpdateInput {
    role: String,
    active: bool,
}
pub async fn team_update(
    State(state): State<AppState>,
    Path((site, id)): Path<(Uuid, Uuid)>,
    owner: Option<Extension<auth::CurrentUser>>,
    user: Option<Extension<User>>,
    Json(input): Json<TeamUpdateInput>,
) -> ApiResult<StatusCode> {
    let actor = team_actor(owner, user, site)?;
    validate_role(&input.role)?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    actor.check(&mut tx, site).await?;
    let (role, active): (String, bool) = sqlx::query_as(
        "SELECT role,active FROM cms_extranet_accounts WHERE id=$1 AND site_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(site)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::missing)?;
    if role == "admin" && active && (input.role != "admin" || !input.active) {
        let other_admin: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE site_id=$1 AND id<>$2 AND active AND role='admin')").bind(site).bind(id).fetch_one(&mut *tx).await?;
        if !other_admin {
            return Err(ApiError::conflict(
                "Conservez au moins un administrateur actif pour ce site.",
            ));
        }
    }
    sqlx::query("UPDATE cms_extranet_accounts SET role=$3,active=$4,updated_at=now() WHERE id=$1 AND site_id=$2")
        .bind(id).bind(site).bind(&input.role).bind(input.active).execute(&mut *tx).await?;
    if !input.active {
        sqlx::query("DELETE FROM cms_extranet_sessions WHERE site_id=$1 AND account_id=$2")
            .bind(site)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    if !input.active || input.role != "admin" {
        sqlx::query("DELETE FROM cms_extranet_team_invites WHERE site_id=$1 AND account_id=$2")
            .bind(site)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    team_audit(&mut tx, site, actor, Some(id), "team_updated").await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn team_revoke(
    State(state): State<AppState>,
    Path((site, id)): Path<(Uuid, Uuid)>,
    owner: Option<Extension<auth::CurrentUser>>,
    user: Option<Extension<User>>,
) -> ApiResult<StatusCode> {
    let actor = team_actor(owner, user, site)?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    actor.check(&mut tx, site).await?;
    let result = sqlx::query("DELETE FROM cms_extranet_team_invites WHERE site_id=$1 AND id=$2")
        .bind(site)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    team_audit(&mut tx, site, actor, None, "team_invitation_revoked").await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Serialize, FromRow)]
struct InvitationDetails {
    email: String,
    name: String,
    role: String,
}
async fn team_invitation_info(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    Json(input): Json<TokenInput>,
) -> ApiResult<Json<InvitationDetails>> {
    auth::rate_limit(
        &state,
        &format!("extranet:team-inspect:{}", tenant.site_id),
        60,
        300,
    )
    .await?;
    if !valid_token(&input.token) {
        return Err(ApiError::forbidden("Invitation invalide ou expirée."));
    }
    let details = sqlx::query_as("SELECT i.email,i.name,i.role FROM cms_extranet_team_invites i WHERE i.site_id=$1 AND i.token_hash=$2 AND i.expires_at>now() AND (i.owner_id IS NOT NULL OR EXISTS(SELECT 1 FROM cms_extranet_accounts a WHERE a.id=i.account_id AND a.site_id=i.site_id AND a.active AND a.role='admin'))")
        .bind(tenant.site_id).bind(auth::digest(&input.token)).fetch_optional(&state.pool).await?.ok_or_else(||ApiError::forbidden("Invitation invalide ou expirée."))?;
    Ok(Json(details))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamAcceptInput {
    token: String,
    name: String,
    password: String,
}
async fn team_accept(
    State(state): State<AppState>,
    Extension(tenant): Extension<Tenant>,
    Json(input): Json<TeamAcceptInput>,
) -> ApiResult<Response> {
    auth::rate_limit(
        &state,
        &format!("extranet:team-accept:{}", tenant.site_id),
        20,
        300,
    )
    .await?;
    if !valid_token(&input.token) {
        return Err(ApiError::forbidden("Invitation invalide ou expirée."));
    }
    let name = input.name.trim().to_string();
    validation::bounded(&name, "Le nom", 1, 100)?;
    validation::password(&input.password)?;
    let token_hash = auth::digest(&input.token);
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cms_extranet_team_invites WHERE site_id=$1 AND token_hash=$2 AND expires_at>now())")
        .bind(tenant.site_id).bind(&token_hash).fetch_one(&state.pool).await?;
    if !valid {
        return Err(ApiError::forbidden("Invitation invalide ou expirée."));
    }
    let hash = auth::hash_password(&state, input.password).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, tenant.site_id).await?;
    let invitation: Option<(String,String,Option<Uuid>,Option<Uuid>)> = sqlx::query_as("DELETE FROM cms_extranet_team_invites WHERE site_id=$1 AND token_hash=$2 AND expires_at>now() RETURNING email,role,owner_id,account_id")
        .bind(tenant.site_id).bind(&token_hash).fetch_optional(&mut *tx).await?;
    let (email, role, owner, account) =
        invitation.ok_or_else(|| ApiError::forbidden("Invitation invalide ou expirée."))?;
    let actor = if let Some(owner) = owner {
        TeamActor::Owner(owner)
    } else {
        TeamActor::Account(account.ok_or_else(ApiError::missing)?)
    };
    actor.check(&mut tx, tenant.site_id).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cms_extranet_accounts WHERE site_id=$1 AND email=$2)",
    )
    .bind(tenant.site_id)
    .bind(&email)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(ApiError::conflict(
            "Ce compte existe déjà. Connectez-vous avec vos identifiants.",
        ));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO cms_extranet_accounts(id,site_id,email,name,password_hash,role) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(tenant.site_id).bind(&email).bind(&name).bind(hash).bind(&role).execute(&mut *tx).await?;
    team_audit(&mut tx, tenant.site_id, actor, Some(id), "team_joined").await?;
    let value = session(&mut tx, tenant.site_id, Some(id), None).await?;
    tx.commit().await?;
    Ok(signed_in(
        User {
            id,
            email,
            name,
            site_id: tenant.site_id,
            role,
        },
        value,
        StatusCode::CREATED,
    ))
}
// Password administration remains reserved for the authenticated central owner.
pub async fn team_reset_password(
    State(state): State<AppState>,
    Path((site, id)): Path<(Uuid, Uuid)>,
    Extension(owner): Extension<auth::CurrentUser>,
    Json(input): Json<PasswordInput>,
) -> ApiResult<StatusCode> {
    validation::password(&input.password)?;
    let hash = auth::hash_password(&state, input.password).await?;
    let mut tx = state.pool.begin().await?;
    lock_site(&mut tx, site).await?;
    let actor = TeamActor::Owner(owner.id);
    actor.check(&mut tx, site).await?;
    let result = sqlx::query("UPDATE cms_extranet_accounts SET password_hash=$3,updated_at=now() WHERE site_id=$1 AND id=$2")
        .bind(site).bind(id).bind(hash).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::missing());
    }
    sqlx::query("DELETE FROM cms_extranet_sessions WHERE site_id=$1 AND account_id=$2")
        .bind(site)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    team_audit(&mut tx, site, actor, Some(id), "team_password_reset").await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roles_enforce_content_and_team_boundaries() {
        for role in ["admin", "owner_support"] {
            assert!(permits(role, &Method::PUT, "/sites/uuid/settings"));
            assert!(permits(role, &Method::GET, "/sites/uuid/users"));
        }
        assert!(permits(
            "editor",
            &Method::POST,
            "/sites/uuid/contents/id/publish"
        ));
        assert!(permits(
            "editor",
            &Method::DELETE,
            "/extranet/api/sites/uuid/contents/id"
        ));
        assert!(!permits("editor", &Method::PUT, "/sites/uuid/settings"));
        assert!(!permits("editor", &Method::GET, "/sites/uuid/users"));
        assert!(!permits("viewer", &Method::POST, "/sites/uuid/contents"));
        assert!(permits("viewer", &Method::GET, "/sites/uuid/contents"));
        assert!(permits("viewer", &Method::POST, "/auth/logout"));
        assert!(!permits("unknown", &Method::GET, "/sites/uuid"));
        assert!(permits("editor", &Method::PUT, "/sites/uuid/design/home"));
        assert!(!permits(
            "editor",
            &Method::PUT,
            "/sites/uuid/design/template"
        ));
        assert!(!permits("viewer", &Method::PUT, "/sites/uuid/design/home"));
        assert!(permits(
            "viewer",
            &Method::POST,
            "/sites/uuid/design/home/preview"
        ));
        assert!(permits(
            "viewer",
            &Method::POST,
            "/sites/uuid/contents/id/layout/preview"
        ));
    }
    #[test]
    fn client_cookie_has_separate_name_and_no_shared_domain() {
        let value = auth::random_token();
        let cookie = cookie(&value, false);
        let cookie = cookie.to_str().unwrap();
        assert!(cookie.starts_with("__Host-extranet_session="));
        for flag in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/"] {
            assert!(cookie.contains(flag));
        }
        assert!(!cookie.contains("Domain="));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{}={value}", auth::COOKIE_NAME)).unwrap(),
        );
        assert!(token(&headers).is_none());
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("{COOKIE}={value}; {COOKIE}={value}")).unwrap(),
        );
        assert!(token(&headers).is_none());
    }
}
