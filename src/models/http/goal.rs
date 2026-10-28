//! HTTP request and response models for goal routes.

use crate::global::{GOAL_MAX_DESCRIPTION, GOAL_MAX_NOTE, MAX_AMOUNT};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::{
    common::{
        double_option, progress_percent, round_cents, validate_account_id, validate_baseline,
        validate_future, validate_max_len, validate_name, validate_positive_amount,
    },
    sql::goal::{ContributionRow, GoalRow},
};

/// Lifecycle state of a goal. Always derived, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GoalStatus {
    /// Still working towards the target.
    Active,
    /// The current amount has reached the target.
    Completed,
    /// Archived by the user (takes precedence over completion).
    Archived,
}

impl GoalStatus {
    /// Parses the `?status=` query value (case-sensitive).
    pub fn parse(value: &str) -> Option<GoalStatus> {
        match value {
            "ACTIVE" => Some(GoalStatus::Active),
            "COMPLETED" => Some(GoalStatus::Completed),
            "ARCHIVED" => Some(GoalStatus::Archived),
            _ => None,
        }
    }
}

/// Derives the status: archived first, then completed once `current >= target`.
pub fn derive_status(archived: bool, current: f64, target: f64) -> GoalStatus {
    if archived {
        GoalStatus::Archived
    } else if round_cents(current) >= round_cents(target) {
        GoalStatus::Completed
    } else {
        GoalStatus::Active
    }
}

/// Whether the goal is on pace for its target date (linear from creation to target date).
///
/// Returns `None` without a target date, `Some(true)` once the target is reached, and
/// `Some(false)` when the date has passed or the window is empty without completion.
pub fn compute_on_track(
    current: f64,
    target: f64,
    created: NaiveDate,
    target_date: Option<NaiveDate>,
    today: NaiveDate,
) -> Option<bool> {
    let target_date = target_date?;
    if round_cents(current) >= round_cents(target) {
        return Some(true);
    }
    let total_days = (target_date - created).num_days();
    if today >= target_date || total_days <= 0 {
        return Some(false);
    }
    let elapsed = (today - created).num_days().clamp(0, total_days);
    #[allow(clippy::cast_precision_loss)] // day counts are tiny
    let expected = target * elapsed as f64 / total_days as f64;
    Some(round_cents(current) >= round_cents(expected))
}

/// Query parameters for `GET /api/goals`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct GoalsQuery {
    /// Only return goals with this derived status (`ACTIVE`, `COMPLETED` or `ARCHIVED`).
    pub status: Option<String>,
}

/// Request body for `POST /api/goals`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGoalRequest {
    /// Display name, e.g. "Emergency fund".
    pub name: String,
    /// Optional description (max 500 characters).
    pub description: Option<String>,
    /// Amount to reach; must be greater than 0.
    pub target_amount: f64,
    /// Optional deadline; must be in the future.
    #[schema(value_type = Option<String>, format = Date)]
    pub target_date: Option<NaiveDate>,
    /// Optional Plaid `account_id` (a depository account you own) whose growth counts as progress.
    pub account_id: Option<String>,
    /// Starting balance to measure account growth from. Defaults to the account's current
    /// balance; pass `0` to count the whole balance. Requires `account_id`.
    pub baseline_balance: Option<f64>,
}

impl CreateGoalRequest {
    /// Validates all fields against `today`.
    pub fn validate(&self, today: NaiveDate) -> Result<(), String> {
        // Validate the name
        validate_name(&self.name)?;

        // Ensure the amount is positive
        validate_positive_amount("target_amount", self.target_amount)?;

        // Validate length of description
        if let Some(d) = &self.description {
            validate_max_len("description", d, GOAL_MAX_DESCRIPTION)?;
        }

        // Ensure the date is in the future
        if let Some(date) = self.target_date {
            validate_future("target_date", date, today)?;
        }

        // Ensure account id is valid
        if let Some(id) = &self.account_id {
            validate_account_id(id)?;
        }

        // Validate initial balance
        if let Some(b) = self.baseline_balance {
            if self.account_id.is_none() {
                return Err("baseline_balance requires account_id".to_string());
            }
            validate_baseline(b)?;
        }
        Ok(())
    }
}

/// Request body for `PUT /api/goals/{id}`. Omitted fields are unchanged; `null` clears
/// `description`, `target_date` and `account_id`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateGoalRequest {
    /// New display name.
    pub name: Option<String>,
    /// New description; `null` or `""` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    /// New target; must be greater than 0.
    pub target_amount: Option<f64>,
    /// New deadline (must be in the future); `null` clears it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub target_date: Option<Option<NaiveDate>>,
    /// Link a different depository account, or `null` to stop tracking an account.
    /// Progress made so far is preserved as a contribution.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub account_id: Option<Option<String>>,
    /// Baseline for a newly linked account. Requires a non-null `account_id`.
    pub baseline_balance: Option<f64>,
    /// Archive or unarchive the goal.
    pub archived: Option<bool>,
}

impl UpdateGoalRequest {
    /// Requires at least one field and validates the ones supplied.
    pub fn validate(&self, today: NaiveDate) -> Result<(), String> {
        if self.name.is_none()
            && self.description.is_none()
            && self.target_amount.is_none()
            && self.target_date.is_none()
            && self.account_id.is_none()
            && self.baseline_balance.is_none()
            && self.archived.is_none()
        {
            return Err("at least one field must be provided".to_string());
        }
        if let Some(name) = &self.name {
            validate_name(name)?;
        }
        if let Some(Some(d)) = &self.description {
            validate_max_len("description", d, GOAL_MAX_DESCRIPTION)?;
        }
        if let Some(t) = self.target_amount {
            validate_positive_amount("target_amount", t)?;
        }
        if let Some(Some(date)) = self.target_date {
            validate_future("target_date", date, today)?;
        }
        if let Some(Some(id)) = &self.account_id {
            validate_account_id(id)?;
        }
        if let Some(b) = self.baseline_balance {
            if !matches!(self.account_id, Some(Some(_))) {
                return Err("baseline_balance requires a non-null account_id".to_string());
            }
            validate_baseline(b)?;
        }
        Ok(())
    }
}

/// Request body for `POST /api/goals/{id}/contributions`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateContributionRequest {
    /// Signed, non-zero amount: positive deposits, negative withdrawals.
    pub amount: f64,
    /// Day the contribution applies to. Defaults to today; cannot be in the future.
    #[schema(value_type = Option<String>, format = Date)]
    pub date: Option<NaiveDate>,
    /// Optional note (max 200 characters).
    pub note: Option<String>,
}

impl CreateContributionRequest {
    /// Validates amount, date and note against `today`.
    pub fn validate(&self, today: NaiveDate) -> Result<(), String> {
        if !self.amount.is_finite() || round_cents(self.amount) == 0.0 {
            return Err("amount must be a non-zero number".to_string());
        }
        if self.amount.abs() > MAX_AMOUNT {
            return Err(format!("amount must be at most {MAX_AMOUNT} in size"));
        }
        if self.date.is_some_and(|d| d > today) {
            return Err("date cannot be in the future".to_string());
        }
        if let Some(n) = &self.note {
            validate_max_len("note", n, GOAL_MAX_NOTE)?;
        }
        Ok(())
    }
}

/// A goal with its derived progress.
#[derive(Debug, Serialize, ToSchema)]
pub struct GoalResponse {
    /// Goal ID.
    pub id: i32,
    /// Display name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Amount to reach.
    pub target_amount: f64,
    /// Optional deadline.
    #[schema(value_type = Option<String>, format = Date)]
    pub target_date: Option<NaiveDate>,
    /// Linked account whose growth counts as progress, if any.
    pub account_id: Option<String>,
    /// Manual contributions plus linked-account growth.
    pub current_amount: f64,
    /// Percentage of the target reached (0–100).
    pub progress_percent: f64,
    /// Amount still needed, never below 0.
    pub remaining: f64,
    /// Whether progress is on pace for `target_date`; `null` without a date.
    pub on_track: Option<bool>,
    /// Derived status.
    pub status: GoalStatus,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl GoalResponse {
    /// Derives all computed fields from a row as of `today`.
    pub fn new(row: &GoalRow, today: NaiveDate) -> Self {
        let growth = match (&row.account_id, row.account_balance, row.baseline_balance) {
            (Some(_), Some(balance), Some(baseline)) => balance - baseline,
            _ => 0.0,
        };
        let current = round_cents(row.manual_total + growth);
        Self {
            id: row.id,
            name: row.name.clone(),
            description: row.description.clone(),
            target_amount: row.target_amount,
            target_date: row.target_date,
            account_id: row.account_id.clone(),
            current_amount: current,
            progress_percent: progress_percent(current, row.target_amount),
            remaining: round_cents((row.target_amount - current).max(0.0)),
            on_track: compute_on_track(
                current,
                row.target_amount,
                row.created_at.date_naive(),
                row.target_date,
                today,
            ),
            status: derive_status(row.archived, current, row.target_amount),
            created_at: row.created_at,
        }
    }
}

/// Response body for `GET /api/goals`.
#[derive(Debug, Serialize, ToSchema)]
pub struct GoalsResponse {
    /// The user's goals, newest first.
    pub goals: Vec<GoalResponse>,
}

/// One contribution in a goal's ledger.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContributionResponse {
    /// Contribution ID.
    pub id: i32,
    /// Signed amount (positive deposit, negative withdrawal).
    pub amount: f64,
    /// Day the contribution applies to.
    #[schema(value_type = String, format = Date)]
    pub date: NaiveDate,
    /// Optional note.
    pub note: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl From<ContributionRow> for ContributionResponse {
    fn from(r: ContributionRow) -> Self {
        Self {
            id: r.id,
            amount: r.amount,
            date: r.contribution_date,
            note: r.note,
            created_at: r.created_at,
        }
    }
}

/// Response body for `GET /api/goals/{id}/contributions`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContributionsResponse {
    /// Contributions, newest first.
    pub contributions: Vec<ContributionResponse>,
}

/// Response body for `POST /api/goals/{id}/contributions`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContributionCreatedResponse {
    /// The new ledger entry.
    pub contribution: ContributionResponse,
    /// The goal after applying the contribution.
    pub goal: GoalResponse,
}

#[cfg(test)]
mod test {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn row(manual: f64, target: f64) -> GoalRow {
        GoalRow {
            id: 1,
            user_id: 1,
            name: "Trip".into(),
            description: None,
            target_amount: target,
            target_date: None,
            account_id: None,
            baseline_balance: None,
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            manual_total: manual,
            account_balance: None,
        }
    }

    #[test]
    fn status_precedence() {
        assert_eq!(derive_status(false, 50.0, 100.0), GoalStatus::Active);
        assert_eq!(derive_status(false, 100.0, 100.0), GoalStatus::Completed);
        assert_eq!(derive_status(false, 150.0, 100.0), GoalStatus::Completed);
        assert_eq!(derive_status(true, 150.0, 100.0), GoalStatus::Archived);
        assert_eq!(derive_status(true, 0.0, 100.0), GoalStatus::Archived);
        // Float noise below a cent does not block completion
        assert_eq!(derive_status(false, 99.999, 100.0), GoalStatus::Completed);
    }

    #[test]
    fn status_parse() {
        assert_eq!(GoalStatus::parse("ACTIVE"), Some(GoalStatus::Active));
        assert_eq!(GoalStatus::parse("COMPLETED"), Some(GoalStatus::Completed));
        assert_eq!(GoalStatus::parse("ARCHIVED"), Some(GoalStatus::Archived));
        assert_eq!(GoalStatus::parse("active"), None);
        assert_eq!(GoalStatus::parse(""), None);
    }

    #[test]
    fn percent_is_clamped() {
        assert_eq!(progress_percent(50.0, 200.0), 25.0);
        assert_eq!(progress_percent(-10.0, 200.0), 0.0);
        assert_eq!(progress_percent(500.0, 200.0), 100.0);
        assert_eq!(progress_percent(1.0, 0.0), 0.0);
    }

    #[test]
    fn on_track_without_date_is_none() {
        assert_eq!(
            compute_on_track(0.0, 100.0, d(2024, 1, 1), None, d(2024, 2, 1)),
            None
        );
    }

    #[test]
    fn on_track_completed_is_true_even_after_deadline() {
        assert_eq!(
            compute_on_track(
                100.0,
                100.0,
                d(2024, 1, 1),
                Some(d(2024, 2, 1)),
                d(2024, 3, 1)
            ),
            Some(true)
        );
    }

    #[test]
    fn on_track_past_deadline_incomplete_is_false() {
        assert_eq!(
            compute_on_track(
                50.0,
                100.0,
                d(2024, 1, 1),
                Some(d(2024, 2, 1)),
                d(2024, 2, 1)
            ),
            Some(false)
        );
    }

    #[test]
    fn on_track_empty_window_is_false() {
        // created on the target date: nothing to pace against
        assert_eq!(
            compute_on_track(
                0.0,
                100.0,
                d(2024, 1, 1),
                Some(d(2024, 1, 1)),
                d(2024, 1, 1)
            ),
            Some(false)
        );
    }

    #[test]
    fn on_track_is_linear() {
        // 100-day window, halfway through: expected 50
        let created = d(2024, 1, 1);
        let target = Some(d(2024, 4, 10));
        let today = d(2024, 2, 20);
        let total = (d(2024, 4, 10) - created).num_days();
        let elapsed = (today - created).num_days();
        let expected = 100.0 * elapsed as f64 / total as f64;
        assert_eq!(
            compute_on_track(expected + 1.0, 100.0, created, target, today),
            Some(true)
        );
        assert_eq!(
            compute_on_track(expected - 1.0, 100.0, created, target, today),
            Some(false)
        );
    }

    #[test]
    fn on_track_before_creation_day_expects_zero() {
        // clock skew: today earlier than created must not panic or go negative
        assert_eq!(
            compute_on_track(
                0.0,
                100.0,
                d(2024, 1, 10),
                Some(d(2024, 2, 10)),
                d(2024, 1, 1)
            ),
            Some(true)
        );
    }

    #[test]
    fn response_manual_only() {
        let r = GoalResponse::new(&row(25.0, 100.0), d(2024, 1, 1));
        assert_eq!(r.current_amount, 25.0);
        assert_eq!(r.progress_percent, 25.0);
        assert_eq!(r.remaining, 75.0);
        assert_eq!(r.status, GoalStatus::Active);
        assert_eq!(r.on_track, None);
    }

    #[test]
    fn response_includes_linked_account_growth() {
        let mut g = row(10.0, 100.0);
        g.account_id = Some("acct".into());
        g.baseline_balance = Some(500.0);
        g.account_balance = Some(540.0);
        let r = GoalResponse::new(&g, d(2024, 1, 1));
        assert_eq!(r.current_amount, 50.0);
    }

    #[test]
    fn response_ignores_growth_when_account_missing() {
        let mut g = row(10.0, 100.0);
        g.account_id = Some("acct".into());
        g.baseline_balance = Some(500.0);
        g.account_balance = None; // balance unknown
        assert_eq!(GoalResponse::new(&g, d(2024, 1, 1)).current_amount, 10.0);
        g.account_id = None; // disconnected: stale baseline/balance are ignored
        g.account_balance = Some(900.0);
        assert_eq!(GoalResponse::new(&g, d(2024, 1, 1)).current_amount, 10.0);
    }

    #[test]
    fn response_account_decline_can_go_negative_but_remaining_is_capped() {
        let mut g = row(0.0, 100.0);
        g.account_id = Some("acct".into());
        g.baseline_balance = Some(500.0);
        g.account_balance = Some(450.0);
        let r = GoalResponse::new(&g, d(2024, 1, 1));
        assert_eq!(r.current_amount, -50.0);
        assert_eq!(r.progress_percent, 0.0);
        assert_eq!(r.remaining, 150.0);
    }

    #[test]
    fn response_archived_and_completed() {
        let mut g = row(100.0, 100.0);
        assert_eq!(
            GoalResponse::new(&g, d(2024, 1, 1)).status,
            GoalStatus::Completed
        );
        g.archived = true;
        assert_eq!(
            GoalResponse::new(&g, d(2024, 1, 1)).status,
            GoalStatus::Archived
        );
    }

    fn create(name: &str, target: f64) -> CreateGoalRequest {
        CreateGoalRequest {
            name: name.into(),
            description: None,
            target_amount: target,
            target_date: None,
            account_id: None,
            baseline_balance: None,
        }
    }

    #[test]
    fn create_validation() {
        let today = d(2024, 6, 1);
        assert!(create("Trip", 100.0).validate(today).is_ok());
        assert!(create("", 100.0).validate(today).is_err());
        assert!(create("Trip", 0.0).validate(today).is_err());
        assert!(create("Trip", f64::NAN).validate(today).is_err());

        let mut g = create("Trip", 100.0);
        g.target_date = Some(today);
        assert!(g.validate(today).is_err(), "today is not in the future");
        g.target_date = Some(d(2024, 6, 2));
        assert!(g.validate(today).is_ok());

        let mut g = create("Trip", 100.0);
        g.description = Some("x".repeat(501));
        assert!(g.validate(today).is_err());

        let mut g = create("Trip", 100.0);
        g.baseline_balance = Some(10.0);
        assert!(g.validate(today).is_err(), "baseline needs an account");
        g.account_id = Some("a".into());
        assert!(g.validate(today).is_ok());
        g.baseline_balance = Some(f64::INFINITY);
        assert!(g.validate(today).is_err());
        g.baseline_balance = None;
        g.account_id = Some("  ".into());
        assert!(g.validate(today).is_err());
    }

    fn update() -> UpdateGoalRequest {
        UpdateGoalRequest {
            name: None,
            description: None,
            target_amount: None,
            target_date: None,
            account_id: None,
            baseline_balance: None,
            archived: None,
        }
    }

    #[test]
    fn update_validation() {
        let today = d(2024, 6, 1);
        assert!(update().validate(today).is_err(), "empty update");

        let mut u = update();
        u.archived = Some(true);
        assert!(u.validate(today).is_ok());

        let mut u = update();
        u.target_date = Some(None);
        assert!(u.validate(today).is_ok(), "clearing the date is allowed");
        u.target_date = Some(Some(today));
        assert!(u.validate(today).is_err());

        let mut u = update();
        u.target_amount = Some(-1.0);
        assert!(u.validate(today).is_err());

        let mut u = update();
        u.description = Some(Some("x".repeat(501)));
        assert!(u.validate(today).is_err());
        u.description = Some(None);
        assert!(u.validate(today).is_ok());

        let mut u = update();
        u.baseline_balance = Some(5.0);
        assert!(u.validate(today).is_err(), "baseline without account");
        u.account_id = Some(None);
        assert!(u.validate(today).is_err(), "baseline with unlink");
        u.account_id = Some(Some("a".into()));
        assert!(u.validate(today).is_ok());
    }

    #[test]
    fn update_distinguishes_null_from_absent_in_json() {
        let u: UpdateGoalRequest = serde_json::from_str(r#"{"account_id": null}"#).unwrap();
        assert_eq!(u.account_id, Some(None));
        let u: UpdateGoalRequest = serde_json::from_str(r#"{"name": "x"}"#).unwrap();
        assert_eq!(u.account_id, None);
    }

    #[test]
    fn contribution_validation() {
        let today = d(2024, 6, 1);
        let ok = |amount: f64| CreateContributionRequest {
            amount,
            date: None,
            note: None,
        };
        assert!(ok(10.0).validate(today).is_ok());
        assert!(ok(-10.0).validate(today).is_ok());
        for bad in [0.0, 0.001, f64::NAN, f64::INFINITY, MAX_AMOUNT * 2.0] {
            assert!(ok(bad).validate(today).is_err(), "{bad}");
        }
        let future = CreateContributionRequest {
            amount: 5.0,
            date: Some(d(2024, 6, 2)),
            note: None,
        };
        assert!(future.validate(today).is_err());
        let long_note = CreateContributionRequest {
            amount: 5.0,
            date: Some(today),
            note: Some("n".repeat(201)),
        };
        assert!(long_note.validate(today).is_err());
    }
}
