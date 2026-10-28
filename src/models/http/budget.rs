//! HTTP request and response models for budget routes.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::{
    category::Category,
    common::{round_cents, validate_name, validate_positive_amount},
    sql::budget::BudgetRow,
};

/// Request body for `POST /api/budgets`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateBudgetRequest {
    /// Display name, e.g. "Eating out".
    pub name: String,
    /// Category the budget tracks. Immutable after creation.
    pub category: Category,
    /// Monthly spending limit; must be greater than 0.
    pub monthly_limit: f64,
    /// Whether the budget is enforced. Defaults to `true`.
    pub active: Option<bool>,
}

impl CreateBudgetRequest {
    /// Validates name and limit.
    pub fn validate(&self) -> Result<(), String> {
        validate_name(&self.name)?;
        validate_positive_amount("monthly_limit", self.monthly_limit)
    }
}

/// Request body for `PUT /api/budgets/{id}`. All fields optional; omitted fields are unchanged.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateBudgetRequest {
    /// New display name.
    pub name: Option<String>,
    /// New monthly limit; must be greater than 0.
    pub monthly_limit: Option<f64>,
    /// Enable or disable the budget.
    pub active: Option<bool>,
}

impl UpdateBudgetRequest {
    /// Requires at least one field and validates the ones supplied.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_none() && self.monthly_limit.is_none() && self.active.is_none() {
            return Err("at least one field must be provided".to_string());
        }
        if let Some(name) = &self.name {
            validate_name(name)?;
        }
        if let Some(limit) = self.monthly_limit {
            validate_positive_amount("monthly_limit", limit)?;
        }
        Ok(())
    }
}

/// Query parameters for budget reads.
#[derive(Debug, Deserialize, ToSchema)]
pub struct BudgetQuery {
    /// Month to evaluate as `YYYY-MM`. Defaults to the current month (UTC).
    pub month: Option<String>,
}

/// A calendar month: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthWindow {
    /// First day of the month.
    pub start: NaiveDate,
    /// First day of the following month.
    pub end: NaiveDate,
}

impl MonthWindow {
    /// The `YYYY-MM` label for this month.
    pub fn label(&self) -> String {
        format!("{:04}-{:02}", self.start.year(), self.start.month())
    }
}

/// Resolves the `month` query value (or `today`'s month when absent) into a [`MonthWindow`].
pub fn parse_month(month: Option<&str>, today: NaiveDate) -> Result<MonthWindow, String> {
    let start = match month {
        None => NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
            .ok_or_else(|| "month is out of range".to_string())?,
        Some(m) => {
            let bytes = m.as_bytes();
            let well_formed = bytes.len() == 7 && bytes[4] == b'-';
            well_formed
                .then(|| NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d").ok())
                .flatten()
                .ok_or_else(|| "month must be in YYYY-MM format".to_string())?
        }
    };
    let end = if start.month() == 12 {
        NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)
    }
    .ok_or_else(|| "month is out of range".to_string())?;
    Ok(MonthWindow { start, end })
}

/// A budget with its computed spending for one month.
#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetResponse {
    /// Budget ID.
    pub id: i32,
    /// Display name.
    pub name: String,
    /// Tracked category.
    pub category: Category,
    /// Monthly limit.
    pub monthly_limit: f64,
    /// Whether the budget is enforced.
    pub active: bool,
    /// Month evaluated, as `YYYY-MM`.
    pub month: String,
    /// Money out in this category during the month.
    pub spent: f64,
    /// `monthly_limit - spent`, never below 0.
    pub remaining: f64,
    /// `spent / monthly_limit * 100`; may exceed 100.
    pub percent_used: f64,
    /// `true` when `spent` is strictly greater than `monthly_limit`.
    pub over_budget: bool,
}

impl BudgetResponse {
    /// Builds a response from a row, the month evaluated and the amount spent in it.
    pub fn new(row: &BudgetRow, month: &MonthWindow, spent: f64) -> Self {
        let spent = round_cents(spent.max(0.0));
        Self {
            id: row.id,
            name: row.name.clone(),
            category: Category::parse(&row.category).unwrap_or(Category::Other),
            monthly_limit: row.monthly_limit,
            active: row.active,
            month: month.label(),
            spent,
            remaining: round_cents((row.monthly_limit - spent).max(0.0)),
            percent_used: round_cents(spent / row.monthly_limit * 100.0),
            over_budget: spent > row.monthly_limit,
        }
    }
}

/// Response body for `GET /api/budgets`.
#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetsResponse {
    /// Month evaluated, as `YYYY-MM`.
    pub month: String,
    /// The user's budgets.
    pub budgets: Vec<BudgetResponse>,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::global::MAX_AMOUNT;
    use chrono::Utc;

    fn row(limit: f64) -> BudgetRow {
        BudgetRow {
            id: 1,
            user_id: 1,
            name: "Food".to_string(),
            category: "FOOD_DINING".to_string(),
            monthly_limit: limit,
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn window() -> MonthWindow {
        parse_month(
            Some("2024-03"),
            NaiveDate::from_ymd_opt(2024, 3, 9).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn parse_month_defaults_to_current_month() {
        let today = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let w = parse_month(None, today).unwrap();
        assert_eq!(w.start, NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
        assert_eq!(w.end, NaiveDate::from_ymd_opt(2024, 3, 1).unwrap());
        assert_eq!(w.label(), "2024-02");
    }

    #[test]
    fn parse_month_december_rolls_into_next_year() {
        let w = parse_month(
            Some("2024-12"),
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(w.end, NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    }

    #[test]
    fn parse_month_rejects_malformed_values() {
        let today = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        for bad in [
            "2024-13", "2024-00", "2024-1", "24-01", "2024/01", "abcdefg", "",
        ] {
            assert!(parse_month(Some(bad), today).is_err(), "{bad}");
        }
    }

    #[test]
    fn response_under_budget() {
        let r = BudgetResponse::new(&row(400.0), &window(), 100.0);
        assert_eq!(r.spent, 100.0);
        assert_eq!(r.remaining, 300.0);
        assert_eq!(r.percent_used, 25.0);
        assert!(!r.over_budget);
        assert_eq!(r.month, "2024-03");
        assert_eq!(r.category, Category::FoodDining);
    }

    #[test]
    fn response_exactly_at_limit_is_not_over() {
        let r = BudgetResponse::new(&row(400.0), &window(), 400.0);
        assert_eq!(r.remaining, 0.0);
        assert_eq!(r.percent_used, 100.0);
        assert!(!r.over_budget);
    }

    #[test]
    fn response_over_budget_clamps_remaining() {
        let r = BudgetResponse::new(&row(400.0), &window(), 500.0);
        assert_eq!(r.remaining, 0.0);
        assert_eq!(r.percent_used, 125.0);
        assert!(r.over_budget);
    }

    #[test]
    fn response_negative_net_spend_is_zero() {
        let r = BudgetResponse::new(&row(400.0), &window(), -20.0);
        assert_eq!(r.spent, 0.0);
        assert_eq!(r.remaining, 400.0);
    }

    #[test]
    fn response_rounds_to_cents() {
        let r = BudgetResponse::new(&row(100.0), &window(), 0.1 + 0.2);
        assert_eq!(r.spent, 0.3);
    }

    #[test]
    fn create_validation() {
        let ok = CreateBudgetRequest {
            name: "Food".into(),
            category: Category::Groceries,
            monthly_limit: 10.0,
            active: None,
        };
        assert!(ok.validate().is_ok());
        for limit in [0.0, -1.0, f64::NAN, f64::INFINITY, MAX_AMOUNT + 1.0] {
            let bad = CreateBudgetRequest {
                name: "Food".into(),
                category: Category::Groceries,
                monthly_limit: limit,
                active: None,
            };
            assert!(bad.validate().is_err(), "{limit}");
        }
        for name in ["", "   ", &"x".repeat(101)] {
            let bad = CreateBudgetRequest {
                name: name.to_string(),
                category: Category::Groceries,
                monthly_limit: 10.0,
                active: None,
            };
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn update_validation() {
        let empty = UpdateBudgetRequest {
            name: None,
            monthly_limit: None,
            active: None,
        };
        assert!(empty.validate().is_err());
        let only_active = UpdateBudgetRequest {
            name: None,
            monthly_limit: None,
            active: Some(false),
        };
        assert!(only_active.validate().is_ok());
        let bad_limit = UpdateBudgetRequest {
            name: None,
            monthly_limit: Some(0.0),
            active: None,
        };
        assert!(bad_limit.validate().is_err());
        let bad_name = UpdateBudgetRequest {
            name: Some(" ".into()),
            monthly_limit: None,
            active: None,
        };
        assert!(bad_name.validate().is_err());
    }
}
