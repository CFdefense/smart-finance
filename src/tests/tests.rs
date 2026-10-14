//! Integration tests for the Smart Finance API.
//!
//! Each test spins up the full Axum router in-process against a real Postgres
//! database. Set `DATABASE_URL` in `.env` or the environment before running.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use tower_cookies::{cookie::Key, CookieManagerLayer};

use crate::{controllers::user::user_routes, controllers::AxumRouter, db};

/// Builds the full application router for testing.
///
/// Identical to `main()` minus CORS and Swagger UI.
async fn test_app() -> (axum::Router, PgPool) {
    dotenvy::dotenv().ok();
    let pool = db::create_pool().await;
    let key = Key::generate();

    let api_router = AxumRouter::new().nest("/user", user_routes());
    let api_router = AxumRouter::new().nest("/api", api_router);

    let app = axum::Router::new()
        .merge(api_router)
        .layer(axum::Extension(pool.clone()))
        .layer(axum::Extension(key))
        .layer(CookieManagerLayer::new());

    (app, pool)
}

/// Generates a unique test email to prevent conflicts between parallel test runs.
fn unique_email(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("test+{prefix}{nanos}@example.com")
}

/// Deletes a test user by email to clean up after each test.
async fn cleanup(pool: &PgPool, email: &str) {
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .ok();
}

/// Builds a JSON body for signup/login requests.
fn auth_body(email: &str, password: &str) -> Body {
    Body::from(json!({"email": email, "password": password}).to_string())
}

// ─── Signup tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn signup_success() {
    let (app, pool) = test_app().await;
    let email = unique_email("signup_ok");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn signup_duplicate_email() {
    let (app, pool) = test_app().await;
    let email = unique_email("signup_dup");

    // First signup — must succeed
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Second signup with same email — must conflict
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn signup_invalid_payload() {
    let (app, _pool) = test_app().await;

    // Missing password field — Axum will return 422 Unprocessable Entity
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"email":"missing-password@example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn signup_bad_password() {
    let (app, _pool) = test_app().await;
    let email = unique_email("signup_badpw");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "weak"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ─── Login tests ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn login_success() {
    let (app, pool) = test_app().await;
    let email = unique_email("login_ok");

    // Create user first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_wrong_password() {
    let (app, pool) = test_app().await;
    let email = unique_email("login_wrongpw");

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(auth_body(&email, "WrongPass_1"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_unknown_email() {
    let (app, _pool) = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(auth_body("nobody_unique_xyz@example.com", "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ─── Logout tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn logout_unauthenticated() {
    let (app, _pool) = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_success() {
    let (app, pool) = test_app().await;
    let email = unique_email("logout_ok");

    // Signup to get a Set-Cookie header
    let signup_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(auth_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Extract the encrypted Set-Cookie value to forward on logout
    let set_cookie = signup_resp
        .headers()
        .get("set-cookie")
        .expect("signup must set a cookie")
        .to_str()
        .unwrap();

    // Extract "auth-token=<value>" — everything before the first ";"
    let cookie_header = set_cookie.split(';').next().unwrap().trim().to_string();

    // Forward the cookie name=value pair — same app instance means same Key, so decryption succeeds
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/logout")
                .header("cookie", &cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}
