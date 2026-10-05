//! HTTP request and response models for user authentication routes.

use regex::Regex;
use serde::Deserialize;
use utoipa::ToSchema;

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
        let email_regex =
            Regex::new(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$").unwrap();
        email_regex.is_match(email)
    }

    /// Validates password strength.
    ///
    /// Rules:
    /// - 8–128 ASCII characters
    /// - At least one uppercase letter
    /// - At least one lowercase letter
    /// - At least one digit
    pub fn validate_password(password: &str) -> Result<(), String> {
        if password.len() < 8 {
            return Err("Password must be at least 8 characters long".to_string());
        }
        if password.len() > 128 {
            return Err("Password must be 128 characters or less".to_string());
        }
        if !password.is_ascii() {
            return Err("Password must contain only ASCII characters".to_string());
        }
        if !password.chars().any(|c| c.is_uppercase()) {
            return Err("Password must contain at least one uppercase letter".to_string());
        }
        if !password.chars().any(|c| c.is_lowercase()) {
            return Err("Password must contain at least one lowercase letter".to_string());
        }
        if !password.chars().any(|c| c.is_numeric()) {
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
