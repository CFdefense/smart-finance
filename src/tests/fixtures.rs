//! Integration test fixtures and helper functions.

use axum::{body::Body, http::Request, routing::post, Json, Router};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{db, plaid::client::PlaidClient};

/// How the mock Plaid server responds.
#[derive(Clone, Copy)]
pub enum MockMode {
    /// Valid Plaid-shaped responses.
    Ok,
    /// Every endpoint returns HTTP 500.
    ServerError,
    /// Every endpoint returns HTTP 200 with a JSON body of the wrong shape.
    BadBody,
}

/// Starts a local mock Plaid API and returns its base URL.
///
/// `tag` is embedded in item/account/transaction IDs so parallel tests never collide.
///
/// # Panics
///
/// Panics if a local TCP listener cannot be bound.
pub async fn spawn_mock_plaid(tag: &str, mode: MockMode) -> String {
    let app = match mode {
        MockMode::Ok => {
            let t = tag.to_string();
            let (t1, t2, t3) = (t.clone(), t.clone(), t);
            Router::new()
                .route(
                    "/link/token/create",
                    post(|| async { Json(json!({"link_token": "link-mock"})) }),
                )
                .route(
                    "/item/public_token/exchange",
                    post(move || {
                        let item_id = format!("item-{t1}");
                        async move {
                            Json(json!({"access_token": "access-mock", "item_id": item_id}))
                        }
                    }),
                )
                .route(
                    "/accounts/get",
                    post(move || {
                        let t = t2.clone();
                        async move {
                            Json(json!({
                                "item": {"item_id": format!("item-{t}")},
                                "accounts": [{
                                    "account_id": format!("acct-{t}"),
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
                        }
                    }),
                )
                .route(
                    "/transactions/sync",
                    post(move |Json(body): Json<Value>| {
                        let t = t3.clone();
                        async move {
                            let tx = |n: u8| {
                                json!({
                                    "transaction_id": format!("tx{n}-{t}"),
                                    "account_id": format!("acct-{t}"),
                                    "name": "Coffee",
                                    "amount": 4.5,
                                    "date": "2024-01-01",
                                    "pending": false
                                })
                            };
                            if body.get("cursor").is_some() {
                                Json(json!({
                                    "added": [],
                                    "modified": [tx(1)],
                                    "removed": [{"transaction_id": format!("tx2-{t}")}],
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
                        }
                    }),
                )
        }
        MockMode::ServerError => Router::new().fallback(|| async {
            (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "plaid mock failure",
            )
        }),
        MockMode::BadBody => Router::new().fallback(|| async { Json(json!({})) }),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

/// Like [`test_app`], but the Plaid client targets `plaid_base_url`.
pub async fn test_app_with_plaid(plaid_base_url: &str) -> Router {
    dotenvy::dotenv().ok();
    crate::create_router_with_plaid(
        "http://localhost:5173",
        PlaidClient::with_base_url(plaid_base_url),
    )
    .await
}

/// Builds the full application router for testing (no CORS, no Swagger).
///
/// Call `create_router` with a dummy origin — CORS is not enforced in unit/integration
/// tests, so any placeholder string is fine. The returned `Router` is ready for
/// [`tower::ServiceExt::oneshot`] calls.
pub async fn test_app() -> Router {
    dotenvy::dotenv().ok();
    crate::create_router("http://localhost:5173").await
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

/// Builds a JSON body for signup/login requests.
pub fn auth_body(email: &str, password: &str) -> Body {
    Body::from(json!({"email": email, "password": password}).to_string())
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
                .body(auth_body(email, "Secret_123"))
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

/// Generates a unique tag (used in mock Plaid IDs) to prevent cross-test collisions.
///
/// # Panics
///
/// Panics if system time is before [`UNIX_EPOCH`].
pub fn unique_tag(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{prefix}{nanos}")
}
