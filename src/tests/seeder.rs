//! Tests for sandbox fixtures and seeder configuration.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use super::fixtures::{cleanup, mock_plaid_url, test_pool, unique_email};

use tokio::sync::Mutex;

/// Prevent these two seeder integration tests from using
/// their shared mock Plaid identifiers simultaneously.
///
/// Other unrelated Rust tests can still run concurrently.
static SEEDER_TEST_LOCK: Mutex<()> = Mutex::const_new(());

use crate::{
    models::http::user::SignupRequest,
    plaid::client::PlaidClient,
    seeder::{
        hash_password, seed_demo_net_worth, seed_demo_transactions, seed_tester_profile,
        TesterFixture,
    },
};

#[test]
fn test_sandbox_fixtures_validity() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixtures_path = std::path::Path::new(manifest_dir).join("fixtures/sandbox-users.json");

    assert!(
        fixtures_path.exists(),
        "fixtures/sandbox-users.json must exist"
    );

    let content = std::fs::read_to_string(&fixtures_path).expect("failed to read fixtures file");
    let fixtures: Vec<TesterFixture> =
        serde_json::from_str(&content).expect("fixtures must be valid JSON");

    assert_eq!(
        fixtures.len(),
        5,
        "expected 5 tester fixtures including Arthur"
    );

    for tester in fixtures {
        assert!(
            SignupRequest::validate_email(&tester.email),
            "invalid email format for {}",
            tester.email
        );
        assert!(
            SignupRequest::validate_password(&tester.password).is_ok(),
            "invalid password for {}",
            tester.email
        );
        assert!(
            !tester.first_name.is_empty(),
            "first_name must not be empty"
        );
        assert!(!tester.last_name.is_empty(), "last_name must not be empty");
        assert!(
            !tester.institution_id.is_empty(),
            "institution_id must not be empty"
        );
        assert!(
            !tester.sandbox_username.is_empty(),
            "sandbox_username must not be empty"
        );
        assert!(
            !tester.sandbox_password.is_empty(),
            "sandbox_password must not be empty"
        );

        // Verify password hashing works with Argon2
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(tester.password.as_bytes(), &salt)
            .expect("hashing should succeed")
            .to_string();

        let parsed_hash = PasswordHash::new(&hash).expect("hash parsing should succeed");
        assert!(
            Argon2::default()
                .verify_password(tester.password.as_bytes(), &parsed_hash)
                .is_ok(),
            "password hash verification failed for {}",
            tester.email
        );

        // Verify seeder's hash_password helper
        let helper_hash = hash_password(&tester.password).expect("hash_password should succeed");
        let parsed_helper = PasswordHash::new(&helper_hash).expect("hash parsing should succeed");
        assert!(
            Argon2::default()
                .verify_password(tester.password.as_bytes(), &parsed_helper)
                .is_ok(),
            "helper hash verification failed for {}",
            tester.email
        );
    }
}

#[tokio::test]
async fn test_seed_tester_profile_lifecycle() {
    // Hold this lock throughout the test so the demo
    // transaction test cannot modify shared mock state.
    let _guard = SEEDER_TEST_LOCK.lock().await;

    let pool = test_pool().await;

    let plaid = PlaidClient::with_base_url(mock_plaid_url());
    let email = unique_email("seed_test");

    let tester = TesterFixture {
        email: email.clone(),
        first_name: "Test".to_string(),
        last_name: "User".to_string(),
        password: "Password123".to_string(),
        institution_id: "ins_109508".to_string(),
        sandbox_username: "custom_test".to_string(),
        sandbox_password: "pass_good".to_string(),
    };

    // 1. Initial seed — creates user, links item, accounts, and syncs transactions
    let res = seed_tester_profile(&pool, &plaid, &tester, false)
        .await
        .expect("initial seed must succeed");

    assert!(res.created_user, "should create new user");
    assert!(!res.skipped, "should not skip initial seed");
    assert_eq!(res.accounts_saved, 1, "should save mock account");
    assert_eq!(res.transactions_synced, 2, "should sync mock transactions");

    // 2. Second seed without reset — should skip
    let res_skip = seed_tester_profile(&pool, &plaid, &tester, false)
        .await
        .expect("second seed should succeed without error");

    assert!(
        res_skip.skipped,
        "second seed should skip already linked user"
    );

    // 3. Third seed with reset — should delete and re-seed
    let res_reset = seed_tester_profile(&pool, &plaid, &tester, true)
        .await
        .expect("reset seed should succeed");

    assert!(res_reset.created_user, "reset seed should recreate user");
    assert!(!res_reset.skipped, "reset seed should not skip");
    assert_eq!(res_reset.accounts_saved, 1);

    // Clean up
    cleanup(&pool, &email).await;
}

/// Verify that synthetic dashboard transactions are generated
/// successfully without duplicating data on repeated runs.

#[tokio::test]
async fn test_demo_transactions_are_repeatable() {
    // Run after or before the lifecycle test, never
    // simultaneously, to avoid shared Plaid mock collisions.
    let _guard = SEEDER_TEST_LOCK.lock().await;

    let pool = test_pool().await;

    // Use our existing mocked Plaid server.
    // No real financial institution is contacted.
    let plaid = PlaidClient::with_base_url(mock_plaid_url());

    // Generate a unique test account to prevent conflicts
    // with other automated tests.
    let email = unique_email("demo_transactions");

    let tester = TesterFixture {
        email: email.clone(),
        first_name: "Demo".to_string(),
        last_name: "Tester".to_string(),
        password: "Password123".to_string(),
        institution_id: "ins_109508".to_string(),
        sandbox_username: "custom_test".to_string(),
        sandbox_password: "pass_good".to_string(),
    };

    // Create the test user and their sandbox bank account.
    let seeded = seed_tester_profile(&pool, &plaid, &tester, false)
        .await
        .expect("Sandbox tester setup should succeed");

    // First run: synthetic transactions should be inserted.
    let first_count = seed_demo_transactions(&pool, seeded.user_id)
        .await
        .expect("First demo seed should succeed");

    assert!(
        first_count > 0,
        "Expected synthetic transactions to be inserted"
    );

    // Second run: all synthetic transaction IDs already exist,
    // so no new rows should be inserted.
    let second_count = seed_demo_transactions(&pool, seeded.user_id)
        .await
        .expect("Second demo seed should succeed");

    assert_eq!(second_count, 0);

    // Verify all eight supported spending categories
    // are represented in the generated transactions.
    let category_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(DISTINCT t.category)
        FROM plaid_transactions t
        JOIN plaid_accounts a ON a.account_id = t.account_id
        JOIN plaid_links l ON l.item_id = a.item_id
        WHERE l.user_id = $1
          AND t.transaction_id LIKE 'smart-finance-demo-v1-%'
        "#,
    )
    .bind(seeded.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(category_count, 8);

    // Remove this temporary user and all related test data.
    cleanup(&pool, &email).await;
}

/// Verify that synthetic net-worth history:
/// - Requires an existing current-day snapshot.
/// - Creates 120 historical monthly records.
/// - Marks those records as demonstration data.
/// - Never duplicates historical records.
/// - Never changes the current-day snapshot.
#[tokio::test]
async fn test_demo_net_worth_history_is_repeatable() {
    // This lock already exists in seeder.rs.
    //
    // It prevents our seeder tests from interfering
    // with each other's mocked Plaid connections.
    let _guard = SEEDER_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let plaid = PlaidClient::with_base_url(mock_plaid_url());

    let email = unique_email("demo_net_worth");

    let tester = TesterFixture {
        email: email.clone(),
        first_name: "History".to_string(),
        last_name: "Tester".to_string(),
        password: "Password123".to_string(),
        institution_id: "ins_109508".to_string(),
        sandbox_username: "custom_test".to_string(),
        sandbox_password: "pass_good".to_string(),
    };

    // Create one isolated test user.
    let seeded = seed_tester_profile(&pool, &plaid, &tester, false)
        .await
        .expect("Test user setup should succeed");

    // No current-day snapshot exists yet.
    //
    // The generator must refuse to invent a starting
    // value for the user's financial history.
    let without_anchor = seed_demo_net_worth(&pool, seeded.user_id).await.unwrap();

    assert_eq!(without_anchor, 0);

    // Create a real current-day snapshot for the test.
    //
    // This is a controlled test fixture, not real user data.
    sqlx::query(
        r#"
        INSERT INTO net_worth_snapshots (
            user_id,
            snapshot_date,
            amount,
            included_accounts,
            excluded_accounts,
            is_demo
        )
        VALUES ($1, CURRENT_DATE, 20000.0, 1, 0, FALSE)
        "#,
    )
    .bind(seeded.user_id)
    .execute(&pool)
    .await
    .unwrap();

    // First generation should create exactly 120
    // historical monthly snapshots.
    let first_count = seed_demo_net_worth(&pool, seeded.user_id).await.unwrap();

    assert_eq!(first_count, 120);

    // Repeating generation should insert nothing.
    let second_count = seed_demo_net_worth(&pool, seeded.user_id).await.unwrap();

    assert_eq!(second_count, 0);

    // Confirm the historical records are marked as demo.
    let demo_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM net_worth_snapshots
        WHERE user_id = $1
          AND is_demo = TRUE
        "#,
    )
    .bind(seeded.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(demo_count, 120);

    // Today's real snapshot must still equal $20,000
    // and must not have been marked as synthetic.
    let (today_amount, today_is_demo): (f64, bool) = sqlx::query_as(
        r#"
            SELECT amount, is_demo
            FROM net_worth_snapshots
            WHERE user_id = $1
              AND snapshot_date = CURRENT_DATE
            "#,
    )
    .bind(seeded.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(today_amount, 20000.0);
    assert!(!today_is_demo);

    // Clean up all records, including the generated ones.
    cleanup(&pool, &email).await;
}
