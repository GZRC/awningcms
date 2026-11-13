use crate::{
    error::{ApiError, ApiResult},
    validation, AppState,
};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub const COOKIE_NAME: &str = "__Host-cms_session";

#[derive(Clone, Serialize, FromRow)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn hash_password_sync(password: &str) -> Result<String, std::io::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| std::io::Error::other("Password hashing failed"))
}

pub(crate) async fn hash_password(state: &AppState, password: String) -> ApiResult<String> {
    let permit = state
        .password_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        hash_password_sync(&password).map_err(|_| ApiError::internal())
    })
    .await
    .map_err(|_| ApiError::internal())?
}

pub(crate) async fn verify_password(
    state: &AppState,
    password: String,
    encoded: String,
) -> ApiResult<bool> {
    let permit = state
        .password_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PasswordHash::new(&encoded)
            .map(|hash| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .map_err(|_| ApiError::internal())
}

fn session_cookie(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=86400"
    ))
    .expect("generated ASCII session cookie")
}
fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "__Host-cms_session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
    )
}

fn extract_token(headers: &HeaderMap) -> Option<String> {
    let mut found = None;
    for header in headers.get_all(header::COOKIE) {
        for cookie in header.to_str().ok()?.split(';') {
            let (key, value) = cookie.trim().split_once('=')?;
            if key == COOKIE_NAME {
                if found.is_some()
                    || value.len() != 64
                    || !value.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return None;
                }
                found = Some(value.to_string());
            }
        }
    }
    found
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = extract_token(request.headers()) else {
        return ApiError::unauthorized().into_response();
    };
    let user = sqlx::query_as::<_, CurrentUser>("SELECT u.id, u.email, u.name FROM cms_sessions s JOIN cms_users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at > now()")
        .bind(digest(&token)).fetch_optional(&state.pool).await;
    match user {
        Ok(Some(user)) => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(None) => ApiError::unauthorized().into_response(),
        Err(error) => ApiError::from(error).into_response(),
    }
}

pub async fn bootstrap(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let initialized: bool =
        sqlx::query_scalar("SELECT completed FROM cms_setup WHERE singleton=true")
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(serde_json::json!({"initialized": initialized})))
}

// Atomic counters are persisted so restarts cannot reset authentication limits. The
// global login counter gates creation of email keys, bounding retained rows.
pub(crate) async fn rate_limit(
    state: &AppState,
    key: &str,
    limit: i32,
    seconds: i32,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM cms_rate_limits WHERE expires_at < now()")
        .execute(&state.pool)
        .await?;
    let attempts: i32 = sqlx::query_scalar("INSERT INTO cms_rate_limits(key,attempts,expires_at) VALUES($1,1,now()+make_interval(secs=>$2)) ON CONFLICT(key) DO UPDATE SET attempts=CASE WHEN cms_rate_limits.expires_at<=now() THEN 1 ELSE LEAST(cms_rate_limits.attempts+1,1000000) END, expires_at=CASE WHEN cms_rate_limits.expires_at<=now() THEN now()+make_interval(secs=>$2) ELSE cms_rate_limits.expires_at END RETURNING attempts")
        .bind(key).bind(seconds as f64).fetch_one(&state.pool).await?;
    if attempts > limit {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Trop de tentatives. Réessayez dans quelques minutes.".into(),
        ));
    }
    Ok(())
}

async fn insert_session(connection: &mut PgConnection, user_id: Uuid) -> ApiResult<String> {
    sqlx::query("DELETE FROM cms_sessions WHERE expires_at <= now()")
        .execute(&mut *connection)
        .await?;
    // Serialize logins for an owner before enforcing the per-user session cap.
    sqlx::query("SELECT id FROM cms_users WHERE id=$1 FOR UPDATE")
        .bind(user_id)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM cms_sessions WHERE token_hash IN (SELECT token_hash FROM cms_sessions WHERE user_id=$1 ORDER BY created_at DESC OFFSET 49)").bind(user_id).execute(&mut *connection).await?;
    let token = random_token();
    sqlx::query("INSERT INTO cms_sessions(token_hash,user_id,expires_at) VALUES($1,$2,now()+interval '24 hours')")
        .bind(digest(&token)).bind(user_id).execute(&mut *connection).await?;
    Ok(token)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupInput {
    token: String,
    email: String,
    password: String,
    name: String,
    site_name: String,
}
pub async fn setup(
    State(state): State<AppState>,
    Json(input): Json<SetupInput>,
) -> ApiResult<Response> {
    rate_limit(&state, "setup", 20, 900).await?;
    if input.token.len() > 256
        || !bool::from(
            digest(input.token.trim())
                .as_bytes()
                .ct_eq(state.setup_token_hash.as_bytes()),
        )
    {
        return Err(ApiError::forbidden("Le code d'activation est incorrect."));
    }
    let email = validation::email(&input.email)?;
    validation::password(&input.password)?;
    let name = input.name.trim().to_string();
    validation::bounded(&name, "Le nom", 1, 100)?;
    let site_name = input.site_name.trim().to_string();
    validation::bounded(&site_name, "Le nom du site", 1, 100)?;
    let site_slug = validation::site_slug(&site_name)?;
    let password_hash = hash_password(&state, input.password).await?;
    let mut tx = state.pool.begin().await?;
    let initialized: bool =
        sqlx::query_scalar("SELECT completed FROM cms_setup WHERE singleton=true FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
    if initialized {
        return Err(ApiError::conflict(
            "Le compte administrateur a déjà été créé.",
        ));
    }
    let user = CurrentUser {
        id: Uuid::new_v4(),
        email,
        name,
    };
    sqlx::query("INSERT INTO cms_users(id,email,name,password_hash) VALUES($1,$2,$3,$4)")
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.name)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
    let site_id = Uuid::new_v4();
    sqlx::query("INSERT INTO cms_sites(id,name,slug,is_primary) VALUES($1,$2,$3,true)")
        .bind(site_id)
        .bind(&site_name)
        .bind(&site_slug)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO cms_settings(site_id,site_name) VALUES($1,$2)")
        .bind(site_id)
        .bind(&site_name)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE cms_setup SET completed=true WHERE singleton=true")
        .execute(&mut *tx)
        .await?;
    let token = insert_session(&mut tx, user.id).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, session_cookie(&token))],
        Json(user),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginInput {
    email: String,
    password: String,
}
pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> ApiResult<Response> {
    rate_limit(&state, "login:global", 30, 60).await?;
    let email = validation::email(&input.email)?;
    validation::bounded(&input.password, "Le mot de passe", 1, 256)?;
    rate_limit(&state, &format!("login:{}", digest(&email)), 8, 300).await?;
    let row: Option<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id,email,name,password_hash FROM cms_users WHERE email=$1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let encoded = row
        .as_ref()
        .map(|r| r.3.clone())
        .unwrap_or_else(|| state.dummy_password_hash.to_string());
    let valid = verify_password(&state, input.password, encoded).await?;
    let Some((id, email, name, _)) = row.filter(|_| valid) else {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "E-mail ou mot de passe incorrect.".into(),
        ));
    };
    let mut tx = state.pool.begin().await?;
    let token = insert_session(&mut tx, id).await?;
    tx.commit().await?;
    Ok((
        [(header::SET_COOKIE, session_cookie(&token))],
        Json(CurrentUser { id, email, name }),
    )
        .into_response())
}

pub async fn me(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = extract_token(&headers) {
        sqlx::query("DELETE FROM cms_sessions WHERE token_hash=$1")
            .bind(digest(&token))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie())],
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_are_random_and_cookie_is_restricted() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_ne!(digest(&a), a);
        let cookie = session_cookie(&a);
        let cookie = cookie.to_str().unwrap();
        for flag in [
            "Path=/",
            "HttpOnly",
            "Secure",
            "SameSite=Strict",
            "Max-Age=86400",
        ] {
            assert!(cookie.contains(flag));
        }
        assert!(!cookie.contains("Domain="));
    }
    #[test]
    fn rejects_malformed_and_duplicate_session_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("__Host-cms_session=short"),
        );
        assert!(extract_token(&headers).is_none());
        let token = random_token();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("other=ok; {COOKIE_NAME}={token}")).unwrap(),
        );
        assert_eq!(extract_token(&headers), Some(token.clone()));
        headers.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("{COOKIE_NAME}={token}")).unwrap(),
        );
        assert!(extract_token(&headers).is_none());
    }
    #[test]
    fn password_hash_verifies_only_original_password() {
        let encoded = hash_password_sync("une phrase longue et sûre").unwrap();
        let hash = PasswordHash::new(&encoded).unwrap();
        assert!(Argon2::default()
            .verify_password("une phrase longue et sûre".as_bytes(), &hash)
            .is_ok());
        assert!(Argon2::default()
            .verify_password(b"another password", &hash)
            .is_err());
    }
}
