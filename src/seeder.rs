//! Plaid Sandbox database seeding logic.
//!
//! Exposes functions to seed `PostgreSQL` with pre-configured tester users,
//! linked Plaid institutions, financial accounts, and synchronized transactions.

use crate::{error::AppError, plaid::client::PlaidClient};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use chrono::{Datelike, Months, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

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

/// Adds a realistic volume of SYNTHETIC transactions to an
/// existing sandbox tester's connected checking account.
///
/// This does not call Plaid or change account balances.
/// The inserted transactions are only for dashboard testing.
///
/// Each transaction receives a stable, unique identifier.
/// Re-running this function will not duplicate existing rows.
///
/// Returns the number of newly inserted transactions.
///
/// # Errors
///
/// Returns an AppError if a database operation fails.
pub async fn seed_demo_transactions(pool: &PgPool, user_id: i32) -> Result<usize, AppError> {
    // Find one existing checking/depository account for this
    // specific user. We don't create fake Plaid connections.
    //
    // We deliberately avoid credit and loan accounts to
    // prevent the demo expenses from being counted twice.
    let account_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT a.account_id
        FROM plaid_accounts a
        JOIN plaid_links l ON l.item_id = a.item_id
        WHERE l.user_id = $1
          AND a.account_type = 'depository'
        ORDER BY a.id
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    // Without a checking/depository account, there's no safe
    // existing account to attach demo transactions to.
    let Some(account_id) = account_id else {
        tracing::warn!("Skipping demo transactions: user {user_id} has no depository account");
        return Ok(0);
    };

    // Eight spending categories, matching the Rust Category
    // enum and the database CHECK constraint.
    //
    // Names and amounts vary so the dashboard charts have
    // realistic-looking proportions rather than equal slices.
    let merchants: [(&str, &str, f64); 16] = [
        ("Supermarket", "GROCERIES", 68.25),
        ("Farmers Market", "GROCERIES", 34.80),
        ("Coffee Shop", "FOOD_DINING", 8.75),
        ("Restaurant", "FOOD_DINING", 46.50),
        ("Clothing Store", "SHOPPING", 82.40),
        ("Online Retailer", "SHOPPING", 39.95),
        ("Gas Station", "TRANSPORTATION", 42.60),
        ("Train Ticket", "TRANSPORTATION", 18.00),
        ("Movie Theater", "ENTERTAINMENT", 27.50),
        ("Streaming Service", "ENTERTAINMENT", 15.99),
        ("Electric Company", "UTILITIES", 112.00),
        ("Internet Provider", "UTILITIES", 64.99),
        ("Pharmacy", "HEALTHCARE", 22.35),
        ("Doctor Copay", "HEALTHCARE", 30.00),
        ("Pet Supplies", "OTHER", 24.90),
        ("Household Supplies", "OTHER", 19.75),
    ];

    let today = Utc::now().date_naive();

    // Always use the first of the month as the anchor.
    // This avoids issues when subtracting months from dates
    // like January 31 or March 31.
    let current_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
        .expect("Current calendar month must be valid");

    let mut inserted = 0_usize;

    // Generate up to twelve months of history:
    // current month plus the previous eleven months.
    for month_offset in 0..12_u32 {
        let month = current_month
            .checked_sub_months(Months::new(month_offset))
            .expect("Demo month must be valid");

        // Generate 32 transactions per complete month.
        //
        // Each merchant occurs twice, with a slightly
        // different amount on each occurrence.
        for index in 0..32_usize {
            let (merchant, category, base_amount) = merchants[index % merchants.len()];

            // Restrict generated dates to days 1 through 28.
            // Those days exist in every calendar month.
            let day = (index % 28 + 1) as u32;

            let date = NaiveDate::from_ymd_opt(month.year(), month.month(), day)
                .expect("Day 1 through 28 must be valid");

            // Don't create transactions dated in the future.
            if date > today {
                continue;
            }

            // Deterministic variation makes consecutive
            // transactions more realistic and makes repeat
            // runs reproducible.
            let variation = 1.0 + ((index * 7 + month_offset as usize) % 11) as f64 / 100.0;

            let amount = (base_amount * variation * 100.0).round() / 100.0;

            // Unique per user, month, and transaction index.
            //
            // The prefix clearly distinguishes our synthetic
            // test data from genuine Plaid transaction IDs.
            let transaction_id = format!(
                "smart-finance-demo-v1-{user_id}-{:04}{:02}-{index:02}",
                month.year(),
                month.month(),
            );

            // Use a SQL transaction ID conflict check so
            // re-running the generator never duplicates rows.
            //
            // category_overridden = TRUE prevents the Plaid
            // category synchronization logic from replacing
            // these intentional demonstration categories.
            let result = sqlx::query(
                r#"
                INSERT INTO plaid_transactions
                    (
                        transaction_id,
                        account_id,
                        name,
                        amount,
                        date,
                        pending,
                        category,
                        category_overridden
                    )
                VALUES ($1, $2, $3, $4, $5, FALSE, $6, TRUE)
                ON CONFLICT (transaction_id) DO NOTHING
                "#,
            )
            .bind(transaction_id)
            .bind(&account_id)
            .bind(merchant)
            .bind(amount)
            .bind(date)
            .bind(category)
            .execute(pool)
            .await
            .map_err(AppError::from)?;

            // rows_affected() is 1 for a new insertion and
            // 0 when this transaction already exists.
            inserted += result.rows_affected() as usize;
        }
    }

    Ok(inserted)
}

/// Generate ten years of synthetic monthly net-worth history.
///
/// This is for LOCAL SANDBOX DEMONSTRATIONS ONLY.
///
/// Unlike real snapshots, generated records have is_demo = TRUE.
///
/// Safety rules:
/// - An existing snapshot for today is required as an anchor.
/// - Today's snapshot is never changed.
/// - Existing historical snapshots are never overwritten.
/// - No Plaid API calls are made.
/// - Repeated runs never duplicate history.
/// - Each user receives a slightly different trajectory.
///
/// Returns the number of newly inserted snapshots.
///
/// # Errors
///
/// Returns an AppError if a database operation fails.
pub async fn seed_demo_net_worth(pool: &PgPool, user_id: i32) -> Result<usize, AppError> {
    // Find today's real net-worth snapshot.
    //
    // The dashboard records this when it successfully
    // loads connected accounts.
    //
    // We require an existing snapshot so our generated
    // history has a sensible ending value.
    let today: Option<(f64, i32, i32)> = sqlx::query_as(
        r#"
        SELECT
            amount,
            included_accounts,
            excluded_accounts
        FROM net_worth_snapshots
        WHERE user_id = $1
          AND snapshot_date = CURRENT_DATE
          AND is_demo = FALSE
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    let Some((current_amount, included, excluded)) = today else {
        tracing::warn!(
            "Skipping demo net worth for user {user_id}: \
             no real snapshot exists for today"
        );

        return Ok(0);
    };

    // The current value must be a valid finite number.
    if !current_amount.is_finite() {
        return Err(AppError::Internal(
            "Cannot generate demo history from an invalid net worth".to_string(),
        ));
    }

    let today_date = Utc::now().date_naive();

    // Anchor all historical dates to the first day of
    // their respective calendar months.
    let current_month = NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1)
        .expect("Current month must be valid");

    let mut inserted = 0_usize;

    // Generate the 120 COMPLETE months before this month.
    //
    // We do not generate a synthetic record for today.
    // Month 1 = previous month.
    // Month 120 = ten years ago.
    for months_ago in 1..=120_u32 {
        let date = current_month
            .checked_sub_months(Months::new(months_ago))
            .expect("Historical demo date must be valid");

        // 0.0 = oldest month
        // 1.0 = newest historical month
        //
        // This produces an overall upward trajectory
        // while allowing some monthly fluctuations.
        let progress = (120 - months_ago) as f64 / 119.0;

        // Start at approximately 35% of today's value,
        // then gradually approach today's value.
        let trend = current_amount * (0.35 + 0.65 * progress);

        // Create a deterministic user-specific fluctuation.
        //
        // Sine waves create ups and downs without requiring
        // random-number dependencies. Different user IDs
        // produce different sequences.
        let user_phase = user_id as f64 * 0.71;
        let month_phase = months_ago as f64;

        let fluctuation = (month_phase * 0.63 + user_phase).sin() * current_amount.abs() * 0.035
            + (month_phase * 0.19 + user_phase).cos() * current_amount.abs() * 0.015;

        // Round to normal currency precision.
        let amount = ((trend + fluctuation) * 100.0).round() / 100.0;

        // Never insert an invalid numeric value.
        if !amount.is_finite() {
            return Err(AppError::Internal(
                "Generated net-worth amount is invalid".to_string(),
            ));
        }

        // INSERT ... ON CONFLICT DO NOTHING means:
        //
        // 1. We never overwrite real historical data.
        // 2. We never overwrite a previous demo snapshot.
        // 3. Running this twice never creates duplicates.
        //
        // The new is_demo column explicitly identifies
        // this record as synthetic test data.
        let result = sqlx::query(
            r#"
            INSERT INTO net_worth_snapshots (
                user_id,
                snapshot_date,
                amount,
                included_accounts,
                excluded_accounts,
                is_demo
            )
            VALUES ($1, $2, $3, $4, $5, TRUE)
            ON CONFLICT (user_id, snapshot_date)
            DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(date)
        .bind(amount)
        .bind(included)
        .bind(excluded)
        .execute(pool)
        .await
        .map_err(AppError::from)?;

        inserted += result.rows_affected() as usize;
    }

    Ok(inserted)
}
