//! SQL row models for goals and their contributions.

use chrono::{DateTime, NaiveDate, Utc};

/// A goal joined with the figures needed to derive its progress.
///
/// `manual_total` is `SUM(goal_contributions.amount)`; `account_balance` is the live
/// `current_balance` of the linked account (`None` when unlinked or unknown).
pub struct GoalRow {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// Display name.
    pub name: String,
    /// Optional free-text description.
    pub description: Option<String>,
    /// Amount the user wants to reach.
    pub target_amount: f64,
    /// Optional date by which the target should be reached.
    pub target_date: Option<NaiveDate>,
    /// Linked Plaid account, if the goal tracks one automatically.
    pub account_id: Option<String>,
    /// Balance of the linked account when it was linked; growth is measured from here.
    pub baseline_balance: Option<f64>,
    /// Whether the goal has been archived.
    pub archived: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
    /// Sum of manual contributions (negative = withdrawals).
    pub manual_total: f64,
    /// Live balance of the linked account.
    pub account_balance: Option<f64>,
}

/// Row model for the `goal_contributions` table.
pub struct ContributionRow {
    /// Primary key.
    pub id: i32,
    /// FK to `goals.id`.
    pub goal_id: i32,
    /// Signed amount (positive deposit, negative withdrawal).
    pub amount: f64,
    /// Day the contribution applies to.
    pub contribution_date: NaiveDate,
    /// Optional note.
    pub note: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}
