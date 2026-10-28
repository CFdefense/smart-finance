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

/// Extracts suffix for test isolation (e.g. `access-seed-123` -> `seed-123`).
fn suffix(token: &str) -> &str {
    if let Some(rest) = token.strip_prefix("access-") {
        rest
    } else if let Some(rest) = token.strip_prefix("public-") {
        rest
    } else if token.is_empty() {
        "mock"
    } else {
        token
    }
}

/// A fake Plaid API with isolated or fixed IDs.
///
/// Failure cases are triggered by the token sent in the request; see `MOCK_FAIL_*`.
fn mock_plaid_router() -> Router {
    Router::new()
        .route(
            "/link/token/create",
            post(|| async { Json(json!({"link_token": "link-mock"})) }),
        )
        .route(
            "/sandbox/public_token/create",
            post(|Json(body): Json<Value>| async move {
                let user = body
                    .get("options")
                    .and_then(|o| o.get("override_username"))
                    .and_then(Value::as_str)
                    .unwrap_or("mock");
                Json(json!({"public_token": format!("public-{user}")}))
            }),
        )
        .route(
            "/sandbox/item/fire_webhook",
            post(|| async { Json(json!({"webhook_fired": true})) }),
        )
        .route(
            "/item/public_token/exchange",
            post(|Json(body): Json<Value>| async move {
                let token = field(&body, "public_token");
                let s = suffix(token);
                mock_failure(token).unwrap_or_else(|| {
                    Json(json!({
                        "access_token": format!("access-{s}"),
                        "item_id": format!("item-{s}")
                    }))
                    .into_response()
                })
            }),
        )
        .route(
            "/accounts/get",
            post(|Json(body): Json<Value>| async move {
                let token = field(&body, "access_token");
                let s = suffix(token);
                mock_failure(token).unwrap_or_else(|| {
                    Json(json!({
                        "item": {"item_id": format!("item-{s}")},
                        "accounts": [{
                            "account_id": format!("acct-{s}"),
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
                let token = field(&body, "access_token");
                let s = suffix(token);
                if let Some(failure) = mock_failure(token) {
                    return failure;
                }
                let tx = move |n: u8| {
                    json!({
                        "transaction_id": format!("tx{n}-{s}"),
                        "account_id": format!("acct-{s}"),
                        "name": "Coffee",
                        "amount": 4.5,
                        "date": "2024-01-01",
                        "pending": false,
                        "personal_finance_category": if n == 1 {
                            json!({"primary": "FOOD_AND_DRINK", "detailed": "FOOD_AND_DRINK_COFFEE"})
                        } else {
                            Value::Null
                        }
                    })
                };
                // First page (no cursor) has more; the follow-up page modifies/removes.
                if body.get("cursor").is_some() {
                    let tx2_id = format!("tx2-{s}");
                    Json(json!({
                        "added": [],
                        "modified": [tx(1)],
                        "removed": [{"transaction_id": tx2_id}],
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
pub fn mock_plaid_url() -> &'static str {
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

/// Sends a request (optionally authenticated and with a JSON body) and returns `(status, json body)`.
///
/// `cookie` may be empty for unauthenticated calls. The body is `Value::Null` when the
/// response is empty or not JSON.
///
/// # Panics
///
/// Panics if the request cannot be built or the router fails to respond.
pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if !cookie.is_empty() {
        builder = builder.header("cookie", cookie);
    }
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Looks up a user's database id by email.
///
/// # Panics
///
/// Panics if the user does not exist.
pub async fn user_id(pool: &PgPool, email: &str) -> i32 {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Returns an ID that is unique across tests and runs (Plaid account/item IDs are global).
///
/// # Panics
///
/// Panics if system time is before [`UNIX_EPOCH`].
pub fn unique_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{nanos}-{n}")
}

/// Seeds a Plaid link plus one account (`account_type`, e.g. `depository`) for `user_id`.
///
/// Returns the new `account_id`. Rows are removed when the user is deleted
/// (`ON DELETE CASCADE`), so [`cleanup`] is enough.
///
/// # Panics
///
/// Panics if the inserts fail.
pub async fn seed_account(
    pool: &PgPool,
    user_id: i32,
    account_type: &str,
    balance: Option<f64>,
) -> String {
    let account_id = unique_id("acct");
    let item_id = unique_id("item");
    sqlx::query("INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(&item_id)
        .bind("access-seeded")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO plaid_accounts (account_id, item_id, name, account_type, current_balance)
         VALUES ($1, $2, 'Seeded', $3, $4)",
    )
    .bind(&account_id)
    .bind(&item_id)
    .bind(account_type)
    .bind(balance)
    .execute(pool)
    .await
    .unwrap();
    account_id
}

/// Sets an account's current balance.
///
/// # Panics
///
/// Panics if the update fails.
pub async fn set_balance(pool: &PgPool, account_id: &str, balance: f64) {
    sqlx::query("UPDATE plaid_accounts SET current_balance = $1 WHERE account_id = $2")
        .bind(balance)
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Seeds a transaction on `account_id` and returns its `transaction_id`.
///
/// # Panics
///
/// Panics if the insert fails.
pub async fn seed_transaction(
    pool: &PgPool,
    account_id: &str,
    amount: f64,
    date: chrono::NaiveDate,
    category: &str,
) -> String {
    let transaction_id = unique_id("tx");
    sqlx::query(
        "INSERT INTO plaid_transactions (transaction_id, account_id, name, amount, date, category)
         VALUES ($1, $2, 'Seeded', $3, $4, $5)",
    )
    .bind(&transaction_id)
    .bind(account_id)
    .bind(amount)
    .bind(date)
    .bind(category)
    .execute(pool)
    .await
    .unwrap();
    transaction_id
}
