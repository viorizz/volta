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
    pub leasing_residual_cents: Option<i64>,
    pub leasing_km_per_year: Option<i32>,
    pub leasing_excess_cents_per_km: Option<i64>,
    /// Valeur de revente estimée aujourd'hui (achat) : sert à la dépréciation.
    pub estimated_value_cents: Option<i64>,
    /// Estimer l'électricité rechargée à domicile depuis le kilométrage.
    pub estimate_home_charging: bool,
    pub consumption_kwh_per_100km: f64,
    pub charging_losses_pct: f64,
    pub monthly_budget_cents: Option<i64>,
}

pub async fn load(db: &sqlx::PgPool) -> anyhow::Result<Settings> {
    Ok(sqlx::query_as!(
        Settings,
        r#"SELECT vehicle_name, purchase_date, purchase_price_cents, initial_odometer_km, current_odometer_km,
                  currency, fuel_consumption_l_per_100km, fuel_price_cents_per_l, home_price_cents_per_kwh,
                  acquisition_type, leasing_down_payment_cents, leasing_monthly_cents, leasing_interest_rate,
                  leasing_start_date, leasing_months, leasing_residual_cents, leasing_km_per_year,
                  leasing_excess_cents_per_km, estimated_value_cents, estimate_home_charging,
                  consumption_kwh_per_100km, charging_losses_pct, monthly_budget_cents
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
    if !(i.consumption_kwh_per_100km.is_finite() && (5.0..50.0).contains(&i.consumption_kwh_per_100km)) {
        return Err(bad_request("Consommation électrique : entre 5 et 50 kWh/100 km"));
    }
    if !(i.charging_losses_pct.is_finite() && (0.0..50.0).contains(&i.charging_losses_pct)) {
        return Err(bad_request("Pertes de recharge : entre 0 et 50 %"));
    }
    if [i.leasing_residual_cents, i.leasing_excess_cents_per_km, i.estimated_value_cents, i.monthly_budget_cents]
        .iter()
        .flatten()
        .any(|v| *v < 0)
        || i.leasing_km_per_year.is_some_and(|k| k <= 0)
    {
        return Err(bad_request("Valeur négative ou nulle"));
    }
    let previous_odometer = load(&s.db).await?.current_odometer_km;
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
               leasing_residual_cents = $16, leasing_km_per_year = $17, leasing_excess_cents_per_km = $18,
               estimated_value_cents = $19, estimate_home_charging = $20, consumption_kwh_per_100km = $21,
               charging_losses_pct = $22, monthly_budget_cents = $23, updated_at = now()
           WHERE id = 1"#,
        i.vehicle_name.trim(), i.purchase_date, i.purchase_price_cents, i.initial_odometer_km, i.current_odometer_km,
        i.currency.to_uppercase(), i.fuel_consumption_l_per_100km, i.fuel_price_cents_per_l, i.home_price_cents_per_kwh,
        i.acquisition_type, i.leasing_down_payment_cents, i.leasing_monthly_cents, i.leasing_interest_rate,
        i.leasing_start_date, i.leasing_months, i.leasing_residual_cents, i.leasing_km_per_year,
        i.leasing_excess_cents_per_km, i.estimated_value_cents, i.estimate_home_charging, i.consumption_kwh_per_100km,
        i.charging_losses_pct, i.monthly_budget_cents
    )
    .execute(&s.db)
    .await?;
    // Un nouveau kilométrage saisi devient un relevé daté (sert aux distances par période).
    if let Some(km) = i.current_odometer_km.filter(|km| Some(*km) != previous_odometer) {
        sqlx::query!("INSERT INTO odometer_readings (read_at, km, source) VALUES (now(), $1, 'manual')", km)
            .execute(&s.db)
            .await?;
    }
    Ok(Json(load(&s.db).await?))
}

#[cfg(test)]
impl Settings {
    /// Réglages de test : achat, 7 L/100 km à 2.00, 16 kWh/100 km, 10 % de pertes.
    pub fn sample() -> Self {
        Settings {
            vehicle_name: "Test".into(),
            purchase_date: None,
            purchase_price_cents: None,
            initial_odometer_km: 0,
            current_odometer_km: None,
            currency: "CHF".into(),
            fuel_consumption_l_per_100km: 7.0,
            fuel_price_cents_per_l: 200,
            home_price_cents_per_kwh: 27,
            acquisition_type: "purchase".into(),
            leasing_down_payment_cents: None,
            leasing_monthly_cents: None,
            leasing_interest_rate: None,
            leasing_start_date: None,
            leasing_months: None,
            leasing_residual_cents: None,
            leasing_km_per_year: None,
            leasing_excess_cents_per_km: None,
            estimated_value_cents: None,
            estimate_home_charging: false,
            consumption_kwh_per_100km: 16.0,
            charging_losses_pct: 10.0,
            monthly_budget_cents: None,
        }
    }
}
