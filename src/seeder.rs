//! Plaid Sandbox database seeding logic.
//!
//! Exposes functions to seed `PostgreSQL` with pre-configured tester users,
//! linked Plaid institutions, financial accounts, and synchronized transactions.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{error::AppError, plaid::client::PlaidClient};

/// Represents a single tester configuration from fixtures.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TesterFixture {
    /// Email used for tester login in the application.
    pub email: String,
    /// Tester's first name.
    pub first_name: String,
    /// Tester's last name.
    pub last_name: String,
    /// Plaintext password for application login.
    pub password: String,
    /// Plaid institution identifier (e.g. `ins_109508` for First Platypus Bank).
    pub institution_id: String,
    /// Custom Plaid sandbox username (e.g. `custom_jfrank`).
    pub sandbox_username: String,
    /// Custom Plaid sandbox password (e.g. `pass_good`).
    pub sandbox_password: String,
}

/// Results of seeding a single tester profile.
#[derive(Debug, Default)]
pub struct SeedResult {
    /// Database user ID.
    pub user_id: i32,
    /// Whether a new user was created.
    pub created_user: bool,
    /// Plaid Item ID.
    pub item_id: String,
    /// Number of financial accounts saved.
    pub accounts_saved: usize,
    /// Number of transactions synchronized.
    pub transactions_synced: usize,
    /// Whether seeding was skipped because accounts were already linked.
    pub skipped: bool,
}

/// Hashes a plaintext password using Argon2id matching backend authentication rules.
///
/// # Errors
///
/// Returns `AppError::Internal` if password hashing fails.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {e}")))
}

/// Seed a single tester profile into `PostgreSQL` using Plaid Sandbox.
///
/// # Errors
///
/// Returns `AppError` if database operations or Plaid API calls fail.
#[allow(clippy::too_many_lines)]
pub async fn seed_tester_profile(
    pool: &PgPool,
    plaid: &PlaidClient,
    tester: &TesterFixture,
    reset: bool,
) -> Result<SeedResult, AppError> {
    let mut result = SeedResult::default();

    if reset {
        sqlx::query!("DELETE FROM users WHERE email = $1", tester.email)
            .execute(pool)
            .await
            .map_err(AppError::from)?;
    }

    // Check if user already exists
    let existing = sqlx::query!("SELECT id FROM users WHERE email = $1", tester.email)
        .fetch_optional(pool)
        .await
        .map_err(AppError::from)?;

    let user_id = if let Some(row) = existing {
        let link_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM plaid_links WHERE user_id = $1",
            row.id
        )
        .fetch_one(pool)
        .await
        .map_err(AppError::from)?
        .unwrap_or(0);

        if link_count > 0 && !reset {
            result.user_id = row.id;
            result.skipped = true;
            return Ok(result);
        }
        row.id
    } else {
        let password_hash = hash_password(&tester.password)?;
        let row = sqlx::query!(
            "INSERT INTO users (email, password_hash, first_name, last_name, account_status)
             VALUES ($1, $2, $3, $4, 'active')
             RETURNING id",
            tester.email,
            password_hash,
            tester.first_name,
            tester.last_name
        )
        .fetch_one(pool)
        .await
        .map_err(AppError::from)?;

        result.created_user = true;
        row.id
    };

    result.user_id = user_id;

    // 1. Create sandbox public token via PlaidClient
    let public_token = plaid
        .create_sandbox_public_token(
            &tester.institution_id,
            &tester.sandbox_username,
            &tester.sandbox_password,
        )
        .await?;

    // 2. Exchange public token for access token and item ID
    let exchange = plaid.exchange_public_token(&public_token).await?;
    result.item_id.clone_from(&exchange.item_id);

    // 3. Persist bank link
    sqlx::query!(
        "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)
         ON CONFLICT (item_id) DO UPDATE SET user_id = EXCLUDED.user_id, access_token = EXCLUDED.access_token",
        user_id,
        exchange.item_id,
        exchange.access_token
    )
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    // 4. Initialize sync cursor
    sqlx::query!(
        "INSERT INTO plaid_sync_cursors (item_id, cursor, has_more) VALUES ($1, NULL, TRUE)
         ON CONFLICT (item_id) DO NOTHING",
        exchange.item_id
    )
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    // 5. Fetch and store accounts
    let accounts = plaid.get_accounts(&exchange.access_token).await?;
    result.accounts_saved = accounts.len();

    for a in &accounts {
        sqlx::query!(
            r#"
            INSERT INTO plaid_accounts
                (account_id, item_id, name, official_name, account_type, account_subtype,
                 mask, current_balance, available_balance, currency_code, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            ON CONFLICT (account_id) DO UPDATE SET
                name              = EXCLUDED.name,
                official_name     = EXCLUDED.official_name,
                account_type      = EXCLUDED.account_type,
                account_subtype   = EXCLUDED.account_subtype,
                mask              = EXCLUDED.mask,
                current_balance   = EXCLUDED.current_balance,
                available_balance = EXCLUDED.available_balance,
                currency_code     = EXCLUDED.currency_code,
                updated_at        = NOW()
            "#,
            a.account_id,
            a.item_id,
            a.name,
            a.official_name,
            a.account_type,
            a.account_subtype,
            a.mask,
            a.current_balance,
            a.available_balance,
            a.currency_code
        )
        .execute(pool)
        .await
        .map_err(AppError::from)?;
    }

    // 6. Signal sandbox transaction generator
    let _ = plaid
        .fire_sandbox_webhook(&exchange.access_token, "SYNC_UPDATES_AVAILABLE")
        .await;

    // 7. Poll and synchronize transactions
    let mut cursor: Option<String> = None;
    let mut total_tx = 0;
    let mut any_sync_succeeded = false;

    for attempt in 1..=5 {
        let mut current_cursor = cursor.clone();
        let mut attempt_tx = 0;
        let mut sync_failed = false;

        loop {
            let sync_res = plaid
                .sync_transactions(&exchange.access_token, current_cursor.clone())
                .await;

            let page = match sync_res {
                Ok(p) => p,
                Err(e) => {
                    // Tolerates PRODUCT_NOT_READY or transient generation delays during polling
                    tracing::warn!("Transactions sync attempt {attempt} returned: {e}");
                    sync_failed = true;
                    break;
                }
            };

            attempt_tx += page.added.len();

            for tx in &page.added {
                sqlx::query!(
                    r#"
                    INSERT INTO plaid_transactions
                        (transaction_id, account_id, name, amount, date, pending, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, NOW())
                    ON CONFLICT (transaction_id) DO UPDATE SET
                        name       = EXCLUDED.name,
                        amount     = EXCLUDED.amount,
                        date       = EXCLUDED.date,
                        pending    = EXCLUDED.pending,
                        updated_at = NOW()
                    "#,
                    tx.transaction_id,
                    tx.account_id,
                    tx.name,
                    tx.amount,
                    tx.date,
                    tx.pending
                )
                .execute(pool)
                .await
                .map_err(AppError::from)?;
            }

            sqlx::query!(
                r#"
                INSERT INTO plaid_sync_cursors (item_id, cursor, has_more, updated_at)
                VALUES ($1, $2, $3, NOW())
                ON CONFLICT (item_id) DO UPDATE SET
                    cursor     = EXCLUDED.cursor,
                    has_more   = EXCLUDED.has_more,
                    updated_at = NOW()
                "#,
                exchange.item_id,
                page.next_cursor,
                page.has_more
            )
            .execute(pool)
            .await
            .map_err(AppError::from)?;

            current_cursor = Some(page.next_cursor);
            if !page.has_more {
                break;
            }
        }

        if !sync_failed {
            any_sync_succeeded = true;
            total_tx += attempt_tx;
            cursor = current_cursor;
        }

        if total_tx > 0 || attempt == 5 {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        let _ = plaid
            .fire_sandbox_webhook(&exchange.access_token, "SYNC_UPDATES_AVAILABLE")
            .await;
    }

    if !any_sync_succeeded {
        return Err(AppError::Internal(
            "transaction sync failed on all 5 attempts".to_string(),
        ));
    }

    result.transactions_synced = total_tx;
    Ok(result)
}
