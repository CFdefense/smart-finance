//! SQL row models for rules.

use chrono::{DateTime, Utc};

/// Row model for the `rules` table. Targeted accounts live in `rule_accounts`.
pub struct RuleRow {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// The user's own sentence.
    pub description: String,
    /// `true` when the rule targets every account the user owns.
    pub all_accounts: bool,
    /// Whether the rule is active.
    pub enabled: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}
