//! SQL row models for the `users` table.

/// Row model for the `users` table.
///
/// Used internally by controllers for DB queries. Never exposed in API responses.
pub struct UserRow {
    /// Primary key.
    pub id: i32,
    /// Unique email address.
    pub email: String,
    /// Argon2 hashed password.
    pub password_hash: String,
    /// Account status: active, inactive, suspended.
    pub account_status: String,
}

/// Row model for reading notification preferences from the `users` table.
///
/// Returned by `GET /api/user/profile`.

pub struct UserProfileRow {
    /// User's first name.
    pub first_name: String,
    /// User's last name.
    pub last_name: String,
    /// Account email address.
    pub email: String,
    /// Whether email notifications are enabled.
    pub email_notifications: bool,
    /// Phone number, if provided.
    pub phone_number: Option<String>,
    /// Whether SMS notifications are enabled.
    pub sms_notifications: bool,
}
