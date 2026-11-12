//! Relevés du compteur : interpolation pour connaître la distance parcourue
//! sur n'importe quelle période (mois, année), en heure locale.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{NaiveDateTime, Utc};
#[cfg(test)]
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{auth::AuthUser, error::bad_request, error::AppResult, settings::Settings, AppState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub at: NaiveDateTime,
    pub km: f64,
}

/// Tous les kilométrages connus, en heure locale, triés et rendus croissants
/// (une saisie erronée plus basse qu'un relevé précédent est ignorée).
pub async fn load(db: &sqlx::PgPool, s: &Settings, tz: &str) -> anyhow::Result<Vec<Reading>> {
    let rows = sqlx::query!(
        r#"SELECT read_at AT TIME ZONE $1 AS "at!", km::float8 AS "km!" FROM odometer_readings
           UNION ALL SELECT started_at AT TIME ZONE $1, odometer_km::float8 FROM charges WHERE odometer_km IS NOT NULL
           UNION ALL SELECT spent_on + time '12:00', odometer_km::float8 FROM expenses WHERE odometer_km IS NOT NULL"#,
        tz
    )
    .fetch_all(db)
    .await?;
    let mut v: Vec<Reading> = rows.into_iter().map(|r| Reading { at: r.at, km: r.km }).collect();
    let start = if s.acquisition_type == "leasing" { s.leasing_start_date } else { s.purchase_date };
    if let Some(d) = start {
        v.push(Reading { at: d.and_hms_opt(0, 0, 0).expect("minuit"), km: s.initial_odometer_km as f64 });
    }
    Ok(normalize(v))
}

pub fn normalize(mut v: Vec<Reading>) -> Vec<Reading> {
    v.sort_by(|a, b| a.at.cmp(&b.at).then(a.km.total_cmp(&b.km)));
    let mut out: Vec<Reading> = Vec::with_capacity(v.len());
    for r in v {
        if out.last().is_none_or(|l| r.km >= l.km) {
            out.push(r);
        }
    }
    out
}

/// Kilométrage interpolé à l'instant `t` ; constant avant le premier et après le dernier relevé.
pub fn km_at(r: &[Reading], t: NaiveDateTime) -> Option<f64> {
    let first = r.first()?;
    let last = r.last()?;
    if t <= first.at {
        return Some(first.km);
    }
    if t >= last.at {
        return Some(last.km);
    }
    let i = r.partition_point(|x| x.at <= t);
    let (a, b) = (r[i - 1], r[i]);
    let span = (b.at - a.at).num_seconds() as f64;
    let frac = if span > 0.0 { (t - a.at).num_seconds() as f64 / span } else { 1.0 };
    Some(a.km + (b.km - a.km) * frac)
}

/// Distance entre `a` et `b`, seulement si les relevés couvrent le début de la
/// période (sinon on sous-estimerait) et en contiennent au moins un après `a`.
pub fn distance(r: &[Reading], a: NaiveDateTime, b: NaiveDateTime) -> Option<f64> {
    let first = r.first()?;
    if first.at > a || !r.iter().any(|x| x.at > a) {
        return None;
    }
    Some((km_at(r, b)? - km_at(r, a)?).max(0.0))
}

// ---------- Saisie manuelle ----------

#[derive(Serialize)]
pub struct ReadingOut {
    at: chrono::DateTime<Utc>,
    km: i32,
    source: String,
}

pub async fn list(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<Vec<ReadingOut>>> {
    let rows = sqlx::query_as!(ReadingOut, r#"SELECT read_at AS at, km, source FROM odometer_readings ORDER BY read_at DESC LIMIT 50"#)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct ReadingInput {
    km: i32,
}

pub async fn create(_: AuthUser, State(s): State<AppState>, Json(i): Json<ReadingInput>) -> AppResult<StatusCode> {
    if i.km < 0 {
        return Err(bad_request("Kilométrage invalide"));
    }
    sqlx::query!("INSERT INTO odometer_readings (read_at, km, source) VALUES (now(), $1, 'manual')", i.km)
        .execute(&s.db)
        .await?;
    Ok(StatusCode::CREATED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(0, 0, 0).unwrap()
    }

    #[test]
    fn interpolates_and_clamps() {
        let r = normalize(vec![
            Reading { at: at(2026, 3, 1), km: 2_000.0 },
            Reading { at: at(2026, 1, 1), km: 1_000.0 },
            Reading { at: at(2026, 2, 1), km: 500.0 }, // erreur de saisie : ignorée
        ]);
        assert_eq!(r.len(), 2);
        // 1er février : 31 jours sur 59 entre les deux relevés
        let feb = km_at(&r, at(2026, 2, 1)).unwrap();
        assert!((feb - (1_000.0 + 1_000.0 * 31.0 / 59.0)).abs() < 0.01);
        assert_eq!(km_at(&r, at(2025, 6, 1)), Some(1_000.0));
        assert_eq!(km_at(&r, at(2027, 1, 1)), Some(2_000.0));
    }

    #[test]
    fn distance_requires_coverage_of_period_start() {
        let r = normalize(vec![Reading { at: at(2026, 1, 1), km: 1_000.0 }, Reading { at: at(2026, 3, 1), km: 2_000.0 }]);
        assert_eq!(distance(&r, at(2026, 1, 1), at(2026, 3, 1)), Some(1_000.0));
        assert_eq!(distance(&r, at(2025, 12, 1), at(2026, 3, 1)), None);
        assert_eq!(distance(&r, at(2026, 3, 1), at(2026, 4, 1)), None);
    }
}
