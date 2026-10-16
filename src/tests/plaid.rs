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

// ─── Data-isolation test ──────────────────────────────────────────────────────

#[tokio::test]
async fn get_transactions_data_isolation() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("plaid_iso_a");
    let email_b = unique_email("plaid_iso_b");

    // Create user A and get their auth cookie
    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;

    // Create user B (no cookie needed for the actual assertion)
    authenticated_cookie(app.clone(), &email_b).await;

    // Resolve DB ids for both users
    let user_a_id: i32 = sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", email_a)
        .fetch_one(&pool)
        .await
        .unwrap();
    let user_b_id: i32 = sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", email_b)
        .fetch_one(&pool)
        .await
        .unwrap();

    // Seed plaid_links
    sqlx::query!(
        "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)",
        user_a_id,
        "test-item-user-a",
        "access-test-user-a"
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)",
        user_b_id,
        "test-item-user-b",
        "access-test-user-b"
    )
    .execute(&pool)
    .await
    .unwrap();

    // Seed plaid_accounts
    sqlx::query!(
        r#"
        INSERT INTO plaid_accounts
            (account_id, item_id, name, account_type, updated_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
        "test-acct-user-a",
        "test-item-user-a",
        "Test Account A",
        "depository"
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO plaid_accounts
            (account_id, item_id, name, account_type, updated_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
        "test-acct-user-b",
        "test-item-user-b",
        "Test Account B",
        "depository"
    )
    .execute(&pool)
    .await
    .unwrap();

    // Seed plaid_transactions
    sqlx::query!(
        r#"
        INSERT INTO plaid_transactions
            (transaction_id, account_id, name, amount, date, pending, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        "#,
        "test-tx-user-a",
        "test-acct-user-a",
        "Transaction A",
        10.00_f64,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        false
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO plaid_transactions
            (transaction_id, account_id, name, amount, date, pending, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
        "#,
        "test-tx-user-b",
        "test-acct-user-b",
        "Transaction B",
        20.00_f64,
        chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
        false
    )
    .execute(&pool)
    .await
    .unwrap();

    // Call GET /api/plaid/transactions as user A
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/plaid/transactions")
                .header("cookie", &cookie_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let txs = json["transactions"].as_array().unwrap();
    let ids: Vec<&str> = txs
        .iter()
        .filter_map(|t| t["transaction_id"].as_str())
        .collect();

    assert!(ids.contains(&"test-tx-user-a"), "must include user A's transaction");
    assert!(!ids.contains(&"test-tx-user-b"), "must NOT include user B's transaction");

    // Clean up seeded rows (CASCADE handles child rows)
    sqlx::query!("DELETE FROM plaid_transactions WHERE transaction_id IN ('test-tx-user-a', 'test-tx-user-b')")
        .execute(&pool)
        .await
        .ok();
    sqlx::query!("DELETE FROM plaid_accounts WHERE account_id IN ('test-acct-user-a', 'test-acct-user-b')")
        .execute(&pool)
        .await
        .ok();
    sqlx::query!("DELETE FROM plaid_links WHERE item_id IN ('test-item-user-a', 'test-item-user-b')")
        .execute(&pool)
        .await
        .ok();
    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
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
