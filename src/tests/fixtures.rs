//! Integration test fixtures and helper functions.

use axum::{body::Body, http::Request, Router};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;

use crate::db;

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
