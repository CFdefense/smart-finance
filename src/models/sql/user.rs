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
}
