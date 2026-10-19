//! Domain models returned by the Plaid client (`crate::plaid::client::PlaidClient`).

use chrono::NaiveDate;

/// Account data returned by Plaid's `/accounts/get` endpoint.
#[derive(Debug)]
pub struct PlaidAccountData {
    /// Plaid's unique account identifier.
    pub account_id: String,
    /// Plaid Item this account belongs to.
    pub item_id: String,
    /// Account display name.
    pub name: String,
    /// Official account name, when available.
    pub official_name: Option<String>,
    /// Account type (e.g. `depository`, `credit`).
    pub account_type: String,
    /// Account subtype (e.g. `checking`, `savings`).
    pub account_subtype: Option<String>,
    /// Masked account number, when available.
    pub mask: Option<String>,
    /// Current balance, when available.
    pub current_balance: Option<f64>,
    /// Available balance, when supported.
    pub available_balance: Option<f64>,
    /// ISO 4217 currency code, when available.
    pub currency_code: Option<String>,
}

/// Transaction data from a single Plaid Transactions Sync page.
#[derive(Debug)]
pub struct PlaidTransactionData {
    /// Plaid's unique transaction identifier.
    pub transaction_id: String,
    /// Account this transaction belongs to.
    pub account_id: String,
    /// Merchant or transaction name.
    pub name: String,
    /// Transaction amount.
    pub amount: f64,
    /// Date of the transaction.
    pub date: NaiveDate,
    /// Whether the transaction is pending.
    pub pending: bool,
}

/// A single page returned by Plaid's `/transactions/sync` endpoint.
#[derive(Debug)]
pub struct PlaidSyncPage {
    /// Newly added transactions.
    pub added: Vec<PlaidTransactionData>,
    /// Modified transactions.
    pub modified: Vec<PlaidTransactionData>,
    /// Transaction IDs of removed transactions.
    pub removed: Vec<String>,
    /// Cursor to pass on the next sync call.
    pub next_cursor: String,
    /// Whether more pages remain.
    pub has_more: bool,
}
