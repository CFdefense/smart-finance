//! Tests for sandbox fixtures and seeder configuration.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use serde::Deserialize;

use crate::models::http::user::SignupRequest;

#[derive(Debug, Deserialize)]
struct TesterFixture {
    email: String,
    first_name: String,
    last_name: String,
    password: String,
    institution_id: String,
    sandbox_username: String,
    sandbox_password: String,
}

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
    }
}
