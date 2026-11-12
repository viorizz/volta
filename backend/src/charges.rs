use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthUser, error::bad_request, error::AppError, error::AppResult, AppState};

const NETWORKS: [&str; 4] = ["supercharger", "home", "public", "other"];

#[derive(Serialize)]
pub struct Charge {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub network: String,
    pub location: String,
    pub energy_kwh: f64,
    pub cost_cents: i64,
    pub odometer_km: Option<i32>,
    pub source: String,
    pub external_id: Option<String>,
    pub notes: String,
}

#[derive(Deserialize)]
pub struct ChargeInput {
    pub started_at: DateTime<Utc>,
    pub network: String,
    #[serde(default)]
    pub location: String,
    pub energy_kwh: f64,
    pub cost_cents: i64,
    pub odometer_km: Option<i32>,
    #[serde(default)]
    pub notes: String,
    pub external_id: Option<String>,
}

impl ChargeInput {
    fn validate(&self) -> AppResult<()> {
        if !NETWORKS.contains(&self.network.as_str()) {
            return Err(bad_request(format!("Réseau inconnu : {}", self.network)));
        }
        if !self.energy_kwh.is_finite() || self.energy_kwh < 0.0 || self.energy_kwh > 1000.0 {
            return Err(bad_request("Énergie invalide"));
        }
        if self.cost_cents < 0 {
            return Err(bad_request("Le coût ne peut pas être négatif"));
        }
        if self.odometer_km.is_some_and(|o| o < 0) {
            return Err(bad_request("Kilométrage invalide"));
        }
        if self.location.len() > 200 || self.notes.len() > 2000 {
            return Err(bad_request("Texte trop long"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct Range {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

pub async fn list(_: AuthUser, State(s): State<AppState>, Query(r): Query<Range>) -> AppResult<Json<Vec<Charge>>> {
    let rows = sqlx::query_as!(
        Charge,
        r#"SELECT id, started_at, network, location, energy_kwh, cost_cents, odometer_km, source, external_id, notes
           FROM charges
           WHERE ($1::timestamptz IS NULL OR started_at >= $1) AND ($2::timestamptz IS NULL OR started_at < $2)
           ORDER BY started_at DESC"#,
        r.from,
        r.to
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create(_: AuthUser, State(s): State<AppState>, Json(i): Json<ChargeInput>) -> AppResult<(StatusCode, Json<Charge>)> {
    i.validate()?;
    let row = sqlx::query_as!(
        Charge,
        r#"INSERT INTO charges (started_at, network, location, energy_kwh, cost_cents, odometer_km, notes, external_id, source)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'manual')
           RETURNING id, started_at, network, location, energy_kwh, cost_cents, odometer_km, source, external_id, notes"#,
        i.started_at, i.network, i.location.trim(), i.energy_kwh, i.cost_cents, i.odometer_km, i.notes, i.external_id
    )
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>, Json(i): Json<ChargeInput>) -> AppResult<Json<Charge>> {
    i.validate()?;
    let row = sqlx::query_as!(
        Charge,
        r#"UPDATE charges SET started_at = $2, network = $3, location = $4, energy_kwh = $5, cost_cents = $6, odometer_km = $7, notes = $8
           WHERE id = $1
           RETURNING id, started_at, network, location, energy_kwh, cost_cents, odometer_km, source, external_id, notes"#,
        id, i.started_at, i.network, i.location.trim(), i.energy_kwh, i.cost_cents, i.odometer_km, i.notes
    )
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

pub async fn remove(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let res = sqlx::query!("DELETE FROM charges WHERE id = $1", id).execute(&s.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ImportInput {
    rows: Vec<ChargeInput>,
}

#[derive(Serialize)]
pub struct ImportResult {
    inserted: u64,
    skipped: u64,
}

/// Import en lot (CSV converti par le frontend). Les lignes dont l'`external_id`
/// existe déjà sont ignorées : on peut réimporter le même fichier sans doublons.
pub async fn import(_: AuthUser, State(s): State<AppState>, Json(input): Json<ImportInput>) -> AppResult<Json<ImportResult>> {
    if input.rows.len() > 5000 {
        return Err(bad_request("5000 lignes maximum par import"));
    }
    for (n, r) in input.rows.iter().enumerate() {
        r.validate().map_err(|e| match e {
            AppError::BadRequest(m) => bad_request(format!("Ligne {} : {m}", n + 1)),
            other => other,
        })?;
    }
    let mut tx = s.db.begin().await?;
    let mut inserted = 0;
    for r in &input.rows {
        let res = sqlx::query!(
            r#"INSERT INTO charges (started_at, network, location, energy_kwh, cost_cents, odometer_km, notes, external_id, source)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'import')
               ON CONFLICT (external_id) DO NOTHING"#,
            r.started_at, r.network, r.location.trim(), r.energy_kwh, r.cost_cents, r.odometer_km, r.notes, r.external_id
        )
        .execute(&mut *tx)
        .await?;
        inserted += res.rows_affected();
    }
    tx.commit().await?;
    Ok(Json(ImportResult { inserted, skipped: input.rows.len() as u64 - inserted }))
}
