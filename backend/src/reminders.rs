//! Rappels d'entretien : à une date et/ou à un kilométrage, éventuellement récurrents.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Months, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthUser, error::bad_request, error::AppError, error::AppResult, odometer, settings, AppState};

/// Rappel affiché comme « bientôt » 30 jours ou 1 000 km avant l'échéance.
const SOON_DAYS: i64 = 30;
const SOON_KM: f64 = 1_000.0;

#[derive(Serialize)]
pub struct Reminder {
    id: Uuid,
    title: String,
    due_on: Option<NaiveDate>,
    due_km: Option<i32>,
    repeat_months: Option<i32>,
    repeat_km: Option<i32>,
    notes: String,
    last_done_on: Option<NaiveDate>,
    /// "overdue", "soon" ou "later".
    status: String,
    days_left: Option<i64>,
    km_left: Option<i64>,
}

#[derive(Deserialize)]
pub struct ReminderInput {
    title: String,
    due_on: Option<NaiveDate>,
    due_km: Option<i32>,
    repeat_months: Option<i32>,
    repeat_km: Option<i32>,
    #[serde(default)]
    notes: String,
}

impl ReminderInput {
    fn validate(&self) -> AppResult<()> {
        if self.title.trim().is_empty() || self.title.len() > 200 || self.notes.len() > 2000 {
            return Err(bad_request("Titre requis (200 caractères max)"));
        }
        if self.due_on.is_none() && self.due_km.is_none() {
            return Err(bad_request("Indique une date ou un kilométrage d'échéance"));
        }
        if self.due_km.is_some_and(|k| k < 0)
            || self.repeat_months.is_some_and(|m| !(1..=120).contains(&m))
            || self.repeat_km.is_some_and(|k| k <= 0)
        {
            return Err(bad_request("Valeurs invalides"));
        }
        Ok(())
    }
}

/// Statut d'un rappel : l'échéance la plus proche (date ou km) l'emporte.
pub fn status(due_on: Option<NaiveDate>, due_km: Option<i32>, today: NaiveDate, km: Option<f64>) -> (&'static str, Option<i64>, Option<i64>) {
    let days = due_on.map(|d| (d - today).num_days());
    let km_left = due_km.zip(km).map(|(d, k)| (d as f64 - k).round() as i64);
    let overdue = days.is_some_and(|d| d < 0) || km_left.is_some_and(|k| k < 0);
    let soon = days.is_some_and(|d| d <= SOON_DAYS) || km_left.is_some_and(|k| (k as f64) <= SOON_KM);
    (if overdue { "overdue" } else if soon { "soon" } else { "later" }, days, km_left)
}

async fn context(s: &AppState) -> anyhow::Result<(NaiveDate, Option<f64>)> {
    let tz = s.cfg.timezone.as_str();
    let today = sqlx::query_scalar!(r#"SELECT (now() AT TIME ZONE $1)::date AS "d!""#, tz).fetch_one(&s.db).await?;
    let settings = settings::load(&s.db).await?;
    let km = odometer::load(&s.db, &settings, tz).await?.last().map(|r| r.km);
    let km = km.into_iter().chain(settings.current_odometer_km.map(f64::from)).reduce(f64::max);
    Ok((today, km))
}

pub async fn list(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<Vec<Reminder>>> {
    let (today, km) = context(&s).await?;
    let rows = sqlx::query!("SELECT id, title, due_on, due_km, repeat_months, repeat_km, notes, last_done_on FROM reminders")
        .fetch_all(&s.db)
        .await?;
    let mut out: Vec<Reminder> = rows
        .into_iter()
        .map(|r| {
            let (st, days_left, km_left) = status(r.due_on, r.due_km, today, km);
            Reminder {
                id: r.id,
                title: r.title,
                due_on: r.due_on,
                due_km: r.due_km,
                repeat_months: r.repeat_months,
                repeat_km: r.repeat_km,
                notes: r.notes,
                last_done_on: r.last_done_on,
                status: st.into(),
                days_left,
                km_left,
            }
        })
        .collect();
    let rank = |s: &str| match s {
        "overdue" => 0,
        "soon" => 1,
        _ => 2,
    };
    out.sort_by_key(|r| (rank(&r.status), r.days_left.unwrap_or(i64::MAX).min(r.km_left.map(|k| k / 50).unwrap_or(i64::MAX))));
    Ok(Json(out))
}

pub async fn create(_: AuthUser, State(s): State<AppState>, Json(i): Json<ReminderInput>) -> AppResult<StatusCode> {
    i.validate()?;
    sqlx::query!(
        "INSERT INTO reminders (title, due_on, due_km, repeat_months, repeat_km, notes) VALUES ($1, $2, $3, $4, $5, $6)",
        i.title.trim(), i.due_on, i.due_km, i.repeat_months, i.repeat_km, i.notes
    )
    .execute(&s.db)
    .await?;
    Ok(StatusCode::CREATED)
}

pub async fn update(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>, Json(i): Json<ReminderInput>) -> AppResult<StatusCode> {
    i.validate()?;
    let res = sqlx::query!(
        "UPDATE reminders SET title = $2, due_on = $3, due_km = $4, repeat_months = $5, repeat_km = $6, notes = $7 WHERE id = $1",
        id, i.title.trim(), i.due_on, i.due_km, i.repeat_months, i.repeat_km, i.notes
    )
    .execute(&s.db)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let res = sqlx::query!("DELETE FROM reminders WHERE id = $1", id).execute(&s.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Prochaine échéance après avoir fait l'entretien aujourd'hui à `km`.
/// `None` si le rappel n'est pas récurrent (il est alors supprimé).
pub fn next_due(repeat_months: Option<i32>, repeat_km: Option<i32>, today: NaiveDate, km: Option<f64>) -> Option<(Option<NaiveDate>, Option<i32>)> {
    let date = repeat_months.and_then(|m| today.checked_add_months(Months::new(m as u32)));
    let due_km = repeat_km.zip(km).map(|(r, k)| (k.round() as i32).saturating_add(r));
    (date.is_some() || due_km.is_some()).then_some((date, due_km))
}

/// « Fait » : reprogramme un rappel récurrent, supprime un rappel unique.
pub async fn done(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let (today, km) = context(&s).await?;
    let r = sqlx::query!("SELECT repeat_months, repeat_km FROM reminders WHERE id = $1", id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if r.repeat_km.is_some() && km.is_none() {
        return Err(bad_request("Kilométrage actuel inconnu : saisis-le pour reprogrammer ce rappel"));
    }
    match next_due(r.repeat_months, r.repeat_km, today, km) {
        Some((due_on, due_km)) => {
            sqlx::query!("UPDATE reminders SET due_on = $2, due_km = $3, last_done_on = $4 WHERE id = $1", id, due_on, due_km, today)
                .execute(&s.db)
                .await?;
        }
        None => {
            sqlx::query!("DELETE FROM reminders WHERE id = $1", id).execute(&s.db).await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn status_uses_nearest_deadline() {
        let today = d(2026, 11, 12);
        assert_eq!(status(Some(d(2026, 11, 1)), None, today, None).0, "overdue");
        assert_eq!(status(Some(d(2026, 12, 1)), None, today, None).0, "soon");
        assert_eq!(status(Some(d(2027, 6, 1)), None, today, None).0, "later");
        // date lointaine mais kilométrage proche
        let (st, _, km) = status(Some(d(2027, 6, 1)), Some(30_000), today, Some(29_400.0));
        assert_eq!((st, km), ("soon", Some(600)));
        assert_eq!(status(None, Some(30_000), today, Some(30_100.0)).0, "overdue");
        // échéance en km sans kilométrage connu : pas d'alerte
        assert_eq!(status(None, Some(30_000), today, None).0, "later");
    }

    #[test]
    fn next_due_reschedules_or_ends() {
        let today = d(2026, 10, 20);
        assert_eq!(next_due(Some(12), None, today, None), Some((Some(d(2027, 10, 20)), None)));
        assert_eq!(next_due(None, Some(20_000), today, Some(31_234.4)), Some((None, Some(51_234))));
        assert_eq!(next_due(None, None, today, Some(1.0)), None);
        // récurrence en km sans kilométrage connu : rien à reprogrammer
        assert_eq!(next_due(None, Some(20_000), today, None), None);
    }
}
