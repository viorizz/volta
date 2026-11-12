//! Frais fixes (assurance, options, abonnements) et leasing : saisie et
//! répartition mois par mois pour les statistiques.

use std::collections::BTreeMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthUser, error::bad_request, error::AppError, error::AppResult, settings::Settings, AppState};

const KINDS: [&str; 5] = ["insurance", "vehicle_option", "charging_subscription", "tax", "other"];
const COVERAGES: [&str; 3] = ["liability", "partial", "full"];

#[derive(Serialize, Clone)]
pub struct FixedCost {
    pub id: Uuid,
    pub kind: String,
    pub label: String,
    pub amount_cents: i64,
    pub frequency: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub coverage: Option<String>,
    pub notes: String,
}

#[derive(Deserialize)]
pub struct FixedCostInput {
    pub kind: String,
    pub label: String,
    pub amount_cents: i64,
    pub frequency: String,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub coverage: Option<String>,
    #[serde(default)]
    pub notes: String,
}

impl FixedCostInput {
    async fn validate(&self, db: &sqlx::PgPool) -> AppResult<()> {
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(bad_request(format!("Type inconnu : {}", self.kind)));
        }
        if self.label.trim().is_empty() || self.label.len() > 200 || self.notes.len() > 2000 {
            return Err(bad_request("Libellé requis (200 caractères max)"));
        }
        if self.amount_cents < 0 {
            return Err(bad_request("Le montant ne peut pas être négatif"));
        }
        if self.frequency != "monthly" && self.frequency != "yearly" {
            return Err(bad_request("Fréquence : mensuelle ou annuelle"));
        }
        if self.end_date.is_some_and(|e| e < self.start_date) {
            return Err(bad_request("La date de fin précède la date de début"));
        }
        match (self.kind.as_str(), self.coverage.as_deref()) {
            ("insurance", Some(c)) if COVERAGES.contains(&c) => {
                let leasing = sqlx::query_scalar!("SELECT acquisition_type = 'leasing' FROM settings WHERE id = 1")
                    .fetch_one(db)
                    .await?
                    .unwrap_or(false);
                if leasing && c != "full" {
                    return Err(bad_request("Véhicule en leasing : la casco complète est obligatoire"));
                }
            }
            ("insurance", _) => return Err(bad_request("Assurance : choisis la couverture")),
            (_, Some(_)) => return Err(bad_request("La couverture ne concerne que l'assurance")),
            _ => {}
        }
        Ok(())
    }
}

pub async fn load_all(db: &sqlx::PgPool) -> anyhow::Result<Vec<FixedCost>> {
    Ok(sqlx::query_as!(
        FixedCost,
        r#"SELECT id, kind, label, amount_cents, frequency, start_date, end_date, coverage, notes
           FROM fixed_costs ORDER BY (end_date IS NULL OR end_date >= CURRENT_DATE) DESC, kind, start_date DESC"#
    )
    .fetch_all(db)
    .await?)
}

pub async fn list(_: AuthUser, State(s): State<AppState>) -> AppResult<Json<Vec<FixedCost>>> {
    Ok(Json(load_all(&s.db).await?))
}

pub async fn create(_: AuthUser, State(s): State<AppState>, Json(i): Json<FixedCostInput>) -> AppResult<(StatusCode, Json<FixedCost>)> {
    i.validate(&s.db).await?;
    let row = sqlx::query_as!(
        FixedCost,
        r#"INSERT INTO fixed_costs (kind, label, amount_cents, frequency, start_date, end_date, coverage, notes)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id, kind, label, amount_cents, frequency, start_date, end_date, coverage, notes"#,
        i.kind, i.label.trim(), i.amount_cents, i.frequency, i.start_date, i.end_date, i.coverage, i.notes
    )
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>, Json(i): Json<FixedCostInput>) -> AppResult<Json<FixedCost>> {
    i.validate(&s.db).await?;
    let row = sqlx::query_as!(
        FixedCost,
        r#"UPDATE fixed_costs SET kind = $2, label = $3, amount_cents = $4, frequency = $5, start_date = $6,
               end_date = $7, coverage = $8, notes = $9
           WHERE id = $1
           RETURNING id, kind, label, amount_cents, frequency, start_date, end_date, coverage, notes"#,
        id, i.kind, i.label.trim(), i.amount_cents, i.frequency, i.start_date, i.end_date, i.coverage, i.notes
    )
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

pub async fn remove(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let res = sqlx::query!("DELETE FROM fixed_costs WHERE id = $1", id).execute(&s.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Répartition mensuelle ----------

fn month_index(d: NaiveDate) -> i32 {
    d.year() * 12 + d.month0() as i32
}
fn month_key(idx: i32) -> String {
    format!("{:04}-{:02}", idx.div_euclid(12), idx.rem_euclid(12) + 1)
}

/// Montants par mois ("YYYY-MM") et par type, du mois de `from` (ou du début)
/// jusqu'au mois de `today` inclus. Un frais est compté chaque mois de son
/// mois de début à son mois de fin. Un montant annuel est réparti sur 12 mois
/// sans perte d'arrondi (les 12 parts somment exactement au montant annuel).
/// Le leasing compte l'acompte le mois du début, puis une mensualité par mois
/// pendant la durée du contrat.
pub fn accrue(costs: &[FixedCost], s: &Settings, from: Option<NaiveDate>, today: NaiveDate) -> BTreeMap<String, BTreeMap<String, i64>> {
    let first = from.map(month_index).unwrap_or(i32::MIN);
    let last = month_index(today);
    let mut out: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut add = |idx: i32, kind: &str, cents: i64| {
        if idx >= first && idx <= last && cents != 0 {
            *out.entry(month_key(idx)).or_default().entry(kind.to_string()).or_default() += cents;
        }
    };

    for c in costs {
        let start = month_index(c.start_date);
        let end = c.end_date.map(month_index).unwrap_or(last).min(last);
        for idx in start.max(first)..=end {
            let cents = if c.frequency == "yearly" {
                let k = (idx - start).rem_euclid(12) as i64 + 1;
                c.amount_cents * k / 12 - c.amount_cents * (k - 1) / 12
            } else {
                c.amount_cents
            };
            add(idx, &c.kind, cents);
        }
    }

    if s.acquisition_type == "leasing" {
        if let (Some(start), Some(months)) = (s.leasing_start_date, s.leasing_months) {
            let start = month_index(start);
            add(start, "leasing", s.leasing_down_payment_cents.unwrap_or(0));
            for i in 0..months {
                add(start + i, "leasing", s.leasing_monthly_cents.unwrap_or(0));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn settings() -> Settings {
        Settings::sample()
    }
    fn cost(kind: &str, amount: i64, freq: &str, start: NaiveDate, end: Option<NaiveDate>) -> FixedCost {
        FixedCost {
            id: Uuid::nil(),
            kind: kind.into(),
            label: "x".into(),
            amount_cents: amount,
            frequency: freq.into(),
            start_date: start,
            end_date: end,
            coverage: None,
            notes: String::new(),
        }
    }
    fn total(m: &BTreeMap<String, BTreeMap<String, i64>>) -> i64 {
        m.values().flat_map(|k| k.values()).sum()
    }

    #[test]
    fn yearly_amount_spread_exactly_over_twelve_months() {
        // 1 450.00 par an, du 1er mars 2025 au 28 février 2026.
        let c = cost("insurance", 145_000, "yearly", d(2025, 3, 1), Some(d(2026, 2, 28)));
        let m = accrue(&[c], &settings(), None, d(2026, 11, 12));
        assert_eq!(m.len(), 12);
        assert_eq!(total(&m), 145_000);
        assert_eq!(m["2025-03"]["insurance"], 12_083);
    }

    #[test]
    fn monthly_subscription_runs_until_today_and_respects_period() {
        let c = cost("vehicle_option", 9_900, "monthly", d(2026, 1, 15), None);
        let all = accrue(&[c.clone()], &settings(), None, d(2026, 11, 12));
        assert_eq!(total(&all), 11 * 9_900); // janvier à novembre
        let this_month = accrue(&[c], &settings(), Some(d(2026, 11, 1)), d(2026, 11, 12));
        assert_eq!(total(&this_month), 9_900);
    }

    #[test]
    fn leasing_counts_down_payment_then_monthly_until_term() {
        let mut s = settings();
        s.acquisition_type = "leasing".into();
        s.leasing_down_payment_cents = Some(500_000);
        s.leasing_monthly_cents = Some(45_000);
        s.leasing_start_date = Some(d(2026, 9, 20));
        s.leasing_months = Some(48);
        let m = accrue(&[], &s, None, d(2026, 11, 12));
        // acompte + septembre, octobre, novembre
        assert_eq!(total(&m), 500_000 + 3 * 45_000);
        assert_eq!(m["2026-09"]["leasing"], 545_000);
        // un contrat terminé ne compte plus
        let later = accrue(&[], &s, Some(d(2031, 1, 1)), d(2031, 6, 1));
        assert_eq!(total(&later), 0);
    }

    #[test]
    fn purchase_ignores_leasing_fields() {
        let mut s = settings();
        s.leasing_monthly_cents = Some(45_000);
        s.leasing_start_date = Some(d(2026, 1, 1));
        s.leasing_months = Some(12);
        assert!(accrue(&[], &s, None, d(2026, 11, 12)).is_empty());
    }
}
