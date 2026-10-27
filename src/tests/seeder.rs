//! Tests for sandbox fixtures and seeder configuration.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use super::fixtures::{cleanup, mock_plaid_url, test_pool, unique_email};
use crate::{
    models::http::user::SignupRequest,
    plaid::client::PlaidClient,
    seeder::{hash_password, seed_tester_profile, TesterFixture},
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

    assert_eq!(fixtures.len(), 4, "expected 4 tester fixtures");

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
