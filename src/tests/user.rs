//! Login, logout, and profile integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use super::fixtures::{authenticated_cookie, cleanup, test_app, test_pool, unique_email};

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

// ─── Profile tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_profile_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/profile")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_profile_returns_defaults() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_defaults");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/profile")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    cleanup(&pool, &email).await;

    assert_eq!(json["first_name"], "Alice");
    assert_eq!(json["last_name"], "Smith");
    assert_eq!(json["email_notifications"], false);
    assert_eq!(json["phone_number"], serde_json::Value::Null);
    assert_eq!(json["sms_notifications"], false);
}

#[tokio::test]
async fn put_profile_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"email_notifications": true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_profile_opt_in_email() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_email_optin");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let put_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(r#"{"email_notifications": true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(put_resp.status(), StatusCode::OK);

    let get_resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/profile")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    cleanup(&pool, &email).await;
    assert_eq!(json["email_notifications"], true);
}

#[tokio::test]
async fn put_profile_set_phone_and_sms() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_sms");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({"phone_number": "+15551234567", "sms_notifications": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn put_profile_sms_without_phone_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_sms_nophone");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(r#"{"sms_notifications": true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_profile_invalid_phone_format_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_badphone");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(r#"{"phone_number": "5551234567"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_profile_clear_phone_disables_sms() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_clear_phone");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Set phone and enable SMS
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({"phone_number": "+15551234567", "sms_notifications": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Clear phone number
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(r#"{"phone_number": ""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Confirm sms_notifications is false and phone is null
    let get_resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/profile")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    cleanup(&pool, &email).await;
    assert_eq!(json["phone_number"], serde_json::Value::Null);
    assert_eq!(json["sms_notifications"], false);
}

#[tokio::test]
async fn put_profile_updates_account_details() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_edit");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Update both names while keeping the email unchanged.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({
                        "first_name": "Bob",
                        "last_name": "Jones"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Retrieve the profile to verify the changes persisted.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/profile")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let profile: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(profile["first_name"], "Bob");
    assert_eq!(profile["last_name"], "Jones");
    assert_eq!(profile["email"], email);
    assert_eq!(profile["email_notifications"], false);
    assert_eq!(profile["sms_notifications"], false);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn put_profile_updates_email() {
    let app = test_app().await;
    let pool = test_pool().await;

    let old_email = unique_email("profile_old_email");
    let new_email = unique_email("profile_new_email");
    let cookie = authenticated_cookie(app.clone(), &old_email).await;

    // Change the user's email address.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(json!({"email": &new_email}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify the new email was saved.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/user/profile")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let profile: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(profile["email"], new_email);

    cleanup(&pool, &old_email).await;
    cleanup(&pool, &new_email).await;
}

#[tokio::test]
async fn put_profile_duplicate_email_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;

    let first_email = unique_email("profile_first");
    let second_email = unique_email("profile_second");

    let cookie = authenticated_cookie(app.clone(), &first_email).await;

    // Create another user whose email is already taken.
    let other_cookie = authenticated_cookie(app.clone(), &second_email).await;

    // Attempt to change the first user's email to the second user's.
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(json!({"email": &second_email}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);

    cleanup(&pool, &first_email).await;
    cleanup(&pool, &second_email).await;

    // The second user's cookie was only needed to create their account.
    drop(other_cookie);
}

#[tokio::test]
async fn put_profile_invalid_email_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_invalid_email");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/profile")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(json!({"email": "not-an-email"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn put_profile_empty_name_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("profile_empty_name");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    for payload in [json!({"first_name": "   "}), json!({"last_name": ""})] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/user/profile")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn change_password_success() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("password_success");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    // The test fixture creates users with password Secret_123.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/password")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({
                        "current_password": "Secret_123",
                        "new_password": "NewSecret_456"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // The old password must no longer work.
    let old_login = app
        .clone()
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

    assert_eq!(old_login.status(), StatusCode::BAD_REQUEST);

    // The new password must work.
    let new_login = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": &email,
                        "password": "NewSecret_456"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(new_login.status(), StatusCode::OK);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn change_password_wrong_current_password() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("password_wrong");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/password")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({
                        "current_password": "WrongPassword_123",
                        "new_password": "NewSecret_456"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn change_password_weak_password_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("password_weak");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/user/password")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({
                        "current_password": "Secret_123",
                        "new_password": "weak"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}
