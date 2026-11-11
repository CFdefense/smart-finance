//! Chat endpoints — AI assistant sessions, history and streamed replies.
//!
//! The Rust API owns chat data and authentication; the ai-service is stateless and trusted
//! with the verified `user_id` only. A reply is relayed to the browser as Server-Sent Events and
//! the whole turn (user message + reply) is saved only after the ai-service reports `done`.

use std::{
    collections::HashSet,
    convert::Infallible,
    sync::{Arc, Mutex, PoisonError},
};

use axum::{
    extract::Path,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Extension, Json,
};
use futures_util::stream;
use serde_json::json;
use sqlx::PgPool;
use tracing::{debug, info, warn};
use utoipa::OpenApi;

use crate::{
    chat::{AiClient, AiEvent, AiStream, HistoryItem, StartError},
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::{
        http::chat::{Message, SendMessageRequest, Session, SessionWithMessages, SessionsResponse},
        sql::chat::{ChatMessage, ChatSession},
    },
};

/// Number of stored messages sent to the ai-service as history.
const HISTORY_LIMIT: i64 = 20;

/// JSON error body returned (with HTTP 503) when the ai-service cannot be reached before the
/// stream opens.
const ASSISTANT_UNAVAILABLE_MESSAGE: &str =
    "The assistant is unavailable right now. Your other data still works.";

/// Shared in-process set of session IDs that currently have a reply streaming.
pub type InFlight = Arc<Mutex<HashSet<i32>>>;

/// Creates an empty [`InFlight`] set (one per application).
#[must_use]
pub fn new_in_flight() -> InFlight {
    Arc::new(Mutex::new(HashSet::new()))
}

/// Marks a session as having a reply in flight; the mark is removed on drop.
struct InFlightGuard {
    set: InFlight,
    session_id: i32,
}

impl InFlightGuard {
    /// Returns `None` when the session already has a reply in flight.
    fn acquire(set: &InFlight, session_id: i32) -> Option<Self> {
        let inserted = set
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id);
        inserted.then(|| Self {
            set: Arc::clone(set),
            session_id,
        })
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.session_id);
    }
}

/// Loads a session only if it belongs to the user; foreign or unknown IDs are `NotFound`.
async fn owned_session(pool: &PgPool, user_id: i32, id: i32) -> ApiResult<ChatSession> {
    sqlx::query_as!(
        ChatSession,
        r#"
        SELECT id, user_id, title, created_at, updated_at
        FROM chat_sessions
        WHERE id = $1 AND user_id = $2
        "#,
        id,
        user_id
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)
}

/// Saves a whole turn (user message and reply) in one transaction and returns the reply row.
///
/// The first saved turn also sets the session title (first 60 characters, not bytes); every
/// saved turn bumps `updated_at`.
async fn save_turn(
    pool: &PgPool,
    session_id: i32,
    user_text: &str,
    reply: &str,
) -> Result<ChatMessage, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query!(
        "INSERT INTO chat_messages (session_id, role, content) VALUES ($1, 'user', $2)",
        session_id,
        user_text
    )
    .execute(&mut *tx)
    .await?;

    let reply_row = sqlx::query_as!(
        ChatMessage,
        r#"
        INSERT INTO chat_messages (session_id, role, content)
        VALUES ($1, 'assistant', $2)
        RETURNING id, session_id, role, content, created_at
        "#,
        session_id,
        reply
    )
    .fetch_one(&mut *tx)
    .await?;

    let title: String = user_text.chars().take(60).collect();
    sqlx::query!(
        "UPDATE chat_sessions SET title = COALESCE(title, $2), updated_at = NOW() WHERE id = $1",
        session_id,
        title
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(reply_row)
}

/// Builds a named SSE event with a JSON payload.
fn sse_event(name: &str, data: &serde_json::Value) -> Event {
    Event::default().event(name).data(data.to_string())
}

/// List the user's chat sessions, newest first.
///
/// `GET /api/chat/sessions`
#[utoipa::path(
    get,
    path = "/sessions",
    summary = "List chat sessions",
    responses(
        (status = 200, description = "Sessions returned, newest first", body = SessionsResponse),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "Chat"
)]
pub async fn api_list_sessions(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
) -> ApiResult<Json<SessionsResponse>> {
    debug!("HANDLER ->> GET /api/chat/sessions user_id={}", user.id);

    let rows = sqlx::query_as!(
        ChatSession,
        r#"
        SELECT id, user_id, title, created_at, updated_at
        FROM chat_sessions
        WHERE user_id = $1
        ORDER BY updated_at DESC, id DESC
        "#,
        user.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    Ok(Json(SessionsResponse {
        sessions: rows.iter().map(Session::new).collect(),
    }))
}

/// Start a new, empty chat session.
///
/// `POST /api/chat/sessions`
#[utoipa::path(
    post,
    path = "/sessions",
    summary = "Create a chat session",
    responses(
        (status = 201, description = "Session created", body = Session),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "Chat"
)]
pub async fn api_create_session(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
) -> ApiResult<(StatusCode, Json<Session>)> {
    debug!("HANDLER ->> POST /api/chat/sessions user_id={}", user.id);

    let row = sqlx::query_as!(
        ChatSession,
        r#"
        INSERT INTO chat_sessions (user_id)
        VALUES ($1)
        RETURNING id, user_id, title, created_at, updated_at
        "#,
        user.id
    )
    .fetch_one(&pool)
    .await
    .map_err(AppError::from)?;

    Ok((StatusCode::CREATED, Json(Session::new(&row))))
}

/// Fetch one session with its messages, oldest first.
///
/// `GET /api/chat/sessions/{id}`
#[utoipa::path(
    get,
    path = "/sessions/{id}",
    summary = "Get a chat session with its messages",
    params(("id" = i32, Path, description = "Session ID")),
    responses(
        (status = 200, description = "Session returned", body = SessionWithMessages),
        (status = 400, description = "Session ID is not an integer"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Session not found")
    ),
    security(("auth-token" = [])),
    tag = "Chat"
)]
pub async fn api_get_session(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<Json<SessionWithMessages>> {
    debug!(
        "HANDLER ->> GET /api/chat/sessions/{id} user_id={}",
        user.id
    );

    let session = owned_session(&pool, user.id, id).await?;

    let messages = sqlx::query_as!(
        ChatMessage,
        r#"
        SELECT id, session_id, role, content, created_at
        FROM chat_messages
        WHERE session_id = $1
        ORDER BY created_at ASC, id ASC
        "#,
        session.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    Ok(Json(SessionWithMessages::new(&session, &messages)))
}

/// State carried through the SSE stream for one reply.
struct ReplyState {
    ai: AiStream,
    pool: PgPool,
    session_id: i32,
    user_text: String,
    /// Released as soon as the terminal event is produced, or when the stream is dropped
    /// (client disconnect).
    guard: Option<InFlightGuard>,
}

/// Maps the next ai-service event to an SSE event, saving the turn on `done`.
async fn next_sse_event(mut state: ReplyState) -> Option<(Result<Event, Infallible>, ReplyState)> {
    let event = match state.ai.next_event().await? {
        AiEvent::Status { tool } => sse_event("status", &json!({ "tool": tool })),
        AiEvent::Token { text } => sse_event("token", &json!({ "text": text })),
        AiEvent::Done { response } => {
            let saved = save_turn(&state.pool, state.session_id, &state.user_text, &response).await;
            state.guard = None;
            if let Ok(message) = saved {
                info!(
                    session_id = state.session_id,
                    outcome = "done",
                    "chat reply saved"
                );
                sse_event("done", &json!({ "message": Message::new(&message) }))
            } else {
                warn!(
                    session_id = state.session_id,
                    outcome = "save_failed",
                    "chat turn could not be saved"
                );
                sse_event(
                    "error",
                    &json!({
                        "code": "save_failed",
                        "message": "The reply could not be saved."
                    }),
                )
            }
        }
        AiEvent::Error { code, message } => {
            state.guard = None;
            warn!(
                session_id = state.session_id,
                outcome = "ai_error",
                "chat reply failed"
            );
            sse_event("error", &json!({ "code": code, "message": message }))
        }
    };
    Some((Ok(event), state))
}

/// Send a prompt and stream the reply as Server-Sent Events.
///
/// Events: `status` (`{"tool"}`), `token` (`{"text"}`), `done` (`{"message"}`) and
/// `error` (`{"code","message"}`). Nothing is saved unless `done` is sent.
///
/// `POST /api/chat/sessions/{id}/messages`
#[utoipa::path(
    post,
    path = "/sessions/{id}/messages",
    summary = "Send a message and stream the reply",
    params(("id" = i32, Path, description = "Session ID")),
    request_body(
        content = SendMessageRequest,
        content_type = "application/json",
        example = json!({"message": "How much did I spend on groceries this month?"})
    ),
    responses(
        (status = 200, description = "SSE stream of status, token, done and error events", content_type = "text/event-stream", body = String),
        (status = 400, description = "Empty or over-length message, or session ID is not an integer"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Session not found"),
        (status = 409, description = "A reply is already streaming in this session"),
        (status = 503, description = "The assistant is unavailable")
    ),
    security(("auth-token" = [])),
    tag = "Chat"
)]
pub async fn api_send_message(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Extension(ai): Extension<AiClient>,
    Extension(in_flight): Extension<InFlight>,
    Path(id): Path<i32>,
    Json(payload): Json<SendMessageRequest>,
) -> ApiResult<Response> {
    debug!(
        "HANDLER ->> POST /api/chat/sessions/{id}/messages user_id={}",
        user.id
    );

    let text = payload.trimmed()?.to_string();

    let session = owned_session(&pool, user.id, id).await?;

    let guard = InFlightGuard::acquire(&in_flight, session.id).ok_or_else(|| {
        AppError::Conflict("a reply is already streaming in this session".to_string())
    })?;

    // Read history before the stream starts, so it excludes the new message.
    let recent = sqlx::query_as!(
        ChatMessage,
        r#"
        SELECT id, session_id, role, content, created_at
        FROM (
            SELECT id, session_id, role, content, created_at
            FROM chat_messages
            WHERE session_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2
        ) latest
        ORDER BY created_at ASC, id ASC
        "#,
        session.id,
        HISTORY_LIMIT
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;
    let history: Vec<HistoryItem> = recent
        .into_iter()
        .map(|m| HistoryItem {
            role: m.role,
            content: m.content,
        })
        .collect();

    info!(
        session_id = session.id,
        message_len = text.chars().count(),
        "chat message received"
    );

    let ai_stream = match ai.start(user.id, &text, history).await {
        Ok(s) => s,
        Err(StartError::Unavailable) => {
            warn!(
                session_id = session.id,
                outcome = "unavailable",
                "chat ai-service unavailable"
            );
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": ASSISTANT_UNAVAILABLE_MESSAGE })),
            )
                .into_response());
        }
    };

    let state = ReplyState {
        ai: ai_stream,
        pool,
        session_id: session.id,
        user_text: text,
        guard: Some(guard),
    };

    Ok(Sse::new(stream::unfold(state, next_sse_event))
        .keep_alive(KeepAlive::default())
        .into_response())
}

/// `OpenAPI` doc for chat routes.
///
/// Constructed only by the `#[openapi]` macro and the root [`crate::swagger::ApiDoc`].
#[derive(OpenApi)]
#[openapi(
    paths(
        api_list_sessions,
        api_create_session,
        api_get_session,
        api_send_message
    ),
    components(schemas(
        SendMessageRequest,
        Session,
        SessionsResponse,
        SessionWithMessages,
        Message
    )),
    info(title = "Chat Routes", description = "AI assistant chat sessions and streamed replies."),
    tags((name = "Chat"))
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct ChatApiDoc;

/// Builds the chat route group. All routes require a valid `auth-token` cookie.
pub fn chat_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/sessions", get(api_list_sessions).post(api_create_session))
        .route("/sessions/{id}", get(api_get_session))
        .route("/sessions/{id}/messages", post(api_send_message))
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
