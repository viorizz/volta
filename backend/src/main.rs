mod auth;
mod charges;
mod error;
mod expenses;
mod settings;
mod stats;

use std::{env, io::Read, sync::Arc};

use anyhow::Context;
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::sync::Mutex;
use tower_http::trace::TraceLayer;

pub struct Config {
    pub admin_password_hash: String,
    pub cookie_secure: bool,
    pub timezone: String,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub limiter: Arc<Mutex<auth::LoginLimiter>>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "status": "ok" })) }))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/charges", get(charges::list).post(charges::create))
        .route("/api/charges/import", post(charges::import))
        .route("/api/charges/{id}", put(charges::update).delete(charges::remove))
        .route("/api/expenses", get(expenses::list).post(expenses::create))
        .route("/api/expenses/{id}", put(expenses::update).delete(expenses::remove))
        .route("/api/settings", get(settings::get).put(settings::put))
        .route("/api/stats", get(stats::get))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// `volta-api hash-password` : lit un mot de passe sur stdin et affiche son hash Argon2id.
fn hash_password() -> anyhow::Result<()> {
    let mut pw = String::new();
    std::io::stdin().read_to_string(&mut pw)?;
    let pw = pw.trim_end_matches(['\n', '\r']);
    anyhow::ensure!(pw.len() >= 12, "12 caractères minimum");
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    let hash = Argon2::default().hash_password(pw.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("{hash}");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.ok() };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal handler")
            .recv()
            .await
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<Option<()>>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if env::args().nth(1).as_deref() == Some("hash-password") {
        return hash_password();
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "volta_api=info,tower_http=info".into()))
        .init();

    let cfg = Config {
        admin_password_hash: env::var("ADMIN_PASSWORD_HASH").context("ADMIN_PASSWORD_HASH manquant")?,
        cookie_secure: env::var("COOKIE_SECURE").map(|v| v != "false").unwrap_or(true),
        timezone: env::var("APP_TIMEZONE").unwrap_or_else(|_| "Europe/Zurich".into()),
    };
    argon2::password_hash::PasswordHash::new(&cfg.admin_password_hash)
        .map_err(|e| anyhow::anyhow!("ADMIN_PASSWORD_HASH n'est pas un hash Argon2 valide : {e}"))?;

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&env::var("DATABASE_URL").context("DATABASE_URL manquant")?)
        .await
        .context("connexion à PostgreSQL")?;
    sqlx::migrate!().run(&db).await.context("migrations")?;

    let state = AppState { db, cfg: Arc::new(cfg), limiter: Arc::new(Mutex::new(auth::LoginLimiter::new())) };
    let addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("API à l'écoute sur {addr}");
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
