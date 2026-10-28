//! HTTP request and response models for rule routes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::{http::common::validate_max_len, sql::rule::RuleRow};

/// Maximum length of a rule sentence.
const MAX_DESCRIPTION: usize = 500;
/// Maximum number of accounts a single rule can target.
const MAX_ACCOUNTS: usize = 50;

/// Request body for `POST /api/rules` and `PUT /api/rules/{id}` (PUT replaces the rule).
#[derive(Debug, Deserialize, ToSchema)]
pub struct RuleRequest {
    /// The rule in the user's own words, e.g. "I don't want to spend any money on food"
    /// (1–500 chars). It is not interpreted by the backend; the AI agent reads it.
    pub description: String,
    /// Plaid `account_id`s the rule applies to. Omitted or empty means **all** of the
    /// user's accounts.
    #[serde(default)]
    pub account_ids: Vec<String>,
    /// Whether the rule is active. Defaults to `true` on create; unchanged on update if omitted.
    pub enabled: Option<bool>,
}

impl RuleRequest {
    /// Validates the sentence and the account list.
    pub fn validate(&self) -> Result<(), String> {
        if self.description.trim().is_empty() {
            return Err("description must not be empty".to_string());
        }
        validate_max_len("description", &self.description, MAX_DESCRIPTION)?;
        if self.account_ids.iter().any(|a| a.trim().is_empty()) {
            return Err("account_ids must not contain empty values".to_string());
        }
        if self.unique_account_ids().len() > MAX_ACCOUNTS {
            return Err(format!("a rule can target at most {MAX_ACCOUNTS} accounts"));
        }
        Ok(())
    }

    /// The targeted accounts with duplicates removed, order preserved.
    pub fn unique_account_ids(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for id in &self.account_ids {
            if !seen.contains(id) {
                seen.push(id.clone());
            }
        }
        seen
    }
}

/// A stored rule.
#[derive(Debug, Serialize, ToSchema)]
pub struct RuleResponse {
    /// Rule ID.
    pub id: i32,
    /// The user's sentence.
    pub description: String,
    /// `true` when the rule applies to every account the user owns (`account_ids` is then empty).
    pub all_accounts: bool,
    /// Accounts the rule targets.
    pub account_ids: Vec<String>,
    /// Whether the rule is active.
    pub enabled: bool,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl RuleResponse {
    /// Builds a response from a row and its targeted accounts.
    pub fn new(row: &RuleRow, account_ids: Vec<String>) -> Self {
        Self {
            id: row.id,
            description: row.description.clone(),
            all_accounts: row.all_accounts,
            account_ids,
            enabled: row.enabled,
            created_at: row.created_at,
        }
    }
}

/// Query parameters for `GET /api/rules`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListQuery {
    /// Only return rules that apply to this account (rules for all accounts are included).
    pub account_id: Option<String>,
    /// Only return enabled (`true`) or disabled (`false`) rules.
    pub enabled: Option<bool>,
}

/// Response body for `GET /api/rules`.
#[derive(Debug, Serialize, ToSchema)]
pub struct RulesResponse {
    /// The user's rules, newest first.
    pub rules: Vec<RuleResponse>,
}

#[cfg(test)]
mod test {
    use super::*;

    fn req(description: &str, accounts: &[&str]) -> RuleRequest {
        RuleRequest {
            description: description.to_string(),
            account_ids: accounts.iter().map(|s| (*s).to_string()).collect(),
            enabled: None,
        }
    }

    #[test]
    fn validates_description() {
        assert!(req("no food", &[]).validate().is_ok());
        assert!(req("   ", &[]).validate().is_err());
        assert!(req("", &[]).validate().is_err());
        assert!(req(&"x".repeat(500), &[]).validate().is_ok());
        assert!(req(&"x".repeat(501), &[]).validate().is_err());
    }

    #[test]
    fn validates_accounts() {
        assert!(req("d", &["a", "b"]).validate().is_ok());
        assert!(req("d", &["a", " "]).validate().is_err());
        let many: Vec<String> = (0..51).map(|i| format!("a{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(req("d", &refs).validate().is_err());
        assert!(req("d", &refs[..50]).validate().is_ok());
    }

    #[test]
    fn duplicate_accounts_are_collapsed() {
        let r = req("d", &["a", "b", "a", "b", "c"]);
        assert_eq!(r.unique_account_ids(), vec!["a", "b", "c"]);
        // 51 entries but only 2 distinct is fine
        let dup: Vec<&str> = (0..51)
            .map(|i| if i % 2 == 0 { "a" } else { "b" })
            .collect();
        assert!(req("d", &dup).validate().is_ok());
    }

    #[test]
    fn account_ids_default_to_empty() {
        let r: RuleRequest = serde_json::from_str(r#"{"description":"d"}"#).unwrap();
        assert!(r.account_ids.is_empty());
        assert_eq!(r.enabled, None);
    }
}
