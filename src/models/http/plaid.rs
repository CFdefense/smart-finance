//! HTTP request and response models for plaid routes.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to exchange a Plaid public token.
#[derive(Deserialize, ToSchema)]
pub struct ExchangeTokenRequest {
    /// Temporary public token returned by Plaid Link after successful linking.
    pub public_token: String,
}

/// Response containing a token used to initialize Plaid Link.
#[derive(Serialize, ToSchema)]
pub struct LinkTokenResponse {
    /// Short-lived token used by the frontend to launch Plaid Link.
    pub link_token: String,
}

/// Response confirming that a bank connection was saved.
#[derive(Serialize, ToSchema)]
pub struct ExchangeTokenResponse {
    /// Indicates whether the connection was successfully saved.
    pub success: bool,
    /// Plaid Item ID identifying the linked financial institution connection.
    pub item_id: String,
}

/// Response containing Plaid financial accounts.
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountsResponse {
    /// Financial accounts linked by the authenticated user.
    pub accounts: Vec<AccountResponse>,
}

/// Singular Plaid financial account.
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
#[derive(Debug, Serialize, ToSchema)]
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
    pub date: String,
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
