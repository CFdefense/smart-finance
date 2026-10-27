//! HTTP request and response models for user routes.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

static EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$").unwrap());

static PHONE_E164_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\+[1-9]\d{7,14}$").unwrap());

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
    /// User first name.
    pub first_name: String,
    /// User last name.
    pub last_name: String,
    /// Account email address.
    pub email: String,
    /// Plaintext password submitted by the user.
    pub password: String,
}

/// Response body for `GET /api/user/profile`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileResponse {
    /// Account email address.
    pub email: String,
    /// Whether the user has opted in to email notifications.
    pub email_notifications: bool,
    /// E.164 phone number, or `null` if not set.
    pub phone_number: Option<String>,
    /// Whether the user has opted in to SMS notifications.
    pub sms_notifications: bool,
}

/// Request body for `PUT /api/user/profile`.
///
/// All fields are optional — omitted fields are left unchanged.
/// Pass `phone_number: ""` to clear the stored phone number (also forces
/// `sms_notifications` to `false`).
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct UpdateProfileRequest {
    /// Opt in or out of email notifications.
    pub email_notifications: Option<bool>,
    /// New E.164 phone number, `""` to clear, or absent to leave unchanged.
    pub phone_number: Option<String>,
    /// Opt in or out of SMS notifications.
    pub sms_notifications: Option<bool>,
}

impl UpdateProfileRequest {
    /// Validates the request against the user's currently-stored phone number.
    ///
    /// Rules:
    /// - If `phone_number` is `Some(s)` and non-empty, it must be E.164 format.
    /// - If the effective `sms_notifications` after the update would be `true`
    ///   and there would be no phone number, returns an error.
    ///
    /// `existing_phone` is the phone number currently stored for the user,
    /// used to determine the effective state when `phone_number` is omitted.
    pub fn validate(&self, existing_phone: Option<&str>) -> Result<(), String> {
        // Validate format if a new non-empty phone is provided
        if let Some(phone) = &self.phone_number {
            if !phone.is_empty() && !PHONE_E164_REGEX.is_match(phone) {
                return Err("phone_number must be in E.164 format (e.g. +15551234567)".to_string());
            }
        }

        // Effective phone after this update:
        //   Some("") → cleared (None), Some(s) → s, None → existing unchanged
        let effective_phone: Option<&str> = match &self.phone_number {
            Some(p) if p.is_empty() => None,
            Some(p) => Some(p.as_str()),
            None => existing_phone,
        };

        let effective_sms = self.sms_notifications.unwrap_or(false);
        if effective_sms && effective_phone.map_or(true, str::is_empty) {
            return Err("sms_notifications requires a phone_number".to_string());
        }

        Ok(())
    }
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
        if self.first_name.trim().is_empty() {
            return Err("First name is required".to_string());
        }

        if self.last_name.trim().is_empty() {
            return Err("Last name is required".to_string());
        }

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
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
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
    fn validate_empty_first_name() {
        let mut request = req("alice@example.com", "Secret_123");
        request.first_name = "".to_string();

        assert!(request.validate().is_err());
    }

    #[test]
    fn validate_empty_last_name() {
        let mut request = req("alice@example.com", "Secret_123");
        request.last_name = "".to_string();

        assert!(request.validate().is_err());
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

    // ── UpdateProfileRequest::validate ──────────────────────────────────

    fn upd(email: Option<bool>, phone: Option<&str>, sms: Option<bool>) -> UpdateProfileRequest {
        UpdateProfileRequest {
            email_notifications: email,
            phone_number: phone.map(str::to_string),
            sms_notifications: sms,
        }
    }

    #[test]
    fn profile_update_all_none_is_ok() {
        assert!(upd(None, None, None).validate(None).is_ok());
    }

    #[test]
    fn profile_update_valid_phone_accepted() {
        assert!(upd(None, Some("+15551234567"), None).validate(None).is_ok());
    }

    #[test]
    fn profile_update_bad_phone_rejected() {
        assert!(upd(None, Some("5551234567"), None).validate(None).is_err());
    }

    #[test]
    fn profile_update_sms_true_no_phone_rejected() {
        assert!(upd(None, None, Some(true)).validate(None).is_err());
    }

    #[test]
    fn profile_update_sms_true_with_new_phone_ok() {
        assert!(upd(None, Some("+15551234567"), Some(true))
            .validate(None)
            .is_ok());
    }

    #[test]
    fn profile_update_sms_true_with_existing_phone_ok() {
        assert!(upd(None, None, Some(true))
            .validate(Some("+15551234567"))
            .is_ok());
    }

    #[test]
    fn profile_update_clear_phone_with_sms_true_rejected() {
        assert!(upd(None, Some(""), Some(true))
            .validate(Some("+15551234567"))
            .is_err());
    }

    #[test]
    fn profile_update_clear_phone_ok() {
        assert!(upd(None, Some(""), None)
            .validate(Some("+15551234567"))
            .is_ok());
    }
}
