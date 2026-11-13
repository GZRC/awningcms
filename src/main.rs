use awningcms::{auth, domains, validation, AppState};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{env, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("awningcms=info,tower_http=warn")
        .init();
    let db_password = tokio::fs::read_to_string(env::var("PGPASSWORD_FILE")?).await?;
    let setup_token = tokio::fs::read_to_string(env::var("SETUP_TOKEN_FILE")?).await?;
    if setup_token.trim().len() < 32 {
        return Err("SETUP_TOKEN_FILE must contain at least 32 characters".into());
    }
    let origin = env::var("PUBLIC_ORIGIN")?;
    validation::origin(&origin).map_err(|_| {
        "PUBLIC_ORIGIN must be an exact https origin (localhost http is allowed for tests)"
    })?;
    let primary_site_origin = env::var("PRIMARY_SITE_ORIGIN").unwrap_or_else(|_| origin.clone());
    validation::origin(&primary_site_origin).map_err(|_| {
        "PRIMARY_SITE_ORIGIN must be an exact https origin (localhost http is allowed for tests)"
    })?;
    let options = PgConnectOptions::new_without_pgpass()
        .host(&env::var("PGHOST").unwrap_or_else(|_| "postgres".into()))
        .port(
            env::var("PGPORT")
                .unwrap_or_else(|_| "5432".into())
                .parse()?,
        )
        .username(&env::var("PGUSER").unwrap_or_else(|_| "cms".into()))
        .database(&env::var("PGDATABASE").unwrap_or_else(|_| "cms".into()))
        .password(db_password.trim());
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let dummy_password_hash = auth::hash_password_sync(&auth::random_token())?;
    let domain_agent = domains::DomainAgent::from_env().await?;
    let state = AppState {
        pool,
        origin: Arc::new(origin),
        primary_site_origin: Arc::new(primary_site_origin),
        setup_token_hash: Arc::new(auth::digest(setup_token.trim())),
        dummy_password_hash: Arc::new(dummy_password_hash),
        password_slots: Arc::new(Semaphore::new(2)),
        domain_agent: Arc::new(domain_agent),
    };
    if state.domain_agent.enabled() {
        tokio::spawn(domains::run_worker(state.clone()));
    }
    let static_dir = env::var("STATIC_DIR").unwrap_or_else(|_| "static".into());
    let app = awningcms::app(state, &static_dir);
    let port: u16 = env::var("PORT").unwrap_or_else(|_| "8080".into()).parse()?;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "CMS server ready");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
