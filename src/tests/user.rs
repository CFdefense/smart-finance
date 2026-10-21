//! Login and logout integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use super::fixtures::{cleanup, test_app, test_pool, unique_email};

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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "Secret_123"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "Secret_123"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "Secret_123"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "weak"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "Secret_123"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"email": &email, "password": "Secret_123"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "Secret_123"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"email": &email, "password": "WrongPass_1"}).to_string(),
                ))
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
                .body(Body::from(
                    json!({"email": "nobody_unique_xyz@example.com", "password": "Secret_123"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_suspended_account() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("login_suspended");

    // First, create an active account.
    let signup_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "first_name": "Test",
                        "last_name": "User",
                        "email": &email,
                        "password": "Secret_123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(signup_response.status(), StatusCode::OK);

    // Simulate an administrator suspending the account.
    sqlx::query("UPDATE users SET account_status = 'suspended' WHERE email = $1")
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap();

    // Attempt to log in using the correct password.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": &email,
                        "password": "Secret_123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn suspended_account_cannot_use_existing_session() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("suspended_session");

    // Create an account and receive an authentication cookie.
    let signup_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "first_name": "Test",
                        "last_name": "User",
                        "email": &email,
                        "password": "Secret_123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(signup_response.status(), StatusCode::OK);

    // Save the cookie issued during signup.
    let cookie = signup_response
        .headers()
        .get("set-cookie")
        .expect("signup should set a cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // Confirm the session works while the account is active.
    let active_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(active_response.status(), StatusCode::OK);

    // Simulate an administrator suspending the account.
    sqlx::query("UPDATE users SET account_status = 'suspended' WHERE email = $1")
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap();

    // Attempt to reuse the same cookie after suspension.
    let suspended_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;

    assert_eq!(suspended_response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabled_account_cannot_use_existing_session() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("disabled_session");

    // Create an account and receive an authentication cookie.
    let signup_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "first_name": "Test",
                        "last_name": "User",
                        "email": &email,
                        "password": "Secret_123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(signup_response.status(), StatusCode::OK);

    // Save the cookie issued during signup.
    let cookie = signup_response
        .headers()
        .get("set-cookie")
        .expect("signup should set a cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // Confirm the session works while the account is active.
    let active_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(active_response.status(), StatusCode::OK);

    // Simulate a disabling of the account.
    sqlx::query("UPDATE users SET account_status = 'disabled' WHERE email = $1")
        .bind(&email)
        .execute(&pool)
        .await
        .unwrap();

    // Attempt to reuse the same cookie after suspension.
    let disabled_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/me")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;

    assert_eq!(disabled_response.status(), StatusCode::UNAUTHORIZED);
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
                .body(Body::from(
                    json!({"first_name": "Alice", "last_name": "Smith", "email": &email, "password": "Secret_123"}).to_string(),
                ))
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
