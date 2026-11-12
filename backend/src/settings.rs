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
    /// "purchase" (achat) ou "leasing".
    pub acquisition_type: String,
    pub leasing_down_payment_cents: Option<i64>,
    pub leasing_monthly_cents: Option<i64>,
    /// Taux annuel en pour cent (informatif : la mensualité l'inclut déjà).
    pub leasing_interest_rate: Option<f64>,
    pub leasing_start_date: Option<NaiveDate>,
    pub leasing_months: Option<i32>,
}

pub async fn load(db: &sqlx::PgPool) -> anyhow::Result<Settings> {
    Ok(sqlx::query_as!(
        Settings,
        r#"SELECT vehicle_name, purchase_date, purchase_price_cents, initial_odometer_km, current_odometer_km,
                  currency, fuel_consumption_l_per_100km, fuel_price_cents_per_l, home_price_cents_per_kwh,
                  acquisition_type, leasing_down_payment_cents, leasing_monthly_cents, leasing_interest_rate,
                  leasing_start_date, leasing_months
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
    let leasing = match i.acquisition_type.as_str() {
        "purchase" => false,
        "leasing" => true,
        _ => return Err(bad_request("Mode d'acquisition inconnu")),
    };
    if leasing {
        if i.leasing_monthly_cents.is_none() || i.leasing_start_date.is_none() || i.leasing_months.is_none() {
            return Err(bad_request("Leasing : mensualité, date de début et durée sont requises"));
        }
        if i.leasing_months.is_some_and(|m| !(1..=120).contains(&m))
            || i.leasing_interest_rate.is_some_and(|r| !(0.0..50.0).contains(&r))
            || i.leasing_monthly_cents.is_some_and(|m| m < 0)
            || i.leasing_down_payment_cents.is_some_and(|d| d < 0)
        {
            return Err(bad_request("Leasing : valeurs invalides (durée 1 à 120 mois, taux 0 à 50 %)"));
        }
        let partial = sqlx::query_scalar!(
            "SELECT count(*) FROM fixed_costs WHERE kind = 'insurance' AND coverage <> 'full' AND (end_date IS NULL OR end_date >= CURRENT_DATE)"
        )
        .fetch_one(&s.db)
        .await?
        .unwrap_or(0);
        if partial > 0 {
            return Err(bad_request(
                "Un leasing exige une casco complète : modifie d'abord l'assurance en cours (casco partielle ou RC seule)",
            ));
        }
    }
    sqlx::query!(
        r#"UPDATE settings SET vehicle_name = $1, purchase_date = $2, purchase_price_cents = $3, initial_odometer_km = $4,
               current_odometer_km = $5, currency = $6, fuel_consumption_l_per_100km = $7, fuel_price_cents_per_l = $8,
               home_price_cents_per_kwh = $9, acquisition_type = $10, leasing_down_payment_cents = $11,
               leasing_monthly_cents = $12, leasing_interest_rate = $13, leasing_start_date = $14, leasing_months = $15,
               updated_at = now()
           WHERE id = 1"#,
        i.vehicle_name.trim(), i.purchase_date, i.purchase_price_cents, i.initial_odometer_km, i.current_odometer_km,
        i.currency.to_uppercase(), i.fuel_consumption_l_per_100km, i.fuel_price_cents_per_l, i.home_price_cents_per_kwh,
        i.acquisition_type, i.leasing_down_payment_cents, i.leasing_monthly_cents, i.leasing_interest_rate,
        i.leasing_start_date, i.leasing_months
    )
    .execute(&s.db)
    .await?;
    Ok(Json(load(&s.db).await?))
}
