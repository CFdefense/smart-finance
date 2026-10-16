//! SQL row models for Plaid tables.

use chrono::{DateTime, NaiveDate, Utc};

/// Row model for the `plaid_links` table.
///
/// Represents a linked bank connection (Plaid Item) for a user.
pub struct PlaidLinkRow {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// Plaid's unique identifier for this Item.
    pub item_id: String,
    /// Plaid access token for this Item. Never exposed in API responses.
    pub access_token: String,
    /// When the Item was linked.
    pub created_at: DateTime<Utc>,
}

/// Row model for the `plaid_accounts` table.
///
/// Represents a financial account fetched from Plaid for a given Item.
pub struct PlaidAccountRow {
    /// Primary key.
    pub id: i32,
    /// Plaid's unique account identifier.
    pub account_id: String,
    /// FK to `plaid_links.item_id`.
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
    /// When the account record was first stored.
    pub created_at: DateTime<Utc>,
    /// When the account record was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Row model for the `plaid_transactions` table.
///
/// Represents a transaction synced from Plaid.
pub struct PlaidTransactionRow {
    /// Primary key.
    pub id: i32,
    /// Plaid's unique transaction identifier.
    pub transaction_id: String,
    /// FK to `plaid_accounts.account_id`.
    pub account_id: String,
    /// Merchant or transaction name.
    pub name: String,
    /// Transaction amount (positive = debit, negative = credit per Plaid convention).
    pub amount: f64,
    /// Date of the transaction.
    pub date: NaiveDate,
    /// Whether the transaction is pending.
    pub pending: bool,
    /// When the transaction was first stored.
    pub created_at: DateTime<Utc>,
    /// When the transaction was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Row model for the `plaid_sync_cursors` table.
///
/// Tracks the Transactions Sync cursor for each Plaid Item.
pub struct PlaidSyncCursorRow {
    /// Primary key.
    pub id: i32,
    /// FK to `plaid_links.item_id`.
    pub item_id: String,
    /// Opaque cursor string returned by Plaid. `None` before the first sync.
    pub cursor: Option<String>,
    /// Whether Plaid has more pages to return for the current sync window.
    pub has_more: bool,
    /// When the cursor was last updated.
    pub updated_at: DateTime<Utc>,
}
