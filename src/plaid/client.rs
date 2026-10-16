//! Plaid HTTP client — initialized once at startup, shared via `Extension`.

use crate::error::AppError;
use chrono::NaiveDate;
use serde::Deserialize;

// ── Public data types returned by PlaidClient methods ────────────────────────

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

// ── Private Plaid API response shapes ────────────────────────────────────────

#[derive(Deserialize)]
struct LinkTokenCreateResponse {
    link_token: String,
}

#[derive(Deserialize)]
struct ExchangeTokenResponse {
    access_token: String,
    item_id: String,
}

#[derive(Deserialize)]
struct AccountsGetResponse {
    accounts: Vec<PlaidAccountRaw>,
    item: PlaidItemRaw,
}

#[derive(Deserialize)]
struct PlaidItemRaw {
    item_id: String,
}

#[derive(Deserialize)]
struct PlaidAccountRaw {
    account_id: String,
    name: String,
    official_name: Option<String>,
    #[serde(rename = "type")]
    account_type: String,
    subtype: Option<String>,
    mask: Option<String>,
    balances: PlaidBalancesRaw,
}

#[derive(Deserialize)]
struct PlaidBalancesRaw {
    current: Option<f64>,
    available: Option<f64>,
    iso_currency_code: Option<String>,
}

#[derive(Deserialize)]
struct TransactionsSyncResponse {
    added: Vec<PlaidTransactionRaw>,
    modified: Vec<PlaidTransactionRaw>,
    removed: Vec<PlaidRemovedTransaction>,
    next_cursor: String,
    has_more: bool,
}

#[derive(Deserialize)]
struct PlaidTransactionRaw {
    transaction_id: String,
    account_id: String,
    name: String,
    amount: f64,
    date: NaiveDate,
    pending: bool,
}

#[derive(Deserialize)]
struct PlaidRemovedTransaction {
    transaction_id: String,
}

// ── PlaidClient ───────────────────────────────────────────────────────────────

/// Shared HTTP client for the Plaid API.
///
/// Initialized once at startup from environment variables and injected as an
/// Axum [`axum::Extension`] alongside the `PgPool` and cookie `Key`.
///
/// # Panics
///
/// [`PlaidClient::new`] panics if any required env var is missing.
#[derive(Clone)]
pub struct PlaidClient {
    http: reqwest::Client,
    base_url: String,
    client_id: String,
    secret: String,
    client_name: String,
}

impl Default for PlaidClient {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaidClient {
    /// Build a new `PlaidClient` from environment variables.
    ///
    /// Reads `PLAID_CLIENT_ID`, `PLAID_SECRET`, `PLAID_ENV` (`sandbox` or
    /// `production`), and `PLAID_CLIENT_NAME`.
    ///
    /// Missing credentials default to empty strings; the client will boot but
    /// any actual Plaid API call will return `AppError::Internal` with a
    /// non-2xx error from Plaid. Set all four variables for production use.
    pub fn new() -> Self {
        let client_id = std::env::var("PLAID_CLIENT_ID").unwrap_or_default();
        let secret = std::env::var("PLAID_SECRET").unwrap_or_default();
        let env = std::env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());
        let client_name = std::env::var("PLAID_CLIENT_NAME")
            .unwrap_or_else(|_| "Smart Finance".to_string());

        let base_url = format!("https://{env}.plaid.com");

        Self {
            http: reqwest::Client::new(),
            base_url,
            client_id,
            secret,
            client_name,
        }
    }

    /// Create a Plaid Link token for the given application user.
    ///
    /// Calls `POST /link/token/create`. The returned token is passed to the
    /// frontend to initialize the Plaid Link UI.
    pub async fn create_link_token(&self, user_id: i32) -> Result<String, AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "secret": self.secret,
            "client_name": self.client_name,
            "user": { "client_user_id": user_id.to_string() },
            "products": ["transactions"],
            "country_codes": ["US"],
            "language": "en"
        });

        let resp = self
            .http
            .post(format!("{}/link/token/create", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Plaid /link/token/create returned {status}: {text}"
            )));
        }

        let parsed: LinkTokenCreateResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        Ok(parsed.link_token)
    }

    /// Exchange a public token for an access token and Item ID.
    ///
    /// Calls `POST /item/public_token/exchange`. Returns `(access_token, item_id)`.
    pub async fn exchange_public_token(
        &self,
        public_token: &str,
    ) -> Result<(String, String), AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "secret": self.secret,
            "public_token": public_token
        });

        let resp = self
            .http
            .post(format!("{}/item/public_token/exchange", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Plaid /item/public_token/exchange returned {status}: {text}"
            )));
        }

        let parsed: ExchangeTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        Ok((parsed.access_token, parsed.item_id))
    }

    /// Fetch all financial accounts for a given access token.
    ///
    /// Calls `POST /accounts/get`.
    pub async fn get_accounts(
        &self,
        access_token: &str,
    ) -> Result<Vec<PlaidAccountData>, AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "secret": self.secret,
            "access_token": access_token
        });

        let resp = self
            .http
            .post(format!("{}/accounts/get", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Plaid /accounts/get returned {status}: {text}"
            )));
        }

        let parsed: AccountsGetResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        let item_id = parsed.item.item_id;
        let accounts = parsed
            .accounts
            .into_iter()
            .map(|a| PlaidAccountData {
                account_id: a.account_id,
                item_id: item_id.clone(),
                name: a.name,
                official_name: a.official_name,
                account_type: a.account_type,
                account_subtype: a.subtype,
                mask: a.mask,
                current_balance: a.balances.current,
                available_balance: a.balances.available,
                currency_code: a.balances.iso_currency_code,
            })
            .collect();

        Ok(accounts)
    }

    /// Fetch one page of transaction changes from Plaid's Transactions Sync API.
    ///
    /// Calls `POST /transactions/sync`. Pass `cursor = None` for the first sync.
    /// Keep calling with the returned `next_cursor` while `has_more` is `true`.
    pub async fn sync_transactions(
        &self,
        access_token: &str,
        cursor: Option<String>,
    ) -> Result<PlaidSyncPage, AppError> {
        let mut body = serde_json::json!({
            "client_id": self.client_id,
            "secret": self.secret,
            "access_token": access_token,
            "count": 100
        });

        if let Some(c) = cursor {
            body["cursor"] = serde_json::Value::String(c);
        }

        let resp = self
            .http
            .post(format!("{}/transactions/sync", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Plaid /transactions/sync returned {status}: {text}"
            )));
        }

        let parsed: TransactionsSyncResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        let map_tx = |t: PlaidTransactionRaw| PlaidTransactionData {
            transaction_id: t.transaction_id,
            account_id: t.account_id,
            name: t.name,
            amount: t.amount,
            date: t.date,
            pending: t.pending,
        };

        Ok(PlaidSyncPage {
            added: parsed.added.into_iter().map(map_tx).collect(),
            modified: parsed.modified.into_iter().map(map_tx).collect(),
            removed: parsed
                .removed
                .into_iter()
                .map(|r| r.transaction_id)
                .collect(),
            next_cursor: parsed.next_cursor,
            has_more: parsed.has_more,
        })
    }
}
