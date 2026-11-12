use std::collections::BTreeMap;

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{Datelike, Months, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::{auth::AuthUser, error::AppResult, fixed_costs, odometer, odometer::Reading, settings, settings::Settings, AppState};

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
    /// Électricité maison estimée (si l'option est active).
    estimated_home_cents: i64,
    expense_cents: i64,
    fixed_cents: i64,
    distance_km: Option<f64>,
    fuel_equivalent_cents: Option<i64>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Comparison {
    /// Distance de la période en km.
    pub distance_km: Option<f64>,
    /// true si la distance est déduite des kWh faute de relevés du compteur.
    pub distance_is_estimate: bool,
    pub km_per_kwh: Option<f64>,
    /// false si l'efficacité sort de 3–10 km/kWh : historique de recharge incomplet.
    /// Toujours true quand la recharge maison est estimée (elle comble l'écart).
    pub efficiency_plausible: bool,
    /// kWh rechargés à domicile, estimés depuis la distance (0 si option inactive).
    pub estimated_home_kwh: f64,
    pub estimated_home_cents: i64,
    /// Recharges saisies + recharge maison estimée.
    pub electric_cost_cents: i64,
    pub fuel_cost_cents: Option<i64>,
    pub savings_cents: Option<i64>,
    pub electric_per_100km_cents: Option<i64>,
    pub fuel_per_100km_cents: i64,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct LeasingStatus {
    end_date: NaiveDate,
    km_driven: f64,
    km_allowed_total: f64,
    km_allowed_to_date: f64,
    /// Projection à la fin du contrat au rythme actuel (après 2 semaines de contrat).
    projected_km: Option<f64>,
    projected_excess_km: Option<f64>,
    projected_excess_cents: Option<i64>,
    residual_cents: Option<i64>,
}

#[derive(Serialize)]
pub struct Ownership {
    since: NaiveDate,
    months: f64,
    total_km: Option<f64>,
    /// Recharges, dépenses, frais fixes et leasing depuis le début.
    running_cents: i64,
    /// Achat : prix − valeur de revente estimée.
    depreciation_cents: Option<i64>,
    total_cents: i64,
    per_month_cents: i64,
    per_km_cents: Option<i64>,
}

#[derive(Serialize)]
pub struct Budget {
    month: String,
    budget_cents: i64,
    spent_cents: i64,
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
    leasing: Option<LeasingStatus>,
    ownership: Option<Ownership>,
    budget: Option<Budget>,
}

/// Coût essence pour une distance donnée, en centimes.
fn fuel_cost(s: &Settings, km: f64) -> i64 {
    (km * s.fuel_consumption_l_per_100km / 100.0 * s.fuel_price_cents_per_l as f64).round() as i64
}

/// kWh qu'il a fallu charger pour parcourir `km` (consommation + pertes).
fn needed_kwh(s: &Settings, km: f64) -> f64 {
    km * s.consumption_kwh_per_100km / 100.0 * (1.0 + s.charging_losses_pct / 100.0)
}

/// Recharge maison estimée : ce qui manque entre l'énergie nécessaire et l'énergie saisie.
fn home_estimate(s: &Settings, km: Option<f64>, recorded_kwh: f64) -> (f64, i64) {
    match km {
        Some(km) if s.estimate_home_charging => {
            let kwh = (needed_kwh(s, km) - recorded_kwh).max(0.0);
            (kwh, (kwh * s.home_price_cents_per_kwh as f64).round() as i64)
        }
        _ => (0.0, 0),
    }
}

pub struct CompareInput {
    /// kWh saisis depuis le début.
    pub all_kwh: f64,
    /// Kilométrage le plus haut connu.
    pub latest_km: Option<f64>,
    pub period_kwh: f64,
    pub period_cost: i64,
    pub whole_history: bool,
    /// Distance de la période d'après les relevés du compteur.
    pub odometer_km: Option<f64>,
}

/// Comparatif électrique / essence.
///
/// Distance : depuis le début, le compteur moins le kilométrage initial ; sur une
/// période, les relevés interpolés ; à défaut, une estimation par les kWh chargés
/// (efficacité moyenne réelle). La recharge maison n'est estimée que sur une
/// distance mesurée, jamais sur une distance elle-même déduite des kWh.
pub fn compare(s: &Settings, i: CompareInput) -> Comparison {
    let latest = i.latest_km.into_iter().chain(s.current_odometer_km.map(f64::from)).reduce(f64::max);
    let total_km = latest.map(|o| (o - s.initial_odometer_km as f64).max(0.0)).filter(|km| *km > 0.0);
    let km_per_kwh = total_km.filter(|_| i.all_kwh > 0.0).map(|km| km / i.all_kwh);

    let (distance_km, estimate) = if i.whole_history {
        (total_km, false)
    } else if let Some(d) = i.odometer_km.filter(|d| *d > 0.0) {
        (Some(d), false)
    } else {
        (km_per_kwh.map(|e| e * i.period_kwh), true)
    };
    let (home_kwh, home_cents) = home_estimate(s, distance_km.filter(|_| !estimate), i.period_kwh);
    let electric = i.period_cost + home_cents;
    let fuel_cost_cents = distance_km.map(|km| fuel_cost(s, km));
    Comparison {
        distance_km,
        distance_is_estimate: estimate,
        km_per_kwh,
        efficiency_plausible: s.estimate_home_charging || km_per_kwh.is_some_and(|e| (3.0..=10.0).contains(&e)),
        estimated_home_kwh: (home_kwh * 10.0).round() / 10.0,
        estimated_home_cents: home_cents,
        electric_cost_cents: electric,
        fuel_cost_cents,
        savings_cents: fuel_cost_cents.map(|f| f - electric),
        electric_per_100km_cents: distance_km.filter(|km| *km > 0.0).map(|km| (electric as f64 / km * 100.0).round() as i64),
        fuel_per_100km_cents: fuel_cost(s, 100.0),
    }
}

/// Suivi du kilométrage inclus dans le leasing.
pub fn leasing_status(s: &Settings, latest_km: Option<f64>, today: NaiveDate) -> Option<LeasingStatus> {
    if s.acquisition_type != "leasing" {
        return None;
    }
    let start = s.leasing_start_date?;
    let months = u32::try_from(s.leasing_months?).ok()?;
    let per_year = s.leasing_km_per_year? as f64;
    let end = start.checked_add_months(Months::new(months))?;
    let total_days = (end - start).num_days().max(1) as f64;
    let elapsed = (today.min(end) - start).num_days().max(0) as f64;
    let km_driven = latest_km.map(|k| (k - s.initial_odometer_km as f64).max(0.0)).unwrap_or(0.0);
    let allowed_total = per_year * months as f64 / 12.0;
    let projected = (elapsed >= 14.0).then(|| km_driven / elapsed * total_days);
    let excess = projected.map(|p| (p - allowed_total).max(0.0));
    Some(LeasingStatus {
        end_date: end,
        km_driven: km_driven.round(),
        km_allowed_total: allowed_total.round(),
        km_allowed_to_date: (allowed_total * elapsed / total_days).round(),
        projected_km: projected.map(f64::round),
        projected_excess_km: excess.map(f64::round),
        projected_excess_cents: excess.zip(s.leasing_excess_cents_per_km).map(|(e, c)| (e * c as f64).round() as i64),
        residual_cents: s.leasing_residual_cents,
    })
}

fn local_midnight(d: NaiveDate) -> NaiveDateTime {
    d.and_hms_opt(0, 0, 0).expect("minuit")
}

/// Distance du mois "YYYY-MM" d'après les relevés, arrêtée à `now`.
fn month_distance(r: &[Reading], month: &str, now: NaiveDateTime) -> Option<f64> {
    let start = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").ok()?;
    let end = local_midnight(start.checked_add_months(Months::new(1))?).min(now);
    odometer::distance(r, local_midnight(start), end)
}

pub async fn get(_: AuthUser, State(st): State<AppState>, Query(q): Query<StatsQuery>) -> AppResult<Json<Stats>> {
    let db = &st.db;
    let tz = st.cfg.timezone.as_str();
    let s = settings::load(db).await?;
    let now = sqlx::query_scalar!(r#"SELECT now() AT TIME ZONE $1 AS "d!""#, tz).fetch_one(db).await?;
    let today = now.date();

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

    let all_kwh = sqlx::query_scalar!(r#"SELECT COALESCE(SUM(energy_kwh), 0)::float8 AS "kwh!" FROM charges"#).fetch_one(db).await?;
    let readings = odometer::load(db, &s, tz).await?;
    let latest_km = readings.last().map(|r| r.km);

    let charge_cost_cents: i64 = by_network.iter().map(|g| g.cost_cents).sum();
    let energy_kwh: f64 = by_network.iter().map(|g| g.energy_kwh).sum();
    let charge_count: i64 = by_network.iter().map(|g| g.count).sum();
    let expense_cents: i64 = by_category.iter().map(|g| g.cost_cents).sum();
    let expense_count: i64 = by_category.iter().map(|g| g.count).sum();
    let comparison = compare(
        &s,
        CompareInput {
            all_kwh,
            latest_km,
            period_kwh: energy_kwh,
            period_cost: charge_cost_cents,
            whole_history: q.from.is_none(),
            odometer_km: q.from.and_then(|f| odometer::distance(&readings, local_midnight(f), now)),
        },
    );

    // Série mensuelle : recharges, dépenses, frais fixes, distance et équivalent essence.
    let mut months: BTreeMap<String, Month> = BTreeMap::new();
    fn month_mut<'a>(months: &'a mut BTreeMap<String, Month>, m: &str) -> &'a mut Month {
        months.entry(m.to_string()).or_insert_with(|| Month { month: m.to_string(), ..Default::default() })
    }
    for r in sqlx::query!(
        r#"SELECT to_char(started_at AT TIME ZONE $2, 'YYYY-MM') AS "m!", SUM(cost_cents)::bigint AS "cost!", SUM(energy_kwh)::float8 AS "kwh!"
           FROM charges WHERE ($1::date IS NULL OR (started_at AT TIME ZONE $2)::date >= $1) GROUP BY 1"#,
        q.from, tz
    )
    .fetch_all(db)
    .await?
    {
        let m = month_mut(&mut months, &r.m);
        m.charge_cost_cents = r.cost;
        m.energy_kwh = r.kwh;
    }
    for r in sqlx::query!(
        r#"SELECT to_char(spent_on, 'YYYY-MM') AS "m!", SUM(amount_cents)::bigint AS "cost!"
           FROM expenses WHERE ($1::date IS NULL OR spent_on >= $1) GROUP BY 1"#,
        q.from
    )
    .fetch_all(db)
    .await?
    {
        month_mut(&mut months, &r.m).expense_cents = r.cost;
    }

    // Frais fixes et leasing, mois par mois jusqu'à aujourd'hui.
    let fixed_list = fixed_costs::load_all(db).await?;
    let fixed = fixed_costs::accrue(&fixed_list, &s, q.from, today);
    let mut fixed_by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for (month, kinds) in &fixed {
        month_mut(&mut months, month).fixed_cents = kinds.values().sum();
        for (k, v) in kinds {
            *fixed_by_kind.entry(k.clone()).or_default() += v;
        }
    }
    let fixed_cents: i64 = fixed_by_kind.values().sum();
    let mut by_fixed: Vec<Group> = fixed_by_kind.into_iter().map(|(key, cost_cents)| Group { key, cost_cents, energy_kwh: 0.0, count: 0 }).collect();
    by_fixed.sort_by_key(|g| -g.cost_cents);

    for m in months.values_mut() {
        m.distance_km = month_distance(&readings, &m.month, now).map(f64::round);
        let km = m.distance_km.or_else(|| comparison.km_per_kwh.map(|e| e * m.energy_kwh));
        m.fuel_equivalent_cents = km.map(|km| fuel_cost(&s, km));
        m.estimated_home_cents = home_estimate(&s, m.distance_km, m.energy_kwh).1;
    }

    let total_cost_cents = charge_cost_cents + comparison.estimated_home_cents + expense_cents + fixed_cents;

    let ownership = if q.from.is_none() {
        let since = if s.acquisition_type == "leasing" { s.leasing_start_date } else { s.purchase_date }.or_else(|| readings.first().map(|r| r.at.date()));
        since.map(|since| {
            let days = (today - since).num_days().max(1) as f64;
            let months_owned = days / 30.4375;
            let depreciation = if s.acquisition_type == "purchase" {
                s.purchase_price_cents.zip(s.estimated_value_cents).map(|(p, v)| (p - v).max(0))
            } else {
                None
            };
            let total = total_cost_cents + depreciation.unwrap_or(0);
            Ownership {
                since,
                months: (months_owned * 10.0).round() / 10.0,
                total_km: comparison.distance_km,
                running_cents: total_cost_cents,
                depreciation_cents: depreciation,
                total_cents: total,
                per_month_cents: (total as f64 / months_owned.max(1.0)).round() as i64,
                per_km_cents: comparison.distance_km.filter(|k| *k > 0.0).map(|k| (total as f64 / k).round() as i64),
            }
        })
    } else {
        None
    };

    let budget = match s.monthly_budget_cents {
        Some(b) => {
            let month_from = today.with_day(1).expect("1er du mois");
            let spent = sqlx::query_scalar!(
                r#"SELECT (COALESCE((SELECT SUM(cost_cents) FROM charges WHERE (started_at AT TIME ZONE $2)::date >= $1), 0)
                         + COALESCE((SELECT SUM(amount_cents) FROM expenses WHERE spent_on >= $1), 0))::bigint AS "s!""#,
                month_from,
                tz
            )
            .fetch_one(db)
            .await?;
            let fixed_month: i64 = fixed_costs::accrue(&fixed_list, &s, Some(month_from), today).values().flat_map(|k| k.values()).sum();
            let month_kwh = sqlx::query_scalar!(
                r#"SELECT COALESCE(SUM(energy_kwh), 0)::float8 AS "k!" FROM charges WHERE (started_at AT TIME ZONE $2)::date >= $1"#,
                month_from,
                tz
            )
            .fetch_one(db)
            .await?;
            let home = home_estimate(&s, odometer::distance(&readings, local_midnight(month_from), now), month_kwh).1;
            Some(Budget { month: month_from.format("%Y-%m").to_string(), budget_cents: b, spent_cents: spent + fixed_month + home })
        }
        None => None,
    };

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
        total_cost_cents,
        purchase_price_cents: s.purchase_price_cents,
        leasing: leasing_status(&s, latest_km, today),
        comparison,
        monthly: months.into_values().collect(),
        ownership,
        budget,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { initial_odometer_km: 1_000, ..Settings::sample() }
    }
    fn input(all_kwh: f64, latest: Option<f64>, period_kwh: f64, cost: i64, whole: bool, odo: Option<f64>) -> CompareInput {
        CompareInput { all_kwh, latest_km: latest, period_kwh, period_cost: cost, whole_history: whole, odometer_km: odo }
    }
    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn whole_history_uses_odometer() {
        // 10 000 km, 1 800 kWh, 500 CHF d'électricité.
        let c = compare(&settings(), input(1_800.0, Some(11_000.0), 1_800.0, 50_000, true, None));
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
        let c = compare(&settings(), input(100.0, Some(11_000.0), 100.0, 3_000, true, None));
        assert!(!c.efficiency_plausible);
    }

    #[test]
    fn partial_period_prefers_odometer_then_kwh_estimate() {
        let c = compare(&settings(), input(2_000.0, Some(11_000.0), 200.0, 6_000, false, Some(1_200.0)));
        assert_eq!(c.distance_km, Some(1_200.0));
        assert!(!c.distance_is_estimate);
        // sans relevés : 5 km/kWh × 200 kWh = 1 000 km
        let c = compare(&settings(), input(2_000.0, Some(11_000.0), 200.0, 6_000, false, None));
        assert_eq!(c.distance_km, Some(1_000.0));
        assert!(c.distance_is_estimate);
        assert_eq!(c.fuel_cost_cents, Some(14_000));
    }

    #[test]
    fn settings_odometer_wins_when_higher() {
        let mut s = settings();
        s.current_odometer_km = Some(12_000);
        let c = compare(&s, input(1_000.0, Some(11_000.0), 1_000.0, 0, true, None));
        assert_eq!(c.distance_km, Some(11_000.0));
    }

    #[test]
    fn no_odometer_means_no_comparison() {
        let c = compare(&settings(), input(500.0, None, 500.0, 10_000, true, None));
        assert_eq!(c.distance_km, None);
        assert_eq!(c.savings_cents, None);
    }

    #[test]
    fn home_charging_fills_the_gap() {
        let mut s = settings();
        s.estimate_home_charging = true;
        // 10 000 km × 16 kWh/100 × 1.10 = 1 760 kWh nécessaires ; 300 saisis (Superchargers)
        let c = compare(&s, input(300.0, Some(11_000.0), 300.0, 15_000, true, None));
        assert_eq!(c.estimated_home_kwh, 1_460.0);
        assert_eq!(c.estimated_home_cents, 39_420); // 1 460 × 0.27
        assert_eq!(c.electric_cost_cents, 54_420);
        assert!(c.efficiency_plausible);
        // jamais d'estimation sur une distance déduite des kWh
        let c = compare(&s, input(300.0, Some(11_000.0), 50.0, 2_000, false, None));
        assert_eq!(c.estimated_home_cents, 0);
    }

    #[test]
    fn leasing_projection_and_excess() {
        let s = Settings {
            acquisition_type: "leasing".into(),
            leasing_start_date: Some(d(2026, 1, 1)),
            leasing_months: Some(48),
            leasing_km_per_year: Some(15_000),
            leasing_excess_cents_per_km: Some(15),
            initial_odometer_km: 0,
            ..Settings::sample()
        };
        // 10 000 km en 181 jours (au 1er juillet) : ~20 170 km/an
        let l = leasing_status(&s, Some(10_000.0), d(2026, 7, 1)).unwrap();
        assert_eq!(l.end_date, d(2030, 1, 1));
        assert_eq!(l.km_allowed_total, 60_000.0);
        let projected = l.projected_km.unwrap();
        assert!((projected - 10_000.0 / 181.0 * 1461.0).abs() < 1.0);
        let excess = l.projected_excess_km.unwrap();
        assert!((excess - (projected - 60_000.0)).abs() < 1.0);
        // 15 ct/km sur l'excédent non arrondi : à 15 ct près de l'excédent arrondi
        assert!((l.projected_excess_cents.unwrap() as f64 - excess * 15.0).abs() <= 15.0);
        // pas de projection les deux premières semaines
        assert_eq!(leasing_status(&s, Some(100.0), d(2026, 1, 5)).unwrap().projected_km, None);
        assert!(leasing_status(&Settings::sample(), Some(1.0), d(2026, 1, 5)).is_none());
    }
}
