//! SQL row models for budgets.

use chrono::{DateTime, Utc};

/// Row model for the `budgets` table.
pub struct BudgetRow {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// Display name.
    pub name: String,
    /// Category string (`FOOD_DINING`, ...).
    pub category: String,
    /// Monthly spending limit.
    pub monthly_limit: f64,
    /// Whether the budget is currently enforced.
    pub active: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}
