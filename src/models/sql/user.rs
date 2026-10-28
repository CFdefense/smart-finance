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
    /// Account email address.
    pub email: String,
    /// Whether the user has opted in to email notifications.
    pub email_notifications: bool,
    /// E.164 phone number, or `None` if not set.
    pub phone_number: Option<String>,
    /// Whether the user has opted in to SMS notifications.
    pub sms_notifications: bool,
    /// Carrier email-to-SMS gateway domain (e.g. `tmomail.net`), or `None` if not set.
    pub sms_carrier_gateway: Option<String>,
}
