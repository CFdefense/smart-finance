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
    seeder::{seed_demo_net_worth, seed_demo_transactions, seed_tester_profile, TesterFixture},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load local environment variables from .env
    dotenvy::dotenv().ok();

    // 1. Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    let reset = args.iter().any(|arg| arg == "--reset" || arg == "-r");

    // Optional synthetic transaction generation.
    //
    // Unlike --reset, this adds test transactions without
    // deleting existing accounts, users, or transactions.
    let demo_transactions = args.iter().any(|arg| arg == "--demo-transactions");

    // Enable synthetic net-worth history generation.
    //
    // This is separate from --demo-transactions so either
    // test dataset can be generated independently.
    let demo_net_worth = args.iter().any(|arg| arg == "--demo-net-worth");

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

    // ------------------------------------------------------------
    // SAFETY CHECKS FOR SYNTHETIC DEMO TRANSACTIONS
    // ------------------------------------------------------------

    // Synthetic dashboard fixtures must never be written
    // while the application is configured for production.
    //
    // This restriction applies even if someone uses --force.
    if demo_transactions && plaid_env != "sandbox" {
        eprintln!("Safety error: demo transactions require PLAID_ENV=sandbox.");
        std::process::exit(1);
    }

    // Avoid accidentally deleting existing sandbox users
    // when the intention is only to add demo transactions.
    if demo_transactions && reset {
        eprintln!("Safety error: do not combine --demo-transactions with --reset.");
        std::process::exit(1);
    }

    // ------------------------------------------------------------
    // SAFETY CHECKS FOR SYNTHETIC NET-WORTH HISTORY
    // ------------------------------------------------------------

    // Never insert demo history outside Plaid's sandbox mode,
    // including when the --force argument was supplied.
    if demo_net_worth && plaid_env != "sandbox" {
        eprintln!("Safety error: demo net-worth history requires PLAID_ENV=sandbox.");
        std::process::exit(1);
    }

    // Generating history must not reset or delete existing
    // sandbox users and their financial information.
    if demo_net_worth && reset {
        eprintln!("Safety error: do not combine --demo-net-worth with --reset.");
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

        // Demo flags are deliberately scoped to Arthur. Never reseed the
        // four original team test accounts as a side effect of demo runs.
        if (demo_transactions || demo_net_worth) && tester.email != "arthur@test.com" {
            println!("  Original tester preserved; demo data is Arthur-only.");
            continue;
        }

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

        // --------------------------------------------------------
        // ADD SYNTHETIC TRANSACTIONS WHEN REQUESTED
        // --------------------------------------------------------
        //
        // This code runs for BOTH existing and newly seeded users.
        //
        // That is important because seed_tester_profile() normally
        // skips users who already have linked Plaid accounts.
        //
        // We want to add more transactions WITHOUT resetting
        // existing tester accounts.
        if demo_transactions && tester.email == "arthur@test.com" {
            let count = seed_demo_transactions(&pool, res.user_id).await?;

            println!("  Added {count} synthetic transactions for dashboard testing.");
        }

        // Add synthetic historical net-worth snapshots.
        //
        // This works for already-seeded users, because res.user_id
        // is still available when seed_tester_profile() skips them.
        //
        // A valid snapshot for today must already exist.
        if demo_net_worth && tester.email == "arthur@test.com" {
            let count = seed_demo_net_worth(&pool, res.user_id).await?;

            println!("  Added {count} synthetic monthly net-worth snapshots.");

            if count == 0 {
                println!(
                    "  No new snapshots inserted. History may already \
             exist, or today's snapshot may be missing."
                );
            }
        }
    }

    // 6. Print completion banner

    println!("\n==================================================");
    println!("✓ Sandbox seeding completed successfully!");
    println!("==================================================");
    Ok(())
}
