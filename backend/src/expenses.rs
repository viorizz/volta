use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthUser, error::bad_request, error::AppError, error::AppResult, AppState};

const CATEGORIES: [&str; 7] = ["tires", "maintenance", "insurance", "tax", "accessories", "parking", "other"];

#[derive(Serialize)]
pub struct Expense {
    pub id: Uuid,
    pub spent_on: NaiveDate,
    pub category: String,
    pub label: String,
    pub amount_cents: i64,
    pub odometer_km: Option<i32>,
    pub notes: String,
}

#[derive(Deserialize)]
pub struct ExpenseInput {
    pub spent_on: NaiveDate,
    pub category: String,
    pub label: String,
    pub amount_cents: i64,
    pub odometer_km: Option<i32>,
    #[serde(default)]
    pub notes: String,
}

impl ExpenseInput {
    fn validate(&self) -> AppResult<()> {
        if !CATEGORIES.contains(&self.category.as_str()) {
            return Err(bad_request(format!("Catégorie inconnue : {}", self.category)));
        }
        if self.label.trim().is_empty() || self.label.len() > 200 || self.notes.len() > 2000 {
            return Err(bad_request("Libellé requis (200 caractères max)"));
        }
        if self.amount_cents < 0 {
            return Err(bad_request("Le montant ne peut pas être négatif"));
        }
        if self.odometer_km.is_some_and(|o| o < 0) {
            return Err(bad_request("Kilométrage invalide"));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct Range {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

pub async fn list(_: AuthUser, State(s): State<AppState>, Query(r): Query<Range>) -> AppResult<Json<Vec<Expense>>> {
    let rows = sqlx::query_as!(
        Expense,
        r#"SELECT id, spent_on, category, label, amount_cents, odometer_km, notes
           FROM expenses
           WHERE ($1::date IS NULL OR spent_on >= $1) AND ($2::date IS NULL OR spent_on < $2)
           ORDER BY spent_on DESC, created_at DESC"#,
        r.from,
        r.to
    )
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

pub async fn create(_: AuthUser, State(s): State<AppState>, Json(i): Json<ExpenseInput>) -> AppResult<(StatusCode, Json<Expense>)> {
    i.validate()?;
    let row = sqlx::query_as!(
        Expense,
        r#"INSERT INTO expenses (spent_on, category, label, amount_cents, odometer_km, notes)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, spent_on, category, label, amount_cents, odometer_km, notes"#,
        i.spent_on, i.category, i.label.trim(), i.amount_cents, i.odometer_km, i.notes
    )
    .fetch_one(&s.db)
    .await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn update(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>, Json(i): Json<ExpenseInput>) -> AppResult<Json<Expense>> {
    i.validate()?;
    let row = sqlx::query_as!(
        Expense,
        r#"UPDATE expenses SET spent_on = $2, category = $3, label = $4, amount_cents = $5, odometer_km = $6, notes = $7
           WHERE id = $1
           RETURNING id, spent_on, category, label, amount_cents, odometer_km, notes"#,
        id, i.spent_on, i.category, i.label.trim(), i.amount_cents, i.odometer_km, i.notes
    )
    .fetch_optional(&s.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

pub async fn remove(_: AuthUser, State(s): State<AppState>, Path(id): Path<Uuid>) -> AppResult<StatusCode> {
    let res = sqlx::query!("DELETE FROM expenses WHERE id = $1", id).execute(&s.db).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
