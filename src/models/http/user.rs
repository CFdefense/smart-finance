//! HTTP request and response models for user authentication routes.

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use utoipa::ToSchema;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$").unwrap()
});

/// Request payload for `POST /api/user/login`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Account email address.
    pub email: String,
    /// Plaintext password submitted by the user.
    pub password: String,
}

/// Request payload for `POST /api/user/signup`.
///
/// Validated server-side via [`SignupRequest::validate`] before insert.
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct SignupRequest {
    /// Account email address.
    pub email: String,
    /// Plaintext password submitted by the user.
    pub password: String,
}

impl SignupRequest {
    /// Validates email format using a standard RFC 5322-compatible regex.
    pub fn validate_email(email: &str) -> bool {
        EMAIL_REGEX.is_match(email)
    }

    /// Validates password strength.
    ///
    /// Rules:
    /// - 8–128 ASCII characters
    /// - At least one uppercase letter
    /// - At least one lowercase letter
    /// - At least one digit
    pub fn validate_password(password: &str) -> Result<(), String> {
        if !password.is_ascii() {
            return Err("Password must contain only ASCII characters".to_string());
        }
        if password.len() < 8 {
            return Err("Password must be at least 8 characters long".to_string());
        }
        if password.len() > 128 {
            return Err("Password must be 128 characters or less".to_string());
        }
        if !password.chars().any(char::is_uppercase) {
            return Err("Password must contain at least one uppercase letter".to_string());
        }
        if !password.chars().any(char::is_lowercase) {
            return Err("Password must contain at least one lowercase letter".to_string());
        }
        if !password.chars().any(|c| c.is_ascii_digit()) {
            return Err("Password must contain at least one number".to_string());
        }
        Ok(())
    }

    /// Validates the entire signup payload.
    ///
    /// Returns `Ok(())` if all fields pass, or `Err(String)` with the first failure message.
    pub fn validate(&self) -> Result<(), String> {
        let email = self.email.trim();
        if email.is_empty() {
            return Err("Email is required".to_string());
        }
        if !Self::validate_email(email) {
            return Err("Invalid email format".to_string());
        }
        Self::validate_password(&self.password)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(email: &str, password: &str) -> SignupRequest {
        SignupRequest {
            email: email.to_string(),
            password: password.to_string(),
        }
    }

    // --- validate_email ---

    #[test]
    fn valid_email() {
        assert!(SignupRequest::validate_email("alice@example.com"));
    }

    #[test]
    fn email_no_at_symbol() {
        assert!(!SignupRequest::validate_email("notanemail"));
    }

    #[test]
    fn email_no_domain() {
        assert!(!SignupRequest::validate_email("alice@"));
    }

    #[test]
    fn email_no_tld() {
        assert!(!SignupRequest::validate_email("alice@example"));
    }

    #[test]
    fn email_empty() {
        assert!(!SignupRequest::validate_email(""));
    }

    // --- validate_password ---

    #[test]
    fn valid_password() {
        assert!(SignupRequest::validate_password("Secret_123").is_ok());
    }

    #[test]
    fn password_too_short() {
        assert!(SignupRequest::validate_password("Ab1").is_err());
    }

    #[test]
    fn password_no_uppercase() {
        assert!(SignupRequest::validate_password("secret_123").is_err());
    }

    #[test]
    fn password_no_lowercase() {
        assert!(SignupRequest::validate_password("SECRET_123").is_err());
    }

    #[test]
    fn password_no_digit() {
        assert!(SignupRequest::validate_password("Secret_abc").is_err());
    }

    #[test]
    fn password_non_ascii() {
        assert!(SignupRequest::validate_password("Sécret_123").is_err());
    }

    #[test]
    fn password_too_long() {
        let long = "A1a".repeat(50); // 150 chars
        assert!(SignupRequest::validate_password(&long).is_err());
    }

    // --- validate (combined) ---

    #[test]
    fn validate_success() {
        assert!(req("alice@example.com", "Secret_123").validate().is_ok());
    }

    #[test]
    fn validate_empty_email() {
        assert!(req("", "Secret_123").validate().is_err());
    }

    #[test]
    fn validate_invalid_email() {
        assert!(req("notanemail", "Secret_123").validate().is_err());
    }

    #[test]
    fn validate_bad_password() {
        assert!(req("alice@example.com", "weak").validate().is_err());
    }
}
