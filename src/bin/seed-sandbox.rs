//! Plaid Sandbox Data Seeder CLI.
//!
//! Seeds database with tester users, connects Plaid sandbox institutions with custom
//! credentials, and synchronizes financial accounts and transactions into `PostgreSQL`.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

use smart_finance::{db, plaid::client::PlaidClient};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct TesterFixture {
    email: String,
    first_name: String,
    last_name: String,
    password: String,
    institution_id: String,
    sandbox_username: String,
    sandbox_password: String,
}

#[derive(Debug, Deserialize)]
struct SandboxPublicTokenResponse {
    public_token: String,
}

/// Fire a sandbox webhook to trigger Plaid's transaction generator.
async fn fire_sandbox_webhook(
    http: &reqwest::Client,
    base_url: &str,
    client_id: &str,
    secret: &str,
    access_token: &str,
    webhook_code: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::json!({
        "client_id": client_id,
        "secret": secret,
        "access_token": access_token,
        "webhook_code": webhook_code
    });

    let resp = http
        .post(format!("{base_url}/sandbox/item/fire_webhook"))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        eprintln!(
            "  Warning: /sandbox/item/fire_webhook ({webhook_code}) returned {status}: {text}"
        );
    }
    Ok(())
}

/// Request a sandbox public token from Plaid using custom credentials.
async fn create_sandbox_public_token(
    http: &reqwest::Client,
    base_url: &str,
    client_id: &str,
    secret: &str,
    institution_id: &str,
    username: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let body = serde_json::json!({
        "client_id": client_id,
        "secret": secret,
        "institution_id": institution_id,
        "initial_products": ["transactions"],
        "options": {
            "override_username": username,
            "override_password": password
        }
    });

    let resp = http
        .post(format!("{base_url}/sandbox/public_token/create"))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Plaid /sandbox/public_token/create returned {status}: {text}").into());
    }

    let parsed: SandboxPublicTokenResponse = resp.json().await?;
    Ok(parsed.public_token)
}

/// Hashes a plaintext password using Argon2id.
fn hash_password(password: &str) -> Result<String, Box<dyn std::error::Error>> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("Password hashing failed: {e}"))?
        .to_string();
    Ok(hash)
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let reset = args.iter().any(|arg| arg == "--reset" || arg == "-r");
    let fixtures_path = args
        .windows(2)
        .find(|w| w[0] == "--fixtures")
        .map_or("fixtures/sandbox-users.json", |w| w[1].as_str());

    println!("==================================================");
    println!("       Smart Finance Plaid Sandbox Seeder         ");
    println!("==================================================");

    let client_id = env::var("PLAID_CLIENT_ID").unwrap_or_default();
    let secret = env::var("PLAID_SECRET").unwrap_or_default();
    let plaid_env = env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());

    if client_id.is_empty() || secret.is_empty() || client_id == "your_client_id" {
        eprintln!(
            "Error: PLAID_CLIENT_ID and PLAID_SECRET must be set in .env with valid credentials."
        );
        std::process::exit(1);
    }

    if plaid_env != "sandbox" && !args.iter().any(|arg| arg == "--force") {
        eprintln!("Safety error: PLAID_ENV is not set to 'sandbox' (found '{plaid_env}').");
        eprintln!("Use --force if you intentionally wish to proceed.");
        std::process::exit(1);
    }

    if !Path::new(fixtures_path).exists() {
        eprintln!("Error: Fixtures file '{fixtures_path}' not found.");
        std::process::exit(1);
    }

    let fixtures_raw = fs::read_to_string(fixtures_path)?;
    let fixtures: Vec<TesterFixture> = serde_json::from_str(&fixtures_raw)?;

    println!("Connecting to database...");
    let pool = db::create_pool().await;
    let plaid = PlaidClient::new();
    let http = reqwest::Client::new();
    let base_url = format!("https://{plaid_env}.plaid.com");

    for tester in fixtures {
        println!("\n--------------------------------------------------");
        println!(
            "Processing tester: {} ({})",
            tester.email, tester.sandbox_username
        );

        if reset {
            println!("  [--reset] Deleting existing user and cascading data...");
            sqlx::query!("DELETE FROM users WHERE email = $1", tester.email)
                .execute(&pool)
                .await?;
        }

        // Check if user already exists
        let existing = sqlx::query!("SELECT id FROM users WHERE email = $1", tester.email)
            .fetch_optional(&pool)
            .await?;

        let user_id = if let Some(row) = existing {
            println!("  User exists (id={}).", row.id);
            let link_count = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM plaid_links WHERE user_id = $1",
                row.id
            )
            .fetch_one(&pool)
            .await?
            .unwrap_or(0);

            if link_count > 0 && !reset {
                println!("  User already has {link_count} linked account(s). Skipping (use --reset to re-seed).");
                continue;
            }
            row.id
        } else {
            println!("  Creating user record...");
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
            .fetch_one(&pool)
            .await?;
            println!("  Created user (id={}).", row.id);
            row.id
        };

        println!(
            "  Requesting Plaid sandbox public token for '{}'...",
            tester.sandbox_username
        );
        let public_token = create_sandbox_public_token(
            &http,
            &base_url,
            &client_id,
            &secret,
            &tester.institution_id,
            &tester.sandbox_username,
            &tester.sandbox_password,
        )
        .await?;

        println!("  Exchanging public token for access token...");
        let exchange = plaid.exchange_public_token(&public_token).await?;

        // Insert plaid_links
        sqlx::query!(
            "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)
             ON CONFLICT (item_id) DO UPDATE SET access_token = EXCLUDED.access_token",
            user_id,
            exchange.item_id,
            exchange.access_token
        )
        .execute(&pool)
        .await?;

        // Seed sync cursor
        sqlx::query!(
            "INSERT INTO plaid_sync_cursors (item_id, cursor, has_more) VALUES ($1, NULL, TRUE)
             ON CONFLICT (item_id) DO NOTHING",
            exchange.item_id
        )
        .execute(&pool)
        .await?;

        println!("  Fetching financial accounts from Plaid...");
        let accounts = plaid.get_accounts(&exchange.access_token).await?;
        println!("  Found {} account(s). Saving...", accounts.len());

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
            .execute(&pool)
            .await?;
        }

        println!("  Triggering Plaid sandbox transaction generation...");
        let _ = fire_sandbox_webhook(
            &http,
            &base_url,
            &client_id,
            &secret,
            &exchange.access_token,
            "SYNC_UPDATES_AVAILABLE",
        )
        .await;

        println!("  Synchronizing transactions from Plaid...");
        let mut cursor: Option<String> = None;
        let mut total_tx = 0;

        for attempt in 1..=5 {
            let mut current_cursor = cursor.clone();
            let mut attempt_tx = 0;

            loop {
                let page = plaid
                    .sync_transactions(&exchange.access_token, current_cursor.clone())
                    .await?;
                let added_count = page.added.len();
                attempt_tx += added_count;

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
                    .execute(&pool)
                    .await?;
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
                .execute(&pool)
                .await?;

                current_cursor = Some(page.next_cursor);
                if !page.has_more {
                    break;
                }
            }

            total_tx += attempt_tx;
            cursor = current_cursor;

            if total_tx > 0 || attempt == 5 {
                break;
            }

            // In Plaid Sandbox, transactions take 1 second to populate after SYNC_UPDATES_AVAILABLE
            tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
            let _ = fire_sandbox_webhook(
                &http,
                &base_url,
                &client_id,
                &secret,
                &exchange.access_token,
                "SYNC_UPDATES_AVAILABLE",
            )
            .await;
        }

        println!("  Synced {total_tx} transaction(s).");
    }

    println!("\n==================================================");
    println!("✓ Sandbox seeding completed successfully!");
    println!("==================================================");
    Ok(())
}
