//! Integration tests for AI chat endpoints (`/api/chat`).

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use axum::body::Bytes;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::fixtures::{
    authenticated_cookie, call, cleanup, test_app, test_app_with_ai, test_pool, unique_email,
};
use crate::chat::AiClient;

/// Parsed Server-Sent Event block from the chat streaming route.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SseEvent {
    event: String,
    data: Value,
}

/// Parses raw SSE body bytes into individual [`SseEvent`] items.
fn parse_sse_events(bytes: &[u8]) -> Vec<SseEvent> {
    let text = String::from_utf8_lossy(bytes);
    let mut events = Vec::new();
    for block in text.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let mut event_name = String::new();
        let mut data_str = String::new();
        for line in block.lines() {
            let line = line.trim();
            if line.starts_with(':') {
                continue;
            }
            if let Some(ev) = line.strip_prefix("event:") {
                event_name = ev.trim().to_string();
            } else if let Some(d) = line.strip_prefix("data:") {
                if !data_str.is_empty() {
                    data_str.push('\n');
                }
                data_str.push_str(d.trim());
            }
        }
        if !event_name.is_empty() || !data_str.is_empty() {
            let data_json: Value =
                serde_json::from_str(&data_str).unwrap_or(Value::String(data_str));
            events.push(SseEvent {
                event: event_name,
                data: data_json,
            });
        }
    }
    events
}

/// Sends an authenticated raw request to the application router.
async fn call_raw(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<Value>,
) -> (StatusCode, Bytes) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
    let body = match body {
        Some(b) => {
            builder = builder.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    (status, bytes)
}

/// Helper that posts a message to a session SSE stream and parses the returned events.
async fn call_send_sse(
    app: &Router,
    session_id: i32,
    cookie: &str,
    message: &str,
) -> (StatusCode, Vec<SseEvent>) {
    let (status, bytes) = call_raw(
        app,
        "POST",
        &format!("/api/chat/sessions/{session_id}/messages"),
        cookie,
        Some(json!({ "message": message })),
    )
    .await;
    let events = if status.is_success() {
        parse_sse_events(&bytes)
    } else {
        Vec::new()
    };
    (status, events)
}

/// Spawns a mock AI server on an ephemeral port in a background thread/runtime.
fn spawn_mock_ai(router: Router) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind tcp listener");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking listener");
    let addr = listener.local_addr().expect("retrieve local addr");
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .expect("create tokio runtime")
            .block_on(async {
                let listener =
                    tokio::net::TcpListener::from_std(listener).expect("convert to tokio listener");
                axum::serve(listener, router).await.ok();
            });
    });
    format!("http://{addr}")
}

// ─── Auth-gate tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn list_sessions_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/chat/sessions", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_session_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "POST", "/api/chat/sessions", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_session_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/chat/sessions/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn send_message_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/chat/sessions/1/messages",
        "",
        Some(json!({ "message": "Hello" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── CRUD / Sessions tests ───────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_sessions() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("chat_crud");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Create session 1
    let (status1, created1) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status1, StatusCode::CREATED);
    let s1_id = created1["id"].as_i64().expect("session1 id");
    assert!(created1["title"].is_null());
    assert!(created1.get("created_at").is_some());
    assert!(created1.get("updated_at").is_some());

    // Create session 2
    let (status2, created2) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status2, StatusCode::CREATED);
    let s2_id = created2["id"].as_i64().expect("session2 id");

    // List sessions: newest first
    let (list_status, list_json) = call(&app, "GET", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(list_status, StatusCode::OK);
    let sessions = list_json["sessions"].as_array().expect("sessions array");
    assert!(sessions.len() >= 2);
    assert_eq!(sessions[0]["id"].as_i64(), Some(s2_id));
    assert_eq!(sessions[1]["id"].as_i64(), Some(s1_id));

    // Get session 1 with empty messages
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{s1_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_json["id"].as_i64(), Some(s1_id));
    assert!(get_json["title"].is_null());
    let messages = get_json["messages"].as_array().expect("messages array");
    assert!(messages.is_empty());

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn foreign_user_session_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("chat_owner_a");
    let email_b = unique_email("chat_owner_b");
    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    // User A creates a session
    let (status, session_a) = call(&app, "POST", "/api/chat/sessions", &cookie_a, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let s_id = session_a["id"].as_i64().expect("session id");

    // User B tries to GET session A -> 404
    let (get_status, _) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{s_id}"),
        &cookie_b,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::NOT_FOUND);

    // User B tries to send message to session A -> 404
    let (send_status, _) = call(
        &app,
        "POST",
        &format!("/api/chat/sessions/{s_id}/messages"),
        &cookie_b,
        Some(json!({ "message": "Can I see your data?" })),
    )
    .await;
    assert_eq!(send_status, StatusCode::NOT_FOUND);

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
}

#[tokio::test]
async fn non_integer_session_id_bad_request() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("chat_non_int");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (get_status, _) = call(
        &app,
        "GET",
        "/api/chat/sessions/not-a-number",
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::BAD_REQUEST);

    let (send_status, _) = call(
        &app,
        "POST",
        "/api/chat/sessions/not-a-number/messages",
        &cookie,
        Some(json!({ "message": "Hello" })),
    )
    .await;
    assert_eq!(send_status, StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_validation_rejects_empty_and_oversized() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("chat_validation");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = session["id"].as_i64().expect("session id");

    // Empty message
    let (empty_status, _) = call(
        &app,
        "POST",
        &format!("/api/chat/sessions/{session_id}/messages"),
        &cookie,
        Some(json!({ "message": "" })),
    )
    .await;
    assert_eq!(empty_status, StatusCode::BAD_REQUEST);

    // Whitespace only message
    let (ws_status, _) = call(
        &app,
        "POST",
        &format!("/api/chat/sessions/{session_id}/messages"),
        &cookie,
        Some(json!({ "message": "   \n\t   " })),
    )
    .await;
    assert_eq!(ws_status, StatusCode::BAD_REQUEST);

    // 2001 chars message (limit is 2000)
    let long_msg = "a".repeat(2001);
    let (long_status, _) = call(
        &app,
        "POST",
        &format!("/api/chat/sessions/{session_id}/messages"),
        &cookie,
        Some(json!({ "message": long_msg })),
    )
    .await;
    assert_eq!(long_status, StatusCode::BAD_REQUEST);

    // Confirm nothing was saved in the session
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let messages = get_json["messages"].as_array().expect("messages array");
    assert!(messages.is_empty());

    cleanup(&pool, &email).await;
}

// ─── Streaming / Relay tests ─────────────────────────────────────────────────

#[tokio::test]
async fn send_message_happy_path() {
    let mock_router = Router::new().route(
        "/chat/stream",
        post(|| async {
            let stream = futures_util::stream::iter(vec![
                Ok::<_, std::io::Error>(Bytes::from(
                    "{\"type\":\"status\",\"tool\":\"web_search\"}\n",
                )),
                Ok::<_, std::io::Error>(Bytes::from("{\"type\":\"token\",\"text\":\"Hello \"}\n")),
                Ok::<_, std::io::Error>(Bytes::from("{\"type\":\"token\",\"text\":\"world!\"}\n")),
                Ok::<_, std::io::Error>(Bytes::from(
                    "{\"type\":\"done\",\"response\":\"Hello world!\"}\n",
                )),
            ]);
            Body::from_stream(stream).into_response()
        }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_happy");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();

    let prompt = "What is the recommended emergency fund size for a family of four in 2026?";
    let (send_status, events) = call_send_sse(&app, session_id, &cookie, prompt).await;
    assert_eq!(send_status, StatusCode::OK);
    assert_eq!(events.len(), 4);

    assert_eq!(events[0].event, "status");
    assert_eq!(events[0].data, json!({ "tool": "web_search" }));

    assert_eq!(events[1].event, "token");
    assert_eq!(events[1].data, json!({ "text": "Hello " }));

    assert_eq!(events[2].event, "token");
    assert_eq!(events[2].data, json!({ "text": "world!" }));

    assert_eq!(events[3].event, "done");
    let stored_msg = &events[3].data["message"];
    assert_eq!(stored_msg["role"], "assistant");
    assert_eq!(stored_msg["content"], "Hello world!");
    assert!(stored_msg["id"].as_i64().is_some());

    // Verify session retrieval contains messages oldest first and title = first 60 chars
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let expected_title = prompt.chars().take(60).collect::<String>();
    assert_eq!(
        get_json["title"].as_str(),
        Some(expected_title.as_str()),
        "title should be first 60 chars of message"
    );

    let messages = get_json["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], prompt);
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "Hello world!");

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_multibyte_title_truncation() {
    let mock_router = Router::new().route(
        "/chat/stream",
        post(|| async {
            let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(
                "{\"type\":\"done\",\"response\":\"Got it\"}\n",
            ))]);
            Body::from_stream(stream).into_response()
        }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_multibyte_title");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();

    // 70 multi-byte characters (each 'é' is 2 UTF-8 bytes: 140 bytes total).
    // Title truncation must produce 60 characters (120 bytes), not 60 bytes (30 chars).
    let prompt = "é".repeat(70);
    let (send_status, events) = call_send_sse(&app, session_id, &cookie, &prompt).await;
    assert_eq!(send_status, StatusCode::OK);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "done");

    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let expected_title: String = prompt.chars().take(60).collect();
    assert_eq!(
        get_json["title"].as_str(),
        Some(expected_title.as_str()),
        "title should be first 60 characters"
    );
    assert_eq!(get_json["title"].as_str().unwrap().chars().count(), 60);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_in_flight_conflict_409() {
    let request_started = Arc::new(AtomicBool::new(false));
    let request_started_clone = request_started.clone();

    let mock_router = Router::new().route(
        "/chat/stream",
        post(move || {
            let request_started = request_started_clone.clone();
            async move {
                request_started.store(true, Ordering::SeqCst);
                let stream = futures_util::stream::unfold(0, |step| async move {
                    match step {
                        0 => {
                            let bytes = Bytes::from("{\"type\":\"token\",\"text\":\"stream1\"}\n");
                            Some((Ok::<_, std::io::Error>(bytes), 1))
                        }
                        1 => {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            let bytes = Bytes::from(
                                "{\"type\":\"done\",\"response\":\"stream1 finished\"}\n",
                            );
                            Some((Ok::<_, std::io::Error>(bytes), 2))
                        }
                        _ => None,
                    }
                });
                Body::from_stream(stream).into_response()
            }
        }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_conflict");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();

    // Start stream 1 in background task
    let app_clone = app.clone();
    let cookie_clone = cookie.clone();
    let stream1_task = tokio::spawn(async move {
        call_send_sse(&app_clone, session_id, &cookie_clone, "Prompt 1").await
    });

    // Wait deterministically until stream 1 has reached the mock and acquired the guard
    let mut reached = false;
    for _ in 0..200 {
        if request_started.load(Ordering::SeqCst) {
            reached = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(reached, "stream 1 did not reach mock within 2s");

    // Send stream 2 on the same session -> 409 Conflict
    let (status2, _) = call(
        &app,
        "POST",
        &format!("/api/chat/sessions/{session_id}/messages"),
        &cookie,
        Some(json!({ "message": "Prompt 2" })),
    )
    .await;
    assert_eq!(status2, StatusCode::CONFLICT);

    // Wait for stream 1 to finish
    let (status1, events1) = stream1_task.await.expect("join task 1");
    assert_eq!(status1, StatusCode::OK);
    assert_eq!(events1.len(), 2);

    // After stream 1 is done, a subsequent send should succeed
    let (status3, events3) = call_send_sse(&app, session_id, &cookie, "Prompt 3").await;
    assert_eq!(status3, StatusCode::OK);
    assert_eq!(events3.len(), 2);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_ai_unavailable_503() {
    let mock_router = Router::new().route(
        "/chat/stream",
        post(|| async { (StatusCode::SERVICE_UNAVAILABLE, "AI unavailable").into_response() }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_503");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = session["id"].as_i64().expect("session id");

    let (send_status, bytes) = call_raw(
        &app,
        "POST",
        &format!("/api/chat/sessions/{session_id}/messages"),
        &cookie,
        Some(json!({ "message": "Hi assistant" })),
    )
    .await;
    assert_eq!(send_status, StatusCode::SERVICE_UNAVAILABLE);

    let err_json: Value = serde_json::from_slice(&bytes).expect("parse json error body");
    assert_eq!(
        err_json["error"],
        "The assistant is unavailable right now. Your other data still works."
    );

    // Confirm nothing was saved
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let messages = get_json["messages"].as_array().expect("messages array");
    assert!(messages.is_empty());

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_mid_stream_ai_error() {
    let mock_router = Router::new().route(
        "/chat/stream",
        post(|| async {
            let stream = futures_util::stream::iter(vec![
                Ok::<_, std::io::Error>(Bytes::from(
                    "{\"type\":\"token\",\"text\":\"Partial answer\"}\n",
                )),
                Ok::<_, std::io::Error>(Bytes::from(
                    "{\"type\":\"error\",\"code\":\"ai_failed\",\"message\":\"LLM service error\"}\n",
                )),
            ]);
            Body::from_stream(stream).into_response()
        }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_mid_err");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();

    let (send_status, events) = call_send_sse(&app, session_id, &cookie, "My question").await;
    assert_eq!(send_status, StatusCode::OK);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event, "token");
    assert_eq!(events[0].data, json!({ "text": "Partial answer" }));
    assert_eq!(events[1].event, "error");
    assert_eq!(events[1].data["code"], "ai_failed");
    assert_eq!(events[1].data["message"], "LLM service error");

    // Nothing was saved
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let messages = get_json["messages"].as_array().expect("messages array");
    assert!(messages.is_empty());

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_unexpected_eof_ai_error() {
    let mock_router = Router::new().route(
        "/chat/stream",
        post(|| async {
            let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(
                "{\"type\":\"token\",\"text\":\"Incomplete\"}\n",
            ))]);
            Body::from_stream(stream).into_response()
        }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_eof_err");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();

    let (send_status, events) = call_send_sse(&app, session_id, &cookie, "My question").await;
    assert_eq!(send_status, StatusCode::OK);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event, "token");
    assert_eq!(events[0].data, json!({ "text": "Incomplete" }));
    assert_eq!(events[1].event, "error");
    assert_eq!(events[1].data["code"], "ai_failed");

    // Nothing was saved
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let messages = get_json["messages"].as_array().expect("messages array");
    assert!(messages.is_empty());

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_passes_history_and_authenticated_user_id() {
    let recorded_requests: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded_requests);

    let mock_router = Router::new().route(
        "/chat/stream",
        post(move |Json(payload): Json<Value>| {
            let recorded = Arc::clone(&recorded_clone);
            async move {
                recorded.lock().unwrap().push(payload);
                let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
                    Bytes::from("{\"type\":\"done\",\"response\":\"Acknowledged\"}\n"),
                )]);
                Body::from_stream(stream).into_response()
            }
        }),
    );

    let mock_url = spawn_mock_ai(mock_router);
    let app = test_app_with_ai(AiClient::with_base_url(mock_url)).await;
    let pool = test_pool().await;
    let email = unique_email("chat_history");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Retrieve the authenticated user's database ID
    let db_user_id: i32 = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .expect("fetch user id");

    let (status, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();

    // Turn 1
    let (send_status1, events1) = call_send_sse(&app, session_id, &cookie, "Question 1").await;
    assert_eq!(send_status1, StatusCode::OK);
    assert_eq!(events1.len(), 1);
    assert_eq!(events1[0].event, "done");

    // Turn 2
    let (send_status2, events2) = call_send_sse(&app, session_id, &cookie, "Question 2").await;
    assert_eq!(send_status2, StatusCode::OK);
    assert_eq!(events2.len(), 1);
    assert_eq!(events2[0].event, "done");

    // Inspect recorded requests
    {
        let recorded = recorded_requests.lock().unwrap();
        assert_eq!(recorded.len(), 2);

        // Call 1
        assert_eq!(recorded[0]["user_id"].as_i64(), Some(i64::from(db_user_id)));
        assert_eq!(recorded[0]["message"], "Question 1");
        let h1 = recorded[0]["history"].as_array().expect("history array 1");
        assert!(h1.is_empty());

        // Call 2: history should contain previous turn in oldest-first order
        assert_eq!(recorded[1]["user_id"].as_i64(), Some(i64::from(db_user_id)));
        assert_eq!(recorded[1]["message"], "Question 2");
        let h2 = recorded[1]["history"].as_array().expect("history array 2");
        assert_eq!(h2.len(), 2);
        assert_eq!(h2[0]["role"], "user");
        assert_eq!(h2[0]["content"], "Question 1");
        assert_eq!(h2[1]["role"], "assistant");
        assert_eq!(h2[1]["content"], "Acknowledged");
    }

    // Verify GET session has all 4 messages
    let (get_status, get_json) = call(
        &app,
        "GET",
        &format!("/api/chat/sessions/{session_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    let messages = get_json["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 4);
    // The title comes from the first turn and is not overwritten by later ones.
    assert_eq!(get_json["title"], "Question 1");

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn send_message_sends_only_the_last_20_messages_as_history() {
    let recorded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded);
    let mock_router = Router::new().route(
        "/chat/stream",
        post(move |Json(payload): Json<Value>| {
            let recorded = Arc::clone(&recorded_clone);
            async move {
                recorded.lock().unwrap().push(payload);
                let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
                    Bytes::from("{\"type\":\"done\",\"response\":\"ok\"}\n"),
                )]);
                Body::from_stream(stream).into_response()
            }
        }),
    );
    let app = test_app_with_ai(AiClient::with_base_url(spawn_mock_ai(mock_router))).await;
    let pool = test_pool().await;
    let email = unique_email("chat_limit");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (_, session) = call(&app, "POST", "/api/chat/sessions", &cookie, None).await;
    let session_id = i32::try_from(session["id"].as_i64().expect("session id")).unwrap();
    for n in 1..=25 {
        sqlx::query(
            "INSERT INTO chat_messages (session_id, role, content) VALUES ($1, 'user', $2)",
        )
        .bind(session_id)
        .bind(format!("old {n}"))
        .execute(&pool)
        .await
        .expect("seed message");
    }

    let (status, _) = call_send_sse(&app, session_id, &cookie, "new").await;
    assert_eq!(status, StatusCode::OK);

    {
        let recorded = recorded.lock().unwrap();
        let history = recorded[0]["history"].as_array().expect("history array");
        assert_eq!(history.len(), 20);
        assert_eq!(history[0]["content"], "old 6");
        assert_eq!(history[19]["content"], "old 25");
    }

    cleanup(&pool, &email).await;
}
