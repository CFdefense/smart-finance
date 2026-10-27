//! Integration test fixtures and helper functions.

use std::sync::OnceLock;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{db, plaid::client::PlaidClient};

/// Token value that makes the mock Plaid API reply with HTTP 500.
pub const MOCK_FAIL_500: &str = "fail-500";
/// Token value that makes the mock Plaid API reply with HTTP 200 and an unparseable body.
pub const MOCK_FAIL_BODY: &str = "fail-body";

/// Returns a failure response when `token` is one of the `MOCK_FAIL_*` markers
/// (as a whole value or as a suffix, e.g. `access-fail-500`).
fn mock_failure(token: &str) -> Option<Response> {
    if token.ends_with(MOCK_FAIL_500) {
        Some((StatusCode::INTERNAL_SERVER_ERROR, "plaid mock failure").into_response())
    } else if token.ends_with(MOCK_FAIL_BODY) {
        Some(Json(json!({})).into_response())
    } else {
        None
    }
}

/// Reads a string field from a mock request body (empty if absent).
fn field<'a>(body: &'a Value, key: &str) -> &'a str {
    body.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A fake Plaid API with fixed IDs (`item-mock`, `acct-mock`, `tx1-mock`, `tx2-mock`).
///
/// Failure cases are triggered by the token sent in the request; see `MOCK_FAIL_*`.
fn mock_plaid_router() -> Router {
    Router::new()
        .route(
            "/link/token/create",
            post(|| async { Json(json!({"link_token": "link-mock"})) }),
        )
        .route(
            "/item/public_token/exchange",
            post(|Json(body): Json<Value>| async move {
                mock_failure(field(&body, "public_token")).unwrap_or_else(|| {
                    Json(json!({"access_token": "access-mock", "item_id": "item-mock"}))
                        .into_response()
                })
            }),
        )
        .route(
            "/accounts/get",
            post(|Json(body): Json<Value>| async move {
                mock_failure(field(&body, "access_token")).unwrap_or_else(|| {
                    Json(json!({
                        "item": {"item_id": "item-mock"},
                        "accounts": [{
                            "account_id": "acct-mock",
                            "name": "Checking",
                            "official_name": null,
                            "type": "depository",
                            "subtype": "checking",
                            "mask": "0000",
                            "balances": {
                                "current": 100.0,
                                "available": 90.0,
                                "iso_currency_code": "USD"
                            }
                        }]
                    }))
                    .into_response()
                })
            }),
        )
        .route(
            "/transactions/sync",
            post(|Json(body): Json<Value>| async move {
                if let Some(failure) = mock_failure(field(&body, "access_token")) {
                    return failure;
                }
                let tx = |n: u8| {
                    json!({
                        "transaction_id": format!("tx{n}-mock"),
                        "account_id": "acct-mock",
                        "name": "Coffee",
                        "amount": 4.5,
                        "date": "2024-01-01",
                        "pending": false
                    })
                };
                // First page (no cursor) has more; the follow-up page modifies/removes.
                if body.get("cursor").is_some() {
                    Json(json!({
                        "added": [],
                        "modified": [tx(1)],
                        "removed": [{"transaction_id": "tx2-mock"}],
                        "next_cursor": "c2",
                        "has_more": false
                    }))
                } else {
                    Json(json!({
                        "added": [tx(1), tx(2)],
                        "modified": [],
                        "removed": [],
                        "next_cursor": "c1",
                        "has_more": true
                    }))
                }
                .into_response()
            }),
        )
}

/// Starts the shared mock Plaid server once and returns its base URL.
///
/// It runs on its own thread and runtime because every `#[tokio::test]` has a
/// separate runtime that is dropped when the test ends.
fn mock_plaid_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, mock_plaid_router()).await.ok();
            });
        });
        format!("http://{addr}")
    })
}

/// Builds the full application router for testing (no CORS, no Swagger).
///
/// Plaid calls go to a local mock server, so no real credentials are needed.
/// CORS is not enforced in unit/integration tests, so the origin is a placeholder.
/// The returned `Router` is ready for [`tower::ServiceExt::oneshot`] calls.
pub async fn test_app() -> Router {
    dotenvy::dotenv().ok();
    crate::create_router(
        "http://localhost:5173",
        PlaidClient::with_base_url(mock_plaid_url()),
    )
    .await
}

/// Returns a fresh connection pool (shares the same `DATABASE_URL` as `test_app`).
pub async fn test_pool() -> PgPool {
    dotenvy::dotenv().ok();
    db::create_pool().await
}

/// Generates a unique test email to prevent conflicts between parallel test runs.
///
/// # Panics
///
/// Panics if system time is before [`UNIX_EPOCH`].
pub fn unique_email(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("test+{prefix}{nanos}@example.com")
}

/// Deletes a test user by email to clean up after each test.
pub async fn cleanup(pool: &PgPool, email: &str) {
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .ok();
}

/// Signs up a fresh user and returns the `auth-token=<value>` cookie string.
///
/// Use this to make authenticated requests in Plaid integration tests.
pub async fn authenticated_cookie(app: Router, email: &str) -> String {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": email, "password": "Secret_123"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .expect("signup must set a cookie")
        .to_str()
        .unwrap()
        .to_string();

    set_cookie.split(';').next().unwrap().trim().to_string()
}
