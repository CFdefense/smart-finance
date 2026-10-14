//! Login and logout integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use super::fixtures::{auth_body, cleanup, test_app, test_pool, unique_email};

// ─── Signup tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn signup_success() {
    let app = test_app().await;
    let pool = test_pool().await;
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
    let app = test_app().await;
    let pool = test_pool().await;
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
    let app = test_app().await;

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
    let app = test_app().await;
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
    let app = test_app().await;
    let pool = test_pool().await;
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
    let app = test_app().await;
    let pool = test_pool().await;
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
    let app = test_app().await;

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
    let app = test_app().await;

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
    let app = test_app().await;
    let pool = test_pool().await;
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
