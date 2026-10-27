//! Plaid Sandbox Data Seeder CLI.
//!
//! Seeds database with tester users, connects Plaid sandbox institutions with custom
//! credentials, and synchronizes financial accounts and transactions into `PostgreSQL`.

use std::env;
use std::fs;
use std::path::Path;

use smart_finance::{
    db,
    plaid::client::PlaidClient,
    seeder::{seed_tester_profile, TesterFixture},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load local environment variables from .env
    dotenvy::dotenv().ok();

    // 1. Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    let reset = args.iter().any(|arg| arg == "--reset" || arg == "-r");
    let fixtures_path = args
        .windows(2)
        .find(|w| w[0] == "--fixtures")
        .map_or("fixtures/sandbox-users.json", |w| w[1].as_str());

    println!("==================================================");
    println!("       Smart Finance Plaid Sandbox Seeder         ");
    println!("==================================================");

    // 2. Validate environment configuration
    let client_id = env::var("PLAID_CLIENT_ID").unwrap_or_default();
    let secret = env::var("PLAID_SECRET").unwrap_or_default();
    let plaid_env = env::var("PLAID_ENV").unwrap_or_else(|_| "sandbox".to_string());

    if client_id.is_empty()
        || secret.is_empty()
        || client_id == "your_client_id"
        || secret == "your_secret"
    {
        eprintln!(
            "Error: PLAID_CLIENT_ID and PLAID_SECRET must be set in .env with valid credentials."
        );
        std::process::exit(1);
    }

    // Guard against running sandbox seeder against live production
    if plaid_env != "sandbox" && !args.iter().any(|arg| arg == "--force") {
        eprintln!("Safety error: PLAID_ENV is not set to 'sandbox' (found '{plaid_env}').");
        eprintln!("Use --force if you intentionally wish to proceed.");
        std::process::exit(1);
    }

    // 3. Load tester definitions from JSON fixtures file
    if !Path::new(fixtures_path).exists() {
        eprintln!("Error: Fixtures file '{fixtures_path}' not found.");
        std::process::exit(1);
    }

    let fixtures_raw = fs::read_to_string(fixtures_path)?;
    let fixtures: Vec<TesterFixture> = serde_json::from_str(&fixtures_raw)?;

    // 4. Initialize database pool and Plaid client
    println!("Connecting to database...");
    let pool = db::create_pool().await;
    let plaid = PlaidClient::new();

    // 5. Seed each tester profile using the core library seeder
    for tester in &fixtures {
        println!("\n--------------------------------------------------");
        println!(
            "Processing tester: {} ({})",
            tester.email, tester.sandbox_username
        );

        let res = seed_tester_profile(&pool, &plaid, tester, reset).await?;

        if res.skipped {
            println!(
                "  User already exists (id={}) with linked accounts. Skipping (use --reset to re-seed).",
                res.user_id
            );
        } else {
            if res.created_user {
                println!("  Created user (id={}).", res.user_id);
            } else {
                println!("  User exists (id={}).", res.user_id);
            }
            println!("  Linked bank connection (item_id={}).", res.item_id);
            println!("  Saved {} account(s).", res.accounts_saved);
            println!("  Synced {} transaction(s).", res.transactions_synced);
        }
    }

    println!("\n==================================================");
    println!("✓ Sandbox seeding completed successfully!");
    println!("==================================================");
    Ok(())
}
