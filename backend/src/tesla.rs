//! Intégration Tesla Fleet API : connexion OAuth du compte et import de
//! l'historique de recharge Supercharger (`/api/1/dx/charging/history`).

use std::env;

use anyhow::{anyhow, bail, Context};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, NaiveDateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{auth::AuthUser, error::bad_request, error::AppResult, AppState};

const AUTHORIZE_URL: &str = "https://auth.tesla.com/oauth2/v3/authorize";
const TOKEN_URL: &str = "https://fleet-auth.prd.vn.cloud.tesla.com/oauth2/v3/token";
/// Région Europe / Moyen-Orient / Afrique (Suisse incluse).
const DEFAULT_API_BASE: &str = "https://fleet-api.prd.eu.vn.cloud.tesla.com";
const SCOPES: &str = "openid offline_access vehicle_device_data vehicle_charging_cmds";
const PAGE_SIZE: u32 = 50;
const MAX_PAGES: u32 = 40;

pub struct TeslaConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub domain: String,
    pub api_base: String,
}

impl TeslaConfig {
    /// Lit la configuration ; `None` si l'intégration n'est pas configurée.
    pub fn from_env() -> Option<Self> {
        let client_id = env::var("TESLA_CLIENT_ID").ok().filter(|v| !v.is_empty())?;
        let client_secret = env::var("TESLA_CLIENT_SECRET").ok().filter(|v| !v.is_empty())?;
        let public_url = env::var("PUBLIC_URL").ok().filter(|v| !v.is_empty())?;
        let public_url = public_url.trim_end_matches('/').to_string();
        let domain = public_url.trim_start_matches("https://").trim_start_matches("http://").to_string();
        Some(Self {
            client_id,
            client_secret,
            redirect_uri: format!("{public_url}/api/tesla/callback"),
            domain,
            api_base: env::var("TESLA_API_BASE").unwrap_or_else(|_| DEFAULT_API_BASE.into()),
        })
    }
}

fn cfg(state: &AppState) -> AppResult<&TeslaConfig> {
    state
        .tesla
        .as_deref()
        .ok_or_else(|| bad_request("Intégration Tesla non configurée (TESLA_CLIENT_ID, TESLA_CLIENT_SECRET, PUBLIC_URL)"))
}

// ---------- Statut ----------

#[derive(Serialize)]
pub struct Status {
    configured: bool,
    connected: bool,
    redirect_uri: Option<String>,
    connected_at: Option<DateTime<Utc>>,
    last_sync_at: Option<DateTime<Utc>>,
    last_sync_inserted: Option<i32>,
    last_error: Option<String>,
}

pub async fn status(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<Status>> {
    let row = sqlx::query!("SELECT connected_at, last_sync_at, last_sync_inserted, last_error FROM tesla_auth WHERE id = 1")
        .fetch_optional(&s.db)
        .await?;
    Ok(Json(Status {
        configured: s.tesla.is_some(),
        connected: row.is_some(),
        redirect_uri: s.tesla.as_ref().map(|t| t.redirect_uri.clone()),
        connected_at: row.as_ref().map(|r| r.connected_at),
        last_sync_at: row.as_ref().and_then(|r| r.last_sync_at),
        last_sync_inserted: row.as_ref().and_then(|r| r.last_sync_inserted),
        last_error: row.and_then(|r| r.last_error),
    }))
}

// ---------- OAuth ----------

#[derive(Serialize)]
pub struct ConnectUrl {
    url: String,
}

/// Renvoie l'URL de connexion Tesla. Le `state` aléatoire, stocké 10 minutes,
/// protège le retour contre les requêtes forgées.
pub async fn connect(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<ConnectUrl>> {
    let c = cfg(&s)?;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let st = URL_SAFE_NO_PAD.encode(bytes);
    sqlx::query!("DELETE FROM oauth_states WHERE expires_at < now()").execute(&s.db).await?;
    sqlx::query!("INSERT INTO oauth_states (state, expires_at) VALUES ($1, now() + interval '10 minutes')", st)
        .execute(&s.db)
        .await?;
    let url = reqwest::Url::parse_with_params(
        AUTHORIZE_URL,
        &[
            ("response_type", "code"),
            ("client_id", c.client_id.as_str()),
            ("redirect_uri", c.redirect_uri.as_str()),
            ("scope", SCOPES),
            ("state", st.as_str()),
            ("locale", "fr-CH"),
            ("prompt_missing_scopes", "true"),
        ],
    )?;
    Ok(Json(ConnectUrl { url: url.to_string() }))
}

#[derive(Deserialize)]
pub struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

/// Retour de Tesla après connexion. Le cookie de session (SameSite=Strict)
/// n'est pas envoyé sur cette redirection inter-sites : c'est le `state`
/// à usage unique, créé par un utilisateur connecté, qui authentifie l'appel.
pub async fn callback(State(s): State<AppState>, Query(q): Query<Callback>) -> Redirect {
    match handle_callback(&s, q).await {
        Ok(()) => {
            let st = s.clone();
            tokio::spawn(async move {
                let _ = sync(&st).await;
            });
            Redirect::to("/?tesla=connected#reglages")
        }
        Err(e) => {
            tracing::warn!(error = ?e, "connexion Tesla échouée");
            let msg = URL_SAFE_NO_PAD.encode(e.to_string());
            Redirect::to(&format!("/?tesla=error&msg={msg}#reglages"))
        }
    }
}

async fn handle_callback(s: &AppState, q: Callback) -> anyhow::Result<()> {
    if let Some(err) = q.error {
        bail!("Tesla a refusé la connexion : {err}");
    }
    let (code, st) = q.code.zip(q.state).ok_or_else(|| anyhow!("Réponse Tesla incomplète"))?;
    let valid = sqlx::query_scalar!("DELETE FROM oauth_states WHERE state = $1 AND expires_at > now() RETURNING state", st)
        .fetch_optional(&s.db)
        .await?;
    if valid.is_none() {
        bail!("Lien de connexion expiré, recommence");
    }
    let c = s.tesla.as_deref().ok_or_else(|| anyhow!("Intégration Tesla non configurée"))?;
    let tok: TokenResponse = token_request(
        &s.http,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &c.client_id),
            ("client_secret", &c.client_secret),
            ("code", &code),
            ("audience", &c.api_base),
            ("redirect_uri", &c.redirect_uri),
        ],
    )
    .await?;
    sqlx::query!(
        r#"INSERT INTO tesla_auth (id, access_token, refresh_token, expires_at, connected_at)
           VALUES (1, $1, $2, now() + make_interval(secs => $3), now())
           ON CONFLICT (id) DO UPDATE SET access_token = $1, refresh_token = $2,
               expires_at = now() + make_interval(secs => $3), connected_at = now(), last_error = NULL"#,
        tok.access_token,
        tok.refresh_token,
        tok.expires_in as f64
    )
    .execute(&s.db)
    .await?;
    Ok(())
}

async fn token_request(http: &reqwest::Client, form: &[(&str, &str)]) -> anyhow::Result<TokenResponse> {
    let res = http.post(TOKEN_URL).form(form).send().await.context("appel Tesla (token)")?;
    let status = res.status();
    let body = res.text().await?;
    if !status.is_success() {
        bail!("Tesla token {status} : {}", truncate(&body, 300));
    }
    serde_json::from_str(&body).with_context(|| format!("réponse token illisible : {}", truncate(&body, 300)))
}

pub async fn disconnect(_: AuthUser, State(s): State<AppState>) -> AppResult<StatusCode> {
    sqlx::query!("DELETE FROM tesla_auth").execute(&s.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Jeton d'accès valide, rafraîchi s'il expire dans moins de 5 minutes.
/// Les refresh tokens Tesla sont à usage unique : le nouveau est enregistré aussitôt.
async fn access_token(s: &AppState, c: &TeslaConfig) -> anyhow::Result<String> {
    let row = sqlx::query!("SELECT access_token, refresh_token, expires_at FROM tesla_auth WHERE id = 1")
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| anyhow!("Compte Tesla non connecté"))?;
    if row.expires_at > Utc::now() + chrono::Duration::minutes(5) {
        return Ok(row.access_token);
    }
    let tok = token_request(
        &s.http,
        &[("grant_type", "refresh_token"), ("client_id", &c.client_id), ("refresh_token", &row.refresh_token)],
    )
    .await
    .context("rafraîchissement du jeton Tesla (reconnecte le compte si l'erreur persiste)")?;
    sqlx::query!(
        "UPDATE tesla_auth SET access_token = $1, refresh_token = $2, expires_at = now() + make_interval(secs => $3) WHERE id = 1",
        tok.access_token,
        tok.refresh_token,
        tok.expires_in as f64
    )
    .execute(&s.db)
    .await?;
    Ok(tok.access_token)
}

// ---------- Historique de recharge ----------

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Session {
    session_id: Option<Value>,
    site_location_name: Option<String>,
    charge_start_date_time: Option<String>,
    unlatch_date_time: Option<String>,
    #[serde(default)]
    fees: Vec<Fee>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Fee {
    fee_type: Option<String>,
    currency_code: Option<String>,
    usage_base: Option<f64>,
    usage_tier1: Option<f64>,
    usage_tier2: Option<f64>,
    usage_tier3: Option<f64>,
    usage_tier4: Option<f64>,
    total_due: Option<f64>,
    net_due: Option<f64>,
    uom: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct ParsedCharge {
    pub external_id: String,
    pub started_at: DateTime<Utc>,
    pub location: String,
    pub energy_kwh: f64,
    pub cost_cents: i64,
    pub notes: String,
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok().map(|n| n.and_utc()))
}

/// Une page de la réponse : `{"response": {"data": [...], ...}}` ou `{"response": [...]}`.
/// Renvoie les recharges lisibles et le nombre de sessions reçues.
pub fn parse_page(body: &str, currency: &str) -> anyhow::Result<(Vec<ParsedCharge>, usize)> {
    let v: Value = serde_json::from_str(body).with_context(|| format!("JSON invalide : {}", truncate(body, 300)))?;
    let resp = v.get("response").unwrap_or(&v);
    let list = resp
        .get("data")
        .or(Some(resp))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("format d'historique inattendu : {}", truncate(body, 300)))?;
    let mut out = Vec::new();
    for item in list {
        let sess: Session = match serde_json::from_value(item.clone()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "session Tesla ignorée");
                continue;
            }
        };
        let Some(id) = sess.session_id.as_ref().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())) else {
            continue;
        };
        let Some(started_at) = sess.charge_start_date_time.as_deref().or(sess.unlatch_date_time.as_deref()).and_then(parse_time) else {
            continue;
        };
        let mut energy = 0.0;
        let mut total = 0.0;
        let mut idle = 0.0;
        let mut other_currency = None;
        for f in &sess.fees {
            let due = f.total_due.or(f.net_due).unwrap_or(0.0);
            total += due;
            let kind = f.fee_type.as_deref().unwrap_or("").to_ascii_uppercase();
            if kind == "CHARGING" && f.uom.as_deref().is_some_and(|u| u.eq_ignore_ascii_case("kwh")) {
                let base = f.usage_base.unwrap_or(0.0);
                energy += if base > 0.0 {
                    base
                } else {
                    [f.usage_tier1, f.usage_tier2, f.usage_tier3, f.usage_tier4].iter().flatten().sum()
                };
            } else if kind != "CHARGING" {
                idle += due;
            }
            if let Some(cur) = f.currency_code.as_deref().filter(|c| !c.eq_ignore_ascii_case(currency)) {
                other_currency = Some(cur.to_string());
            }
        }
        let mut notes = vec!["Importé depuis Tesla".to_string()];
        if idle > 0.0 {
            notes.push(format!("dont frais d'occupation {idle:.2}"));
        }
        if let Some(c) = other_currency {
            notes.push(format!("montant facturé en {c}"));
        }
        out.push(ParsedCharge {
            external_id: format!("tesla:{id}"),
            started_at,
            location: sess.site_location_name.unwrap_or_default().chars().take(200).collect(),
            energy_kwh: (energy * 1000.0).round() / 1000.0,
            cost_cents: (total * 100.0).round().max(0.0) as i64,
            notes: notes.join(" · "),
        });
    }
    Ok((out, list.len()))
}

#[derive(Serialize)]
pub struct SyncResult {
    received: usize,
    inserted: u64,
    updated: u64,
}

/// Récupère tout l'historique et l'enregistre. Une recharge déjà importée est
/// mise à jour (les frais peuvent être finalisés après coup), sauf si elle a
/// été saisie à la main.
pub async fn sync(s: &AppState) -> anyhow::Result<SyncResult> {
    let _guard = s.tesla_lock.lock().await;
    let result = run_sync(s).await;
    match &result {
        Ok(r) => {
            sqlx::query!(
                "UPDATE tesla_auth SET last_sync_at = now(), last_sync_inserted = $1, last_error = NULL WHERE id = 1",
                r.inserted as i32
            )
            .execute(&s.db)
            .await?;
        }
        Err(e) => {
            sqlx::query!("UPDATE tesla_auth SET last_error = $1 WHERE id = 1", truncate(&format!("{e:#}"), 500))
                .execute(&s.db)
                .await?;
        }
    }
    result
}

async fn run_sync(s: &AppState) -> anyhow::Result<SyncResult> {
    let c = s.tesla.as_deref().ok_or_else(|| anyhow!("Intégration Tesla non configurée"))?;
    let token = access_token(s, c).await?;
    let currency = crate::settings::load(&s.db).await?.currency;
    let mut res = SyncResult { received: 0, inserted: 0, updated: 0 };
    for page in 1..=MAX_PAGES {
        let r = s
            .http
            .get(format!("{}/api/1/dx/charging/history", c.api_base))
            .bearer_auth(&token)
            .query(&[("pageNo", page.to_string()), ("pageSize", PAGE_SIZE.to_string()), ("sortOrder", "DESC".into())])
            .send()
            .await
            .context("appel Tesla (historique)")?;
        let status = r.status();
        let body = r.text().await?;
        if !status.is_success() {
            bail!("Tesla historique {status} : {}", truncate(&body, 300));
        }
        let (charges, count) = parse_page(&body, &currency)?;
        res.received += count;
        for ch in &charges {
            let inserted = sqlx::query_scalar!(
                r#"INSERT INTO charges (started_at, network, location, energy_kwh, cost_cents, notes, external_id, source)
                   VALUES ($1, 'supercharger', $2, $3, $4, $5, $6, 'tesla_api')
                   ON CONFLICT (external_id) DO UPDATE
                     SET energy_kwh = EXCLUDED.energy_kwh, cost_cents = EXCLUDED.cost_cents, location = EXCLUDED.location
                     WHERE charges.source = 'tesla_api'
                   RETURNING (xmax = 0) AS "inserted!""#,
                ch.started_at,
                ch.location,
                ch.energy_kwh,
                ch.cost_cents,
                ch.notes,
                ch.external_id
            )
            .fetch_optional(&s.db)
            .await?;
            match inserted {
                Some(true) => res.inserted += 1,
                Some(false) => res.updated += 1,
                None => {}
            }
        }
        if count < PAGE_SIZE as usize {
            break;
        }
    }
    Ok(res)
}

pub async fn sync_now(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<SyncResult>> {
    cfg(&s)?;
    sync(&s).await.map(Json).map_err(|e| bad_request(format!("{e:#}")))
}

/// Synchronisation automatique toutes les 12 heures.
pub fn spawn_auto_sync(s: AppState) {
    if s.tesla.is_none() {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        loop {
            let connected = sqlx::query_scalar!("SELECT EXISTS (SELECT 1 FROM tesla_auth)")
                .fetch_one(&s.db)
                .await
                .ok()
                .flatten()
                .unwrap_or(false);
            if connected {
                match sync(&s).await {
                    Ok(r) => tracing::info!(inserted = r.inserted, updated = r.updated, "synchro Tesla"),
                    Err(e) => tracing::warn!(error = %format!("{e:#}"), "synchro Tesla échouée"),
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(12 * 3600)).await;
        }
    });
}

/// `volta-api tesla-register` : enregistre le domaine auprès de Tesla (une seule fois).
/// Prérequis : la clé publique est servie sur
/// https://<domaine>/.well-known/appspecific/com.tesla.3p.public-key.pem
pub async fn register_cli() -> anyhow::Result<()> {
    let c = TeslaConfig::from_env().ok_or_else(|| anyhow!("TESLA_CLIENT_ID, TESLA_CLIENT_SECRET et PUBLIC_URL sont requis"))?;
    let http = reqwest::Client::new();
    let key_url = format!("https://{}/.well-known/appspecific/com.tesla.3p.public-key.pem", c.domain);
    let key = http.get(&key_url).send().await?;
    anyhow::ensure!(key.status().is_success(), "clé publique introuvable sur {key_url} ({})", key.status());
    println!("Clé publique trouvée sur {key_url}");

    let tok: Value = {
        let res = http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", c.client_id.as_str()),
                ("client_secret", c.client_secret.as_str()),
                ("scope", "openid vehicle_device_data vehicle_charging_cmds"),
                ("audience", c.api_base.as_str()),
            ])
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await?;
        anyhow::ensure!(status.is_success(), "jeton partenaire refusé ({status}) : {body}");
        serde_json::from_str(&body)?
    };
    let partner = tok["access_token"].as_str().ok_or_else(|| anyhow!("pas d'access_token dans la réponse"))?;
    let res = http
        .post(format!("{}/api/1/partner_accounts", c.api_base))
        .bearer_auth(partner)
        .json(&serde_json::json!({ "domain": c.domain }))
        .send()
        .await?;
    let status = res.status();
    let body = res.text().await?;
    anyhow::ensure!(status.is_success(), "enregistrement refusé ({status}) : {body}");
    println!("Domaine {} enregistré auprès de Tesla ({}).", c.domain, c.api_base);
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"response":{"data":[
      {"sessionId":123456,"vin":"5YJ3E7EB0LF000000","siteLocationName":"Lausanne, Switzerland",
       "chargeStartDateTime":"2026-10-03T18:02:11+02:00","chargeStopDateTime":"2026-10-03T18:31:40+02:00",
       "fees":[
         {"feeType":"CHARGING","currencyCode":"CHF","pricingType":"PAYMENT","rateBase":0.52,"usageBase":42.5,"usageTier1":0,"totalBase":22.10,"totalDue":22.10,"netDue":22.10,"uom":"kwh","isPaid":true},
         {"feeType":"PARKING","currencyCode":"CHF","usageBase":4,"totalDue":2.00,"netDue":2.00,"uom":"min"}
       ],
       "invoices":[{"fileName":"inv.pdf","contentId":"abc","invoiceType":"IMMEDIATE"}]},
      {"sessionId":"789","siteLocationName":"Martigny","chargeStartDateTime":"2026-09-01T08:00:00Z",
       "fees":[{"feeType":"CHARGING","currencyCode":"EUR","usageBase":0,"usageTier1":10,"usageTier2":5,"totalDue":7.5,"uom":"KWH"}]},
      {"vin":"sans identifiant","fees":[]}
    ],"totalResults":3,"hasMoreData":false}}"#;

    #[test]
    fn parses_history_page() {
        let (charges, count) = parse_page(SAMPLE, "CHF").unwrap();
        assert_eq!(count, 3);
        assert_eq!(charges.len(), 2);
        let a = &charges[0];
        assert_eq!(a.external_id, "tesla:123456");
        assert_eq!(a.started_at.to_rfc3339(), "2026-10-03T16:02:11+00:00");
        assert_eq!(a.energy_kwh, 42.5);
        assert_eq!(a.cost_cents, 2410); // recharge + frais d'occupation
        assert!(a.notes.contains("occupation 2.00"));
        let b = &charges[1];
        assert_eq!(b.external_id, "tesla:789");
        assert_eq!(b.energy_kwh, 15.0); // paliers additionnés quand usageBase = 0
        assert!(b.notes.contains("EUR"));
    }

    #[test]
    fn rejects_unknown_shape_with_body_excerpt() {
        let err = parse_page(r#"{"response":{"oops":true}}"#, "CHF").unwrap_err();
        assert!(err.to_string().contains("oops"));
    }
}
