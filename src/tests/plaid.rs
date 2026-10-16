//! Plaid integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use super::fixtures::{authenticated_cookie, cleanup, test_app, test_pool, unique_email};

// ─── Auth-gate tests (no Plaid credentials required) ─────────────────────────

#[tokio::test]
async fn link_token_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plaid/link-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn exchange_token_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plaid/exchange-token")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"public_token":"tok"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn exchange_token_missing_body() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_exchg_nobody");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plaid/exchange-token")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_accounts_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/plaid/accounts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sync_transactions_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plaid/transactions/sync")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_transactions_unauthenticated() {
    let app = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/plaid/transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_transactions_authenticated_empty() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_tx_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/plaid/transactions")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["transactions"], serde_json::json!([]));
    assert_eq!(json["limit"], 50);
    assert_eq!(json["offset"], 0);
}

#[tokio::test]
async fn get_accounts_authenticated_empty() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_acct_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/plaid/accounts")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["accounts"], serde_json::json!([]));
}

#[tokio::test]
async fn sync_transactions_authenticated_no_items() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_sync_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plaid/transactions/sync")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["added"], 0);
    assert_eq!(json["modified"], 0);
    assert_eq!(json["removed"], 0);
    assert_eq!(json["has_more"], false);
}

// ─── Sandbox tests (require PLAID_CLIENT_ID in env) ──────────────────────────

#[tokio::test]
async fn link_token_authenticated() {
    // Skip if Plaid credentials are not configured
    if std::env::var("PLAID_CLIENT_ID").is_err() {
        return;
    }

    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_link_ok");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plaid/link-token")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = json["link_token"].as_str().unwrap_or("");
    assert!(!token.is_empty(), "link_token must be non-empty");
}
