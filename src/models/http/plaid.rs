//! Plaid models: HTTP request/response models for plaid routes, plus the raw
//! Plaid API wire shapes used by [`crate::plaid::client::PlaidClient`].

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to exchange a Plaid public token.
#[derive(Deserialize, ToSchema)]
pub struct ExchangeTokenRequest {
    /// Temporary public token returned by Plaid Link after successful linking.
    pub public_token: String,
}

/// Response containing a token used to initialize Plaid Link.
///
/// Also deserialized from Plaid's `/link/token/create` response.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LinkTokenResponse {
    /// Short-lived token used by the frontend to launch Plaid Link.
    pub link_token: String,
}

/// Response confirming that a bank connection was saved.
///
/// Also deserialized from Plaid's `/item/public_token/exchange` response.
/// `access_token` is never serialized and is excluded from the `OpenAPI`
/// schema, so it cannot reach the frontend.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct ExchangeTokenResponse {
    /// Indicates whether the connection was successfully saved. Not sent by
    /// Plaid; set by the controller once the connection is stored.
    #[serde(default)]
    pub success: bool,
    /// Plaid Item ID identifying the linked financial institution connection.
    pub item_id: String,
    /// Plaid access token for the Item. Backend-only.
    #[serde(skip_serializing)]
    #[schema(ignore)]
    pub access_token: String,
}

/// Response containing Plaid financial accounts.
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountsResponse {
    /// Financial accounts linked by the authenticated user.
    pub accounts: Vec<AccountResponse>,
}

/// Singular Plaid financial account.
///
/// Also the account model returned by [`crate::plaid::client::PlaidClient::get_accounts`].
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountResponse {
    /// Your application's or Plaid's account identifier.
    pub account_id: String,
    /// Plaid Item associated with this account.
    pub item_id: String,
    /// Account display name, such as Checking.
    pub name: String,
    /// Official account name, when available.
    pub official_name: Option<String>,
    /// Account type, such as depository, credit, or loan.
    pub account_type: String,
    /// Account subtype, such as checking or savings.
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

/// Response containing synced transcation items.
#[derive(Debug, Serialize, ToSchema)]
pub struct TransactionSyncResponse {
    /// Number of newly added transactions.
    pub added: usize,
    /// Number of existing transactions that were updated.
    pub modified: usize,
    /// Number of transactions that were removed.
    pub removed: usize,
    /// Whether additional synchronization pages remain.
    pub has_more: bool,
}

/// Response containing transaction items.
#[derive(Debug, Serialize, ToSchema)]
pub struct TransactionsResponse {
    /// Transactions stored for the authenticated user.
    pub transactions: Vec<TransactionResponse>,
    /// Total number of transactions matching the query, if calculated.
    pub total: Option<i64>,
    /// Maximum number of transactions returned.
    pub limit: i64,
    /// Number of transactions skipped.
    pub offset: i64,
}

/// A singular transaction.
///
/// Also deserialized from the transactions in Plaid's `/transactions/sync`
/// response (unknown Plaid fields are ignored).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionResponse {
    /// Unique transaction ID.
    pub transaction_id: String,
    /// Account ID used.
    pub account_id: String,
    /// Account owner's name.
    pub name: String,
    /// Transaction amount.
    pub amount: f64,
    /// Date of the transaction.
    pub date: NaiveDate,
    /// Transaction status.
    pub pending: bool,
}

/// Query parameters for `GET /api/plaid/transactions`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TransactionsQuery {
    /// Maximum number of transactions to return. Defaults to 50.
    pub limit: Option<i64>,
    /// Number of transactions to skip. Defaults to 0.
    pub offset: Option<i64>,
}

// ── Raw Plaid API response shapes ────────────────────────────────────────────
//
// Only shapes that differ from the models above live here.

/// Response from Plaid's `/accounts/get` endpoint.
#[derive(Deserialize)]
pub struct PlaidAccountsGetResponse {
    /// Accounts belonging to the Item.
    pub accounts: Vec<PlaidAccountRaw>,
    /// The Item the accounts belong to.
    pub item: PlaidItemRaw,
}

/// Plaid Item as returned by `/accounts/get`.
#[derive(Deserialize)]
pub struct PlaidItemRaw {
    /// Plaid Item ID.
    pub item_id: String,
}

/// Account as returned by `/accounts/get`.
#[derive(Deserialize)]
pub struct PlaidAccountRaw {
    /// Plaid's unique account identifier.
    pub account_id: String,
    /// Account display name.
    pub name: String,
    /// Official account name, when available.
    pub official_name: Option<String>,
    /// Account type (e.g. `depository`, `credit`).
    #[serde(rename = "type")]
    pub account_type: String,
    /// Account subtype (e.g. `checking`, `savings`).
    pub subtype: Option<String>,
    /// Masked account number, when available.
    pub mask: Option<String>,
    /// Account balances.
    pub balances: PlaidBalancesRaw,
}

/// Account balances as returned by `/accounts/get`.
#[derive(Deserialize)]
pub struct PlaidBalancesRaw {
    /// Current balance, when available.
    pub current: Option<f64>,
    /// Available balance, when supported.
    pub available: Option<f64>,
    /// ISO 4217 currency code, when available.
    pub iso_currency_code: Option<String>,
}

/// A single page returned by Plaid's `/transactions/sync` endpoint.
#[derive(Deserialize)]
pub struct PlaidTransactionsSyncResponse {
    /// Newly added transactions.
    pub added: Vec<TransactionResponse>,
    /// Modified transactions.
    pub modified: Vec<TransactionResponse>,
    /// Removed transactions.
    pub removed: Vec<PlaidRemovedTransaction>,
    /// Cursor to pass on the next sync call.
    pub next_cursor: String,
    /// Whether more pages remain.
    pub has_more: bool,
}

/// Removed transaction reference as returned by `/transactions/sync`.
#[derive(Deserialize)]
pub struct PlaidRemovedTransaction {
    /// ID of the removed transaction.
    pub transaction_id: String,
}
