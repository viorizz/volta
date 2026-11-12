use axum::{extract::State, Json};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{auth::AuthUser, error::bad_request, error::AppResult, AppState};

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub vehicle_name: String,
    pub purchase_date: Option<NaiveDate>,
    pub purchase_price_cents: Option<i64>,
    pub initial_odometer_km: i32,
    pub current_odometer_km: Option<i32>,
    pub currency: String,
    pub fuel_consumption_l_per_100km: f64,
    pub fuel_price_cents_per_l: i64,
    pub home_price_cents_per_kwh: i64,
}

pub async fn load(db: &sqlx::PgPool) -> anyhow::Result<Settings> {
    Ok(sqlx::query_as!(
        Settings,
        r#"SELECT vehicle_name, purchase_date, purchase_price_cents, initial_odometer_km, current_odometer_km,
                  currency, fuel_consumption_l_per_100km, fuel_price_cents_per_l, home_price_cents_per_kwh
           FROM settings WHERE id = 1"#
    )
    .fetch_one(db)
    .await?)
}

pub async fn get(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<Settings>> {
    Ok(Json(load(&s.db).await?))
}

pub async fn put(_: AuthUser, State(s): State<AppState>, Json(i): Json<Settings>) -> AppResult<Json<Settings>> {
    if i.vehicle_name.trim().is_empty() || i.vehicle_name.len() > 100 {
        return Err(bad_request("Nom du véhicule requis"));
    }
    if i.currency.len() != 3 {
        return Err(bad_request("Devise : code à 3 lettres (CHF, EUR…)"));
    }
    if !(i.fuel_consumption_l_per_100km.is_finite() && i.fuel_consumption_l_per_100km > 0.0 && i.fuel_consumption_l_per_100km < 50.0) {
        return Err(bad_request("Consommation essence invalide"));
    }
    if i.initial_odometer_km < 0
        || i.current_odometer_km.is_some_and(|c| c < i.initial_odometer_km)
        || i.fuel_price_cents_per_l < 0
        || i.home_price_cents_per_kwh < 0
        || i.purchase_price_cents.is_some_and(|p| p < 0)
    {
        return Err(bad_request("Valeur négative ou kilométrage actuel inférieur au kilométrage initial"));
    }
    sqlx::query!(
        r#"UPDATE settings SET vehicle_name = $1, purchase_date = $2, purchase_price_cents = $3, initial_odometer_km = $4,
               current_odometer_km = $5, currency = $6, fuel_consumption_l_per_100km = $7, fuel_price_cents_per_l = $8,
               home_price_cents_per_kwh = $9, updated_at = now()
           WHERE id = 1"#,
        i.vehicle_name.trim(), i.purchase_date, i.purchase_price_cents, i.initial_odometer_km, i.current_odometer_km,
        i.currency.to_uppercase(), i.fuel_consumption_l_per_100km, i.fuel_price_cents_per_l, i.home_price_cents_per_kwh
    )
    .execute(&s.db)
    .await?;
    Ok(Json(load(&s.db).await?))
}
