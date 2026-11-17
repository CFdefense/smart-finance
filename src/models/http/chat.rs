//! HTTP request and response models for chat routes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::AppError,
    models::sql::chat::{ChatMessage, ChatSession},
};

/// Maximum allowed length of a chat message in characters.
pub const MAX_MESSAGE_LENGTH: usize = 2000;

/// Request payload for sending a chat message to a session.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
pub struct SendMessageRequest {
    /// The message text sent by the user (max 2,000 characters).
    #[schema(example = "How much did I spend on groceries this month?")]
    pub message: String,
}

impl SendMessageRequest {
    /// Trims the message and validates its length.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::BadRequest`] if the message is empty after trimming or exceeds 2,000 characters.
    pub fn trimmed(&self) -> Result<&str, AppError> {
        let trimmed = self.message.trim();
        let char_count = trimmed.chars().count();
        if char_count == 0 {
            return Err(AppError::BadRequest(
                "message must not be empty".to_string(),
            ));
        }
        if char_count > MAX_MESSAGE_LENGTH {
            return Err(AppError::BadRequest(format!(
                "message must be at most {MAX_MESSAGE_LENGTH} characters"
            )));
        }
        Ok(trimmed)
    }
}

/// A stored chat message in a session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct Message {
    /// Primary key.
    pub id: i32,
    /// Message role (`user` or `assistant`).
    pub role: String,
    /// Text content of the message.
    pub content: String,
    /// Timestamp when the message was recorded.
    pub created_at: DateTime<Utc>,
}

impl Message {
    /// Builds a response from a stored message row.
    #[must_use]
    pub fn new(row: &ChatMessage) -> Self {
        Self {
            id: row.id,
            role: row.role.clone(),
            content: row.content.clone(),
            created_at: row.created_at,
        }
    }
}

/// A chat session without message history (used in listings and creation responses).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct Session {
    /// Primary key.
    pub id: i32,
    /// Session title, derived from the first user message.
    pub title: Option<String>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Session {
    /// Builds a response from a session row.
    #[must_use]
    pub fn new(row: &ChatSession) -> Self {
        Self {
            id: row.id,
            title: row.title.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Response payload containing a list of chat sessions.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct SessionsResponse {
    /// List of chat sessions, ordered newest updated first.
    pub sessions: Vec<Session>,
}

/// A chat session with its full message history in chronological order.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct SessionWithMessages {
    /// Primary key.
    pub id: i32,
    /// Session title, derived from the first user message.
    pub title: Option<String>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Messages in this session, oldest first.
    pub messages: Vec<Message>,
}

impl SessionWithMessages {
    /// Builds a response from a session row and its message rows (oldest first).
    #[must_use]
    pub fn new(session: &ChatSession, messages: &[ChatMessage]) -> Self {
        Self {
            id: session.id,
            title: session.title.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            messages: messages.iter().map(Message::new).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trimmed_valid_message() {
        let req = SendMessageRequest {
            message: "  Hello, world!  ".to_string(),
        };
        assert_eq!(req.trimmed().unwrap(), "Hello, world!");
    }

    #[test]
    fn test_trimmed_empty_message_rejected() {
        let req = SendMessageRequest {
            message: "   \n\t  ".to_string(),
        };
        assert!(matches!(req.trimmed(), Err(AppError::BadRequest(_))));
    }

    #[test]
    fn test_trimmed_exact_limit_accepted() {
        let text = "a".repeat(2000);
        let req = SendMessageRequest {
            message: format!("  {text}  "),
        };
        assert_eq!(req.trimmed().unwrap().len(), 2000);
    }

    #[test]
    fn test_trimmed_over_limit_rejected() {
        let text = "a".repeat(2001);
        let req = SendMessageRequest { message: text };
        assert!(matches!(req.trimmed(), Err(AppError::BadRequest(_))));
    }

    #[test]
    fn test_char_counting_multibyte_unicode() {
        // Multi-byte Unicode: "🦀" is 4 bytes, but 1 char.
        // 2000 crabs = 2000 chars (8000 bytes) -> must be accepted.
        let text: String = "🦀".repeat(2000);
        let req = SendMessageRequest { message: text };
        assert_eq!(req.trimmed().unwrap().chars().count(), 2000);

        // 2001 crabs = 2001 chars -> must be rejected.
        let text_over: String = "🦀".repeat(2001);
        let req_over = SendMessageRequest { message: text_over };
        assert!(matches!(req_over.trimmed(), Err(AppError::BadRequest(_))));
    }

    #[test]
    fn test_dto_conversions() {
        let now = Utc::now();
        let sql_session = ChatSession {
            id: 1,
            user_id: 42,
            title: Some("My session".to_string()),
            created_at: now,
            updated_at: now,
        };
        let sql_message = ChatMessage {
            id: 10,
            session_id: 1,
            role: "user".to_string(),
            content: "Hello".to_string(),
            created_at: now,
        };

        let session = Session::new(&sql_session);
        assert_eq!(session.id, 1);
        assert_eq!(session.title.as_deref(), Some("My session"));

        let message = Message::new(&sql_message);
        assert_eq!(message.id, 10);
        assert_eq!(message.role, "user");
        assert_eq!(message.content, "Hello");

        let session_with_messages = SessionWithMessages::new(&sql_session, &[sql_message]);
        assert_eq!(session_with_messages.messages.len(), 1);
        assert_eq!(session_with_messages.messages[0].id, 10);
    }
}
