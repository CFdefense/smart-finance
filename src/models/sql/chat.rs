//! SQL row models for chat sessions and messages.

use chrono::{DateTime, Utc};

/// Row model for the `chat_sessions` table.
pub struct ChatSession {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// First 60 characters of the first message; `None` until a turn is saved.
    pub title: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time (bumped on each saved turn).
    pub updated_at: DateTime<Utc>,
}

/// Row model for the `chat_messages` table.
pub struct ChatMessage {
    /// Primary key.
    pub id: i32,
    /// FK to `chat_sessions.id`.
    pub session_id: i32,
    /// Author role (`user` or `assistant`).
    pub role: String,
    /// Message text.
    pub content: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}
