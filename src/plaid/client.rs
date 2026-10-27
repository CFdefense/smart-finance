//! Plaid HTTP client — initialized once at startup, shared via `Extension`.

use crate::{
    error::AppError,
    models::http::plaid::{
        AccountResponse, ExchangeTokenResponse, LinkTokenResponse, PlaidAccountsGetResponse,
        PlaidTransactionsSyncResponse,
    },
};
use std::time::Duration;

/// Shared HTTP client for the Plaid API.
///
/// Initialized once at startup from environment variables and injected as an
/// Axum [`axum::Extension`] alongside the `PgPool` and cookie `Key`.
///
/// Missing `PLAID_CLIENT_ID` or `PLAID_SECRET` credentials default to empty
/// strings so the server boots without Plaid configured (useful in tests).
/// Actual Plaid API calls will return `AppError::Internal` until real
/// credentials are set.
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
    ///
    /// # Panics
    ///
    /// Panics if the underlying `reqwest::Client` cannot be built (should not
    /// happen in normal operation).
    pub fn new() -> Self {
        let client_id = std::env::var("PLAID_CLIENT_ID").unwrap_or_default();
        let secret = std::env::var("PLAID_SECRET").unwrap_or_default();
        let env = std::env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());
        let client_name =
            std::env::var("PLAID_CLIENT_NAME").unwrap_or_else(|_| "Smart Finance".to_string());

        let base_url = format!("https://{env}.plaid.com");

        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
            base_url,
            client_id,
            secret,
            client_name,
        }
    }

    /// Build a `PlaidClient` that talks to `base_url` instead of the Plaid
    /// host derived from `PLAID_ENV`. Used to point tests at a local mock.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let mut client = Self::new();
        client.base_url = base_url.into();
        client
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

        let parsed: LinkTokenResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        Ok(parsed.link_token)
    }

    /// Exchange a public token for an access token and Item ID.
    ///
    /// Calls `POST /item/public_token/exchange`. The returned `success` flag
    /// is `false`; the caller sets it once the connection is persisted.
    pub async fn exchange_public_token(
        &self,
        public_token: &str,
    ) -> Result<ExchangeTokenResponse, AppError> {
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

        resp.json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))
    }

    /// Fetch all financial accounts for a given access token.
    ///
    /// Calls `POST /accounts/get`.
    pub async fn get_accounts(&self, access_token: &str) -> Result<Vec<AccountResponse>, AppError> {
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

        let parsed: PlaidAccountsGetResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        let item_id = parsed.item.item_id;
        let accounts = parsed
            .accounts
            .into_iter()
            .map(|a| AccountResponse {
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
    ) -> Result<PlaidTransactionsSyncResponse, AppError> {
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

        resp.json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))
    }

    /// Create a sandbox public token with custom user credentials.
    ///
    /// Calls `POST /sandbox/public_token/create`. Allows automated linking of
    /// specific sandbox user personas without browser UI interaction.
    pub async fn create_sandbox_public_token(
        &self,
        institution_id: &str,
        username: &str,
        password: &str,
    ) -> Result<String, AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "secret": self.secret,
            "institution_id": institution_id,
            "initial_products": ["transactions"],
            "options": {
                "override_username": username,
                "override_password": password
            }
        });

        let resp = self
            .http
            .post(format!("{}/sandbox/public_token/create", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Plaid /sandbox/public_token/create returned {status}: {text}"
            )));
        }

        let parsed: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid response parse error: {e}")))?;

        parsed
            .get("public_token")
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| {
                AppError::Internal("Missing public_token in Plaid sandbox response".to_string())
            })
    }

    /// Fire a webhook simulation event in Plaid Sandbox.
    ///
    /// Calls `POST /sandbox/item/fire_webhook` with the specified `webhook_code`
    /// (e.g. `SYNC_UPDATES_AVAILABLE` or `DEFAULT_UPDATE`) to trigger mock
    /// transaction processing.
    pub async fn fire_sandbox_webhook(
        &self,
        access_token: &str,
        webhook_code: &str,
    ) -> Result<(), AppError> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "secret": self.secret,
            "access_token": access_token,
            "webhook_code": webhook_code
        });

        let resp = self
            .http
            .post(format!("{}/sandbox/item/fire_webhook", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Plaid request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Plaid /sandbox/item/fire_webhook returned {status}: {text}"
            )));
        }

        Ok(())
    }
}
