use std::collections::BTreeMap;

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{auth::AuthUser, error::AppResult, fixed_costs, settings, settings::Settings, AppState};

#[derive(Deserialize)]
pub struct StatsQuery {
    /// Début de période (inclus). Absent = depuis le début.
    from: Option<NaiveDate>,
}

#[derive(Serialize)]
pub struct Group {
    key: String,
    cost_cents: i64,
    energy_kwh: f64,
    count: i64,
}

#[derive(Serialize, Default)]
pub struct Month {
    month: String,
    charge_cost_cents: i64,
    energy_kwh: f64,
    expense_cents: i64,
    fixed_cents: i64,
    fuel_equivalent_cents: Option<i64>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Comparison {
    /// Distance de la période en km.
    pub distance_km: Option<f64>,
    /// true si la distance est déduite des kWh (période partielle).
    pub distance_is_estimate: bool,
    pub km_per_kwh: Option<f64>,
    /// false si l'efficacité sort de 3–10 km/kWh : historique de recharge incomplet
    /// (recharges à domicile non saisies, compteur faux…). Le comparatif est alors faussé.
    pub efficiency_plausible: bool,
    pub electric_cost_cents: i64,
    pub fuel_cost_cents: Option<i64>,
    pub savings_cents: Option<i64>,
    pub electric_per_100km_cents: Option<i64>,
    pub fuel_per_100km_cents: i64,
}

#[derive(Serialize)]
pub struct Stats {
    currency: String,
    from: Option<NaiveDate>,
    charge_cost_cents: i64,
    energy_kwh: f64,
    charge_count: i64,
    by_network: Vec<Group>,
    expense_cents: i64,
    expense_count: i64,
    by_category: Vec<Group>,
    /// Frais fixes et leasing répartis sur la période.
    fixed_cents: i64,
    by_fixed: Vec<Group>,
    total_cost_cents: i64,
    purchase_price_cents: Option<i64>,
    comparison: Comparison,
    monthly: Vec<Month>,
}

/// Coût essence pour une distance donnée, en centimes.
fn fuel_cost(s: &Settings, km: f64) -> i64 {
    (km * s.fuel_consumption_l_per_100km / 100.0 * s.fuel_price_cents_per_l as f64).round() as i64
}

/// Comparatif électrique / essence.
///
/// La distance totale vient du compteur (kilométrage le plus haut connu moins le
/// kilométrage à l'achat). Pour une période partielle, on ne connaît pas les km
/// parcourus : on les estime avec l'efficacité moyenne réelle (km par kWh chargé).
pub fn compare(s: &Settings, all_kwh: f64, max_odometer: Option<i32>, period_kwh: f64, period_cost: i64, whole_history: bool) -> Comparison {
    let latest = max_odometer.into_iter().chain(s.current_odometer_km).max();
    let total_km = latest.map(|o| (o - s.initial_odometer_km).max(0) as f64).filter(|km| *km > 0.0);
    let km_per_kwh = total_km.filter(|_| all_kwh > 0.0).map(|km| km / all_kwh);

    let (distance_km, estimate) = if whole_history {
        (total_km, false)
    } else {
        (km_per_kwh.map(|e| e * period_kwh), true)
    };
    let fuel_cost_cents = distance_km.map(|km| fuel_cost(s, km));
    Comparison {
        distance_km,
        distance_is_estimate: estimate,
        km_per_kwh,
        efficiency_plausible: km_per_kwh.is_some_and(|e| (3.0..=10.0).contains(&e)),
        electric_cost_cents: period_cost,
        fuel_cost_cents,
        savings_cents: fuel_cost_cents.map(|f| f - period_cost),
        electric_per_100km_cents: distance_km.filter(|km| *km > 0.0).map(|km| (period_cost as f64 / km * 100.0).round() as i64),
        fuel_per_100km_cents: fuel_cost(s, 100.0),
    }
}

pub async fn get(_: AuthUser, State(st): State<AppState>, Query(q): Query<StatsQuery>) -> AppResult<Json<Stats>> {
    let db = &st.db;
    let tz = st.cfg.timezone.as_str();
    let s = settings::load(db).await?;

    let net = sqlx::query!(
        r#"SELECT network, COALESCE(SUM(cost_cents), 0)::bigint AS "cost!", COALESCE(SUM(energy_kwh), 0)::float8 AS "kwh!", COUNT(*) AS "n!"
           FROM charges WHERE ($1::date IS NULL OR (started_at AT TIME ZONE $2)::date >= $1)
           GROUP BY network ORDER BY 2 DESC"#,
        q.from, tz
    )
    .fetch_all(db)
    .await?;
    let by_network: Vec<Group> = net.into_iter().map(|r| Group { key: r.network, cost_cents: r.cost, energy_kwh: r.kwh, count: r.n }).collect();

    let cat = sqlx::query!(
        r#"SELECT category, COALESCE(SUM(amount_cents), 0)::bigint AS "cost!", COUNT(*) AS "n!"
           FROM expenses WHERE ($1::date IS NULL OR spent_on >= $1)
           GROUP BY category ORDER BY 2 DESC"#,
        q.from
    )
    .fetch_all(db)
    .await?;
    let by_category: Vec<Group> = cat.into_iter().map(|r| Group { key: r.category, cost_cents: r.cost, energy_kwh: 0.0, count: r.n }).collect();

    let all = sqlx::query!(
        r#"SELECT COALESCE((SELECT SUM(energy_kwh) FROM charges), 0)::float8 AS "kwh!",
                  GREATEST((SELECT MAX(odometer_km) FROM charges), (SELECT MAX(odometer_km) FROM expenses)) AS max_odo"#
    )
    .fetch_one(db)
    .await?;

    let charge_cost_cents: i64 = by_network.iter().map(|g| g.cost_cents).sum();
    let energy_kwh: f64 = by_network.iter().map(|g| g.energy_kwh).sum();
    let charge_count: i64 = by_network.iter().map(|g| g.count).sum();
    let expense_cents: i64 = by_category.iter().map(|g| g.cost_cents).sum();
    let expense_count: i64 = by_category.iter().map(|g| g.count).sum();
    let comparison = compare(&s, all.kwh, all.max_odo, energy_kwh, charge_cost_cents, q.from.is_none());

    // Série mensuelle : recharges et dépenses fusionnées par mois.
    let mut months: BTreeMap<String, Month> = BTreeMap::new();
    for r in sqlx::query!(
        r#"SELECT to_char(started_at AT TIME ZONE $2, 'YYYY-MM') AS "m!", SUM(cost_cents)::bigint AS "cost!", SUM(energy_kwh)::float8 AS "kwh!"
           FROM charges WHERE ($1::date IS NULL OR (started_at AT TIME ZONE $2)::date >= $1) GROUP BY 1"#,
        q.from, tz
    )
    .fetch_all(db)
    .await?
    {
        let m = months.entry(r.m.clone()).or_insert_with(|| Month { month: r.m, ..Default::default() });
        m.charge_cost_cents = r.cost;
        m.energy_kwh = r.kwh;
        m.fuel_equivalent_cents = comparison.km_per_kwh.map(|e| fuel_cost(&s, e * r.kwh));
    }
    for r in sqlx::query!(
        r#"SELECT to_char(spent_on, 'YYYY-MM') AS "m!", SUM(amount_cents)::bigint AS "cost!"
           FROM expenses WHERE ($1::date IS NULL OR spent_on >= $1) GROUP BY 1"#,
        q.from
    )
    .fetch_all(db)
    .await?
    {
        months.entry(r.m.clone()).or_insert_with(|| Month { month: r.m, ..Default::default() }).expense_cents = r.cost;
    }

    // Frais fixes et leasing, mois par mois jusqu'à aujourd'hui (fuseau de l'app).
    let today = sqlx::query_scalar!(r#"SELECT (now() AT TIME ZONE $1)::date AS "d!""#, tz).fetch_one(db).await?;
    let fixed = fixed_costs::accrue(&fixed_costs::load_all(db).await?, &s, q.from, today);
    let mut fixed_by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for (month, kinds) in &fixed {
        let sum: i64 = kinds.values().sum();
        months.entry(month.clone()).or_insert_with(|| Month { month: month.clone(), ..Default::default() }).fixed_cents = sum;
        for (k, v) in kinds {
            *fixed_by_kind.entry(k.clone()).or_default() += v;
        }
    }
    let fixed_cents: i64 = fixed_by_kind.values().sum();
    let mut by_fixed: Vec<Group> = fixed_by_kind
        .into_iter()
        .map(|(key, cost_cents)| Group { key, cost_cents, energy_kwh: 0.0, count: 0 })
        .collect();
    by_fixed.sort_by_key(|g| -g.cost_cents);

    Ok(Json(Stats {
        currency: s.currency.clone(),
        from: q.from,
        charge_cost_cents,
        energy_kwh,
        charge_count,
        by_network,
        expense_cents,
        expense_count,
        by_category,
        fixed_cents,
        by_fixed,
        total_cost_cents: charge_cost_cents + expense_cents + fixed_cents,
        purchase_price_cents: s.purchase_price_cents,
        comparison,
        monthly: months.into_values().collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            vehicle_name: "Test".into(),
            purchase_date: None,
            purchase_price_cents: None,
            initial_odometer_km: 1_000,
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
        }
    }

    #[test]
    fn whole_history_uses_odometer() {
        // 10 000 km, 1 800 kWh, 500 CHF d'électricité.
        let c = compare(&settings(), 1_800.0, Some(11_000), 1_800.0, 50_000, true);
        assert_eq!(c.distance_km, Some(10_000.0));
        assert!(!c.distance_is_estimate);
        // 10 000 km × 7 L/100 × 2.00 CHF = 1 400 CHF
        assert_eq!(c.fuel_cost_cents, Some(140_000));
        assert_eq!(c.savings_cents, Some(90_000));
        assert_eq!(c.electric_per_100km_cents, Some(500));
        assert_eq!(c.fuel_per_100km_cents, 1_400);
        assert!(c.efficiency_plausible);
    }

    #[test]
    fn missing_charges_flag_implausible_efficiency() {
        // 10 000 km pour seulement 100 kWh saisis : 100 km/kWh, impossible.
        let c = compare(&settings(), 100.0, Some(11_000), 100.0, 3_000, true);
        assert!(!c.efficiency_plausible);
    }

    #[test]
    fn partial_period_estimates_distance_from_kwh() {
        let c = compare(&settings(), 2_000.0, Some(11_000), 200.0, 6_000, false);
        // 5 km/kWh × 200 kWh = 1 000 km
        assert_eq!(c.distance_km, Some(1_000.0));
        assert!(c.distance_is_estimate);
        assert_eq!(c.fuel_cost_cents, Some(14_000));
    }

    #[test]
    fn settings_odometer_wins_when_higher() {
        let mut s = settings();
        s.current_odometer_km = Some(12_000);
        let c = compare(&s, 1_000.0, Some(11_000), 1_000.0, 0, true);
        assert_eq!(c.distance_km, Some(11_000.0));
    }

    #[test]
    fn no_odometer_means_no_comparison() {
        let c = compare(&settings(), 500.0, None, 500.0, 10_000, true);
        assert_eq!(c.distance_km, None);
        assert_eq!(c.savings_cents, None);
    }
}
