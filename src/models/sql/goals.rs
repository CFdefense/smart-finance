//! SQL row models for goals.

use chrono::{DateTime, NaiveDate, Utc};

/// A goal joined with the live balance of its linked account.
///
/// Progress is derived from `baseline_balance`, `account_balance` and `target_amount`.
pub struct GoalRow {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// Display name.
    pub name: String,
    /// Optional free-text description.
    pub description: Option<String>,
    /// Account balance the user wants to reach.
    pub target_amount: f64,
    /// Optional date by which the target should be reached.
    pub target_date: Option<NaiveDate>,
    /// Linked Plaid account whose balance the goal tracks.
    pub account_id: String,
    /// Balance of the account when the goal was created (or re-linked); progress is measured from here.
    pub baseline_balance: f64,
    /// Whether the goal has been archived.
    pub archived: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
    /// Live balance of the linked account (`None` when Plaid reported none).
    pub account_balance: Option<f64>,
}
