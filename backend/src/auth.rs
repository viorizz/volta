use std::time::{Duration, Instant};

use anyhow::anyhow;
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use axum::{extract::FromRequestParts, extract::State, http::request::Parts, http::StatusCode, Json};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{error::AppError, error::AppResult, AppState};

pub const COOKIE_NAME: &str = "volta_session";
const SESSION_DAYS: i64 = 30;
const MAX_FAILURES: u32 = 5;
const LOCK_WINDOW: Duration = Duration::from_secs(15 * 60);

/// Limite les tentatives de connexion : 5 échecs bloquent 15 minutes.
pub struct LoginLimiter {
    failures: u32,
    window_start: Instant,
}

impl LoginLimiter {
    pub fn new() -> Self {
        Self { failures: 0, window_start: Instant::now() }
    }
    fn check(&mut self) -> AppResult<()> {
        if self.window_start.elapsed() > LOCK_WINDOW {
            self.failures = 0;
            self.window_start = Instant::now();
        }
        if self.failures >= MAX_FAILURES {
            return Err(AppError::TooManyRequests);
        }
        Ok(())
    }
}

fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[derive(Deserialize)]
pub struct LoginInput {
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(input): Json<LoginInput>,
) -> AppResult<(CookieJar, StatusCode)> {
    state.limiter.lock().await.check()?;

    let stored = state.cfg.admin_password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let parsed = PasswordHash::new(&stored).map_err(|e| anyhow!("ADMIN_PASSWORD_HASH invalide: {e}"))?;
        Ok(Argon2::default().verify_password(input.password.as_bytes(), &parsed).is_ok())
    })
    .await??;

    if !ok {
        state.limiter.lock().await.failures += 1;
        return Err(AppError::Unauthorized);
    }
    *state.limiter.lock().await = LoginLimiter::new();

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);

    sqlx::query!("DELETE FROM sessions WHERE expires_at < now()").execute(&state.db).await?;
    sqlx::query!(
        "INSERT INTO sessions (token_hash, expires_at) VALUES ($1, now() + make_interval(days => $2))",
        hash_token(&token),
        SESSION_DAYS as i32
    )
    .execute(&state.db)
    .await?;

    let cookie = Cookie::build((COOKIE_NAME, token))
        .http_only(true)
        .secure(state.cfg.cookie_secure)
        .same_site(SameSite::Strict)
        .path("/")
        .max_age(time::Duration::days(SESSION_DAYS))
        .build();
    Ok((jar.add(cookie), StatusCode::NO_CONTENT))
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> AppResult<(CookieJar, StatusCode)> {
    if let Some(c) = jar.get(COOKIE_NAME) {
        sqlx::query!("DELETE FROM sessions WHERE token_hash = $1", hash_token(c.value()))
            .execute(&state.db)
            .await?;
    }
    Ok((jar.remove(Cookie::build(COOKIE_NAME).path("/")), StatusCode::NO_CONTENT))
}

pub async fn me(_user: AuthUser) -> Json<Value> {
    Json(json!({ "authenticated": true }))
}

/// Extracteur : la requête doit porter un cookie de session valide.
pub struct AuthUser;

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(COOKIE_NAME).ok_or(AppError::Unauthorized)?;
        let found = sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE token_hash = $1 AND expires_at > now())",
            hash_token(token.value())
        )
        .fetch_one(&state.db)
        .await?;
        if found == Some(true) {
            Ok(AuthUser)
        } else {
            Err(AppError::Unauthorized)
        }
    }
}
