//! SQL row models and database query helpers for chat sessions and messages.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Row model for the `chat_sessions` table.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ChatSession {
    /// Primary key.
    pub id: i32,
    /// Foreign key to `users.id`.
    pub user_id: i32,
    /// Session title (first 60 characters of the first message, or `None` if empty).
    pub title: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

/// Row model for the `chat_messages` table.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ChatMessage {
    /// Primary key.
    pub id: i32,
    /// Foreign key to `chat_sessions.id`.
    pub session_id: i32,
    /// Author role (`user` or `assistant`).
    pub role: String,
    /// Message content.
    pub content: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// Creates a new empty chat session for the specified user.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the database insert fails.
pub async fn create_session(pool: &PgPool, user_id: i32) -> Result<ChatSession, sqlx::Error> {
    sqlx::query_as!(
        ChatSession,
        r#"
        INSERT INTO chat_sessions (user_id)
        VALUES ($1)
        RETURNING id, user_id, title, created_at, updated_at
        "#,
        user_id
    )
    .fetch_one(pool)
    .await
}

/// Lists all chat sessions for a user, ordered with the newest `updated_at` first.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the database query fails.
pub async fn list_sessions(pool: &PgPool, user_id: i32) -> Result<Vec<ChatSession>, sqlx::Error> {
    sqlx::query_as!(
        ChatSession,
        r#"
        SELECT id, user_id, title, created_at, updated_at
        FROM chat_sessions
        WHERE user_id = $1
        ORDER BY updated_at DESC, id DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/// Retrieves a specific chat session belonging to the given user.
///
/// Returns `None` if no matching session is found for that user.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the database query fails.
pub async fn get_session(
    pool: &PgPool,
    user_id: i32,
    id: i32,
) -> Result<Option<ChatSession>, sqlx::Error> {
    sqlx::query_as!(
        ChatSession,
        r#"
        SELECT id, user_id, title, created_at, updated_at
        FROM chat_sessions
        WHERE user_id = $1 AND id = $2
        "#,
        user_id,
        id
    )
    .fetch_optional(pool)
    .await
}

/// Lists all messages in a chat session in chronological order (oldest first).
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the database query fails.
pub async fn list_messages(
    pool: &PgPool,
    session_id: i32,
) -> Result<Vec<ChatMessage>, sqlx::Error> {
    sqlx::query_as!(
        ChatMessage,
        r#"
        SELECT id, session_id, role, content, created_at
        FROM chat_messages
        WHERE session_id = $1
        ORDER BY created_at ASC, id ASC
        "#,
        session_id
    )
    .fetch_all(pool)
    .await
}

/// Retrieves the most recent `limit` messages in a chat session, returned in chronological order (oldest first).
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the database query fails.
pub async fn recent_messages(
    pool: &PgPool,
    session_id: i32,
    limit: i64,
) -> Result<Vec<ChatMessage>, sqlx::Error> {
    sqlx::query_as!(
        ChatMessage,
        r#"
        SELECT id, session_id, role, content, created_at
        FROM (
            SELECT id, session_id, role, content, created_at
            FROM chat_messages
            WHERE session_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2
        ) sub
        ORDER BY created_at ASC, id ASC
        "#,
        session_id,
        limit
    )
    .fetch_all(pool)
    .await
}

/// Saves a full conversation turn (user prompt and assistant reply) in a single transaction.
///
/// If the session title is currently `NULL`, it will be initialized to the first 60 characters
/// of the user's message. The session's `updated_at` timestamp is updated to `NOW()`.
///
/// Returns the newly inserted assistant [`ChatMessage`].
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if any query or transaction operation fails.
pub async fn save_turn(
    pool: &PgPool,
    session_id: i32,
    user_text: &str,
    assistant_text: &str,
) -> Result<ChatMessage, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"
        INSERT INTO chat_messages (session_id, role, content)
        VALUES ($1, 'user', $2)
        "#,
        session_id,
        user_text
    )
    .execute(&mut *tx)
    .await?;

    let assistant_msg = sqlx::query_as!(
        ChatMessage,
        r#"
        INSERT INTO chat_messages (session_id, role, content)
        VALUES ($1, 'assistant', $2)
        RETURNING id, session_id, role, content, created_at
        "#,
        session_id,
        assistant_text
    )
    .fetch_one(&mut *tx)
    .await?;

    // Title is the first 60 characters (not bytes) of the first message.
    let title_prefix: String = user_text.chars().take(60).collect();

    sqlx::query!(
        r#"
        UPDATE chat_sessions
        SET title = COALESCE(title, $2),
            updated_at = NOW()
        WHERE id = $1
        "#,
        session_id,
        title_prefix
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(assistant_msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixtures::{cleanup, test_pool, unique_email};

    async fn create_test_user(pool: &PgPool, email: &str) -> i32 {
        sqlx::query_scalar!(
            r#"
            INSERT INTO users (first_name, last_name, email, password_hash)
            VALUES ('Chat', 'Tester', $1, 'hashed_pw')
            RETURNING id
            "#,
            email
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_session_and_message_queries() {
        let pool = test_pool().await;
        let email = unique_email("chatsql");
        let user_id = create_test_user(&pool, &email).await;

        let other_email = unique_email("chatsql_other");
        let other_user_id = create_test_user(&pool, &other_email).await;

        // 1. Create session
        let session = create_session(&pool, user_id).await.unwrap();
        assert_eq!(session.user_id, user_id);
        assert!(session.title.is_none());

        // 2. Get session (owner vs other user)
        let fetched = get_session(&pool, user_id, session.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, session.id);

        let foreign = get_session(&pool, other_user_id, session.id).await.unwrap();
        assert!(foreign.is_none());

        // 3. Save turn 1 (first turn sets title to first 60 chars)
        let long_prompt = "What is my total spending across all accounts in the last thirty days? Please check groceries too.";
        let assistant_reply = "Your total spending was $1,234.56.";
        let asst_msg = save_turn(&pool, session.id, long_prompt, assistant_reply)
            .await
            .unwrap();
        assert_eq!(asst_msg.session_id, session.id);
        assert_eq!(asst_msg.role, "assistant");
        assert_eq!(asst_msg.content, assistant_reply);

        let updated_session = get_session(&pool, user_id, session.id)
            .await
            .unwrap()
            .unwrap();
        let expected_title: String = long_prompt.chars().take(60).collect();
        assert_eq!(
            updated_session.title.as_deref(),
            Some(expected_title.as_str())
        );

        // 4. Save turn 2 (does not overwrite existing title)
        let prompt_2 = "Can you break it down by category?";
        let reply_2 = "Sure! Groceries: $400, Utilities: $200.";
        let asst_msg_2 = save_turn(&pool, session.id, prompt_2, reply_2)
            .await
            .unwrap();
        assert_eq!(asst_msg_2.content, reply_2);

        let session_after_turn_2 = get_session(&pool, user_id, session.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            session_after_turn_2.title.as_deref(),
            Some(expected_title.as_str())
        );
        assert!(session_after_turn_2.updated_at >= updated_session.updated_at);

        // 5. List sessions
        let sessions = list_sessions(&pool, user_id).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session.id);

        // 6. List messages (oldest first)
        let messages = list_messages(&pool, session.id).await.unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, long_prompt);
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, assistant_reply);
        assert_eq!(messages[2].role, "user");
        assert_eq!(messages[2].content, prompt_2);
        assert_eq!(messages[3].role, "assistant");
        assert_eq!(messages[3].content, reply_2);

        // 7. Recent messages with limit (e.g. limit 2 -> last 2 messages, oldest first)
        let recent = recent_messages(&pool, session.id, 2).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].role, "user");
        assert_eq!(recent[0].content, prompt_2);
        assert_eq!(recent[1].role, "assistant");
        assert_eq!(recent[1].content, reply_2);

        // Cleanup
        cleanup(&pool, &email).await;
        cleanup(&pool, &other_email).await;
    }
}
