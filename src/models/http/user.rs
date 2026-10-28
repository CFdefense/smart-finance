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
    /// User's first name.
    pub first_name: String,
    /// User's last name.
    pub last_name: String,
    /// Account email address.
    pub email: String,
    /// Whether the user has opted in to email notifications.
    pub email_notifications: bool,
    /// E.164 phone number, or `null` if not set.
    pub phone_number: Option<String>,
    /// Whether the user has opted in to SMS notifications.
    pub sms_notifications: bool,
    /// Carrier email-to-SMS gateway domain (e.g. `tmomail.net`), or `null` if not set.
    pub sms_carrier_gateway: Option<String>,
}

/// Supported SMS carrier gateways.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, ToSchema)]
pub enum SmsCarrierGateway {
    /// T-Mobile's email-to-SMS gateway.
    #[serde(rename = "tmomail.net")]
    TMobile,

    /// Verizon's email-to-SMS gateway.
    #[serde(rename = "vtext.com")]
    Verizon,

    /// Represents clearing the selected carrier gateway.
    #[serde(rename = "")]
    None,
}

impl SmsCarrierGateway {
    /// Returns the gateway domain used by the notification service.
    /// None represents a request to clear the carrier selection.
    pub fn as_domain(self) -> Option<&'static str> {
        match self {
            Self::TMobile => Some("tmomail.net"),
            Self::Verizon => Some("vtext.com"),
            Self::None => None,
        }
    }
}

/// Request body for `PUT /api/user/profile`.
///
/// All fields are optional — omitted fields are left unchanged.
/// Pass `phone_number: ""` to clear the stored phone number (also forces
/// `sms_notifications` to `false`).
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct UpdateProfileRequest {
    /// New first name, if provided.
    pub first_name: Option<String>,
    /// New last name, if provided.
    pub last_name: Option<String>,
    /// New account email, if provided.
    pub email: Option<String>,
    /// Opt in or out of email notifications.
    pub email_notifications: Option<bool>,
    /// New E.164 phone number, `""` to clear, or absent to leave unchanged.
    pub phone_number: Option<String>,
    /// Opt in or out of SMS notifications.
    pub sms_notifications: Option<bool>,
    /// Carrier email-to-SMS gateway domain (e.g. `tmomail.net`), `""` to clear, or absent to leave unchanged.
    pub sms_carrier_gateway: Option<SmsCarrierGateway>,
}

impl UpdateProfileRequest {
    /// Validates the request against the user's currently-stored phone number.
    ///
    /// Rules:
    /// - If `phone_number` is `Some(s)` and non-empty, it must be E.164 format.
    /// - If the effective `sms_notifications` after the update would be `true`
    ///   and there would be no phone number, returns an error.
    /// - If the effective `sms_notifications` would be `true` and there would be
    ///   no carrier gateway, returns an error.
    ///
    /// `existing_phone`, `existing_gateway`, and `existing_sms` are the values
    /// currently stored for the user.
    pub fn validate(
        &self,
        existing_phone: Option<&str>,
        existing_gateway: Option<&str>,
        existing_sms: bool,
    ) -> Result<(), String> {
        // Validate first name
        if let Some(first_name) = &self.first_name {
            if first_name.trim().is_empty() {
                return Err("First name cannot be empty".to_string());
            }
        }

        // Validate last name
        if let Some(last_name) = &self.last_name {
            if last_name.trim().is_empty() {
                return Err("Last name cannot be empty".to_string());
            }
        }

        // Validate email
        if let Some(email) = &self.email {
            if !SignupRequest::validate_email(email.trim()) {
                return Err("Invalid email address".to_string());
            }
        }
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

        // Use the requested carrier, or retain the existing one.
        // SmsCarrierGateway::None explicitly clears the carrier.
        let effective_gateway: Option<&str> = match self.sms_carrier_gateway {
            Some(gateway) => gateway.as_domain(),
            None => existing_gateway,
        };

        let effective_sms = self.sms_notifications.unwrap_or(existing_sms);
        if effective_sms && effective_phone.map_or(true, str::is_empty) {
            return Err("sms_notifications requires a phone_number".to_string());
        }
        if effective_sms && effective_gateway.map_or(true, str::is_empty) {
            return Err("sms_notifications requires an sms_carrier_gateway".to_string());
        }

        Ok(())
    }
}

/// Request payload for changing the authenticated user's password.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// The user's current password, required for verification.
    pub current_password: String,

    /// The new password to save after validation.
    pub new_password: String,
}

impl ChangePasswordRequest {
    /// Validate the new password using the signup password rules.
    pub fn validate(&self) -> Result<(), String> {
        SignupRequest::validate_password(&self.new_password)
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

    fn upd(
        email: Option<bool>,
        phone: Option<&str>,
        sms: Option<bool>,
        gateway: Option<&str>,
    ) -> UpdateProfileRequest {
        UpdateProfileRequest {
            first_name: None,
            last_name: None,
            email: None,
            email_notifications: email,
            phone_number: phone.map(str::to_string),
            sms_notifications: sms,
            sms_carrier_gateway: gateway.map(|value| match value {
                "tmomail.net" => SmsCarrierGateway::TMobile,
                "vtext.com" => SmsCarrierGateway::Verizon,
                "" => SmsCarrierGateway::None,
                _ => panic!("Unsupported gateway in test fixture"),
            }),
        }
    }

    #[test]
    fn profile_update_all_none_is_ok() {
        assert!(upd(None, None, None, None)
            .validate(None, None, false)
            .is_ok());
    }

    #[test]
    fn profile_update_valid_phone_accepted() {
        assert!(upd(None, Some("+15551234567"), None, None)
            .validate(None, None, false)
            .is_ok());
    }

    #[test]
    fn profile_update_bad_phone_rejected() {
        assert!(upd(None, Some("5551234567"), None, None)
            .validate(None, None, false)
            .is_err());
    }

    #[test]
    fn profile_update_sms_true_no_phone_rejected() {
        assert!(upd(None, None, Some(true), None)
            .validate(None, None, false)
            .is_err());
    }

    #[test]
    fn profile_update_sms_true_with_new_phone_ok() {
        assert!(
            upd(None, Some("+15551234567"), Some(true), Some("tmomail.net"))
                .validate(None, None, false)
                .is_ok()
        );
    }

    #[test]
    fn profile_update_sms_true_with_existing_phone_ok() {
        assert!(upd(None, None, Some(true), None)
            .validate(Some("+15551234567"), Some("tmomail.net"), false)
            .is_ok());
    }

    #[test]
    fn profile_update_clear_phone_with_sms_true_rejected() {
        assert!(upd(None, Some(""), Some(true), Some("tmomail.net"))
            .validate(Some("+15551234567"), None, false)
            .is_err());
    }

    #[test]
    fn profile_update_clear_phone_ok() {
        assert!(upd(None, Some(""), None, None)
            .validate(Some("+15551234567"), None, false)
            .is_ok());
    }

    #[test]
    fn profile_update_sms_true_no_gateway_rejected() {
        assert!(upd(None, Some("+15551234567"), Some(true), None)
            .validate(None, None, false)
            .is_err());
    }

    #[test]
    fn profile_update_sms_true_with_existing_gateway_ok() {
        assert!(upd(None, None, Some(true), None)
            .validate(Some("+15551234567"), Some("vtext.com"), false)
            .is_ok());
    }

    #[test]
    fn profile_update_clear_gateway_with_sms_true_rejected() {
        assert!(upd(None, None, Some(true), Some(""))
            .validate(Some("+15551234567"), Some("tmomail.net"), false)
            .is_err());
    }

    // Regression: omitting sms_notifications must fall back to the existing DB
    // value, not silently default to false.
    #[test]
    fn profile_update_clear_gateway_sms_omitted_uses_existing_sms() {
        // User has SMS enabled in DB; request clears the gateway but omits sms_notifications.
        // effective_sms should be true (from existing_sms), so this must be rejected.
        assert!(upd(None, None, None, Some(""))
            .validate(Some("+15551234567"), Some("tmomail.net"), true)
            .is_err());
    }

    /// Verifies that only supported SMS carrier gateways can be deserialized.
    #[test]
    fn sms_carrier_gateway_deserialization() {
        // Supported carriers should deserialize successfully.
        assert!(serde_json::from_str::<SmsCarrierGateway>("\"tmomail.net\"").is_ok());

        assert!(serde_json::from_str::<SmsCarrierGateway>("\"vtext.com\"").is_ok());

        // An empty string represents clearing the carrier.
        assert!(serde_json::from_str::<SmsCarrierGateway>("\"\"").is_ok());

        // Unsupported carriers should be rejected.
        assert!(serde_json::from_str::<SmsCarrierGateway>("\"unsupported.com\"").is_err());
    }
}
