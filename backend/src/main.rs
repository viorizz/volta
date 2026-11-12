mod auth;
mod charges;
mod crypto;
mod error;
mod expenses;
mod fixed_costs;
mod odometer;
mod reminders;
mod settings;
mod stats;
mod tesla;

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
    pub tesla: Option<Arc<tesla::TeslaConfig>>,
    pub tesla_lock: Arc<Mutex<()>>,
    pub http: reqwest::Client,
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
        .route("/api/fixed-costs", get(fixed_costs::list).post(fixed_costs::create))
        .route("/api/fixed-costs/{id}", put(fixed_costs::update).delete(fixed_costs::remove))
        .route("/api/reminders", get(reminders::list).post(reminders::create))
        .route("/api/reminders/{id}", put(reminders::update).delete(reminders::remove))
        .route("/api/reminders/{id}/done", post(reminders::done))
        .route("/api/odometer", get(odometer::list).post(odometer::create))
        .route("/api/settings", get(settings::get).put(settings::put))
        .route("/api/stats", get(stats::get))
        .route("/api/tesla/status", get(tesla::status))
        .route("/api/tesla/connect", post(tesla::connect))
        .route("/api/tesla/callback", get(tesla::callback))
        .route("/api/tesla/sync", post(tesla::sync_now))
        .route("/api/tesla/disconnect", post(tesla::disconnect))
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
    match env::args().nth(1).as_deref() {
        Some("hash-password") => return hash_password(),
        Some("tesla-register") => return tesla::register_cli().await,
        _ => {}
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

    let tesla = match tesla::TeslaConfig::from_env() {
        Ok(Some(t)) => {
            tracing::info!("intégration Tesla active ({})", t.api_base);
            Some(Arc::new(t))
        }
        Ok(None) => {
            tracing::info!("intégration Tesla inactive (TESLA_CLIENT_ID / TESLA_CLIENT_SECRET / PUBLIC_URL absents)");
            None
        }
        Err(e) => {
            tracing::error!("intégration Tesla désactivée : {e:#}");
            None
        }
    };
    let state = AppState {
        db,
        cfg: Arc::new(cfg),
        limiter: Arc::new(Mutex::new(auth::LoginLimiter::new())),
        tesla,
        tesla_lock: Arc::new(Mutex::new(())),
        http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?,
    };
    if let Err(e) = tesla::encrypt_legacy_tokens(&state).await {
        tracing::error!("chiffrement des jetons Tesla existants : {e:#}");
    }
    tesla::spawn_auto_sync(state.clone());
    let addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("API à l'écoute sur {addr}");
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
