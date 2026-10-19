//! Plaid integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use super::fixtures::{
    authenticated_cookie, cleanup, test_app, test_pool, unique_email, MOCK_FAIL_500, MOCK_FAIL_BODY,
};

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

    // Pre-clean any leftovers from a previous panicked run (idempotent)
    sqlx::query!(
        "DELETE FROM plaid_links WHERE item_id IN ('test-item-user-a', 'test-item-user-b')"
    )
    .execute(&pool)
    .await
    .ok();

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

    assert!(
        ids.contains(&"test-tx-user-a"),
        "must include user A's transaction"
    );
    assert!(
        !ids.contains(&"test-tx-user-b"),
        "must NOT include user B's transaction"
    );

    // Clean up seeded rows (CASCADE handles child rows)
    sqlx::query!("DELETE FROM plaid_transactions WHERE transaction_id IN ('test-tx-user-a', 'test-tx-user-b')")
        .execute(&pool)
        .await
        .ok();
    sqlx::query!(
        "DELETE FROM plaid_accounts WHERE account_id IN ('test-acct-user-a', 'test-acct-user-b')"
    )
    .execute(&pool)
    .await
    .ok();
    sqlx::query!(
        "DELETE FROM plaid_links WHERE item_id IN ('test-item-user-a', 'test-item-user-b')"
    )
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

// ─── Mock-Plaid tests (local mock server, no real credentials) ───────────────

/// Sends an authenticated request and returns `(status, json body)`.
async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", cookie);
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
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn plaid_full_flow_with_mock() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_flow");

    // Pre-clean any leftovers from a previous panicked run (the mock uses fixed IDs)
    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-mock'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, json) = call(&app, "POST", "/api/plaid/link-token", &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["link_token"], "link-mock");

    let (status, json) = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(r#"{"public_token":"public-mock"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["item_id"], "item-mock");

    // Exchanging the same item twice violates the UNIQUE constraint
    let (status, _) = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(r#"{"public_token":"public-mock"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let (status, json) = call(&app, "GET", "/api/plaid/accounts", &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["accounts"][0]["account_id"], "acct-mock");

    let (status, json) = call(&app, "POST", "/api/plaid/transactions/sync", &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["added"], 2);
    assert_eq!(json["modified"], 1);
    assert_eq!(json["removed"], 1);

    let (status, json) = call(
        &app,
        "GET",
        "/api/plaid/transactions?limit=500&offset=0",
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["limit"], 200);
    assert_eq!(json["total"], 1);
    assert_eq!(json["transactions"][0]["transaction_id"], "tx1-mock");

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn get_transactions_negative_params_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_tx_neg");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "GET",
        "/api/plaid/transactions?limit=-1",
        &cookie,
        None,
    )
    .await;
    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Every Plaid-backed endpoint that takes a token must surface upstream
/// failures as HTTP 500. `marker` is a `MOCK_FAIL_*` value that tells the mock to fail.
async fn assert_plaid_failures_are_500(marker: &str, prefix: &str) {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email(prefix);
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Seed a link whose access token makes the mock fail on accounts/sync
    let user_id: i32 = sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", email)
        .fetch_one(&pool)
        .await
        .unwrap();
    let item_id = format!("item-{prefix}");
    let access_token = format!("access-{marker}");
    sqlx::query!("DELETE FROM plaid_links WHERE item_id = $1", item_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query!(
        "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)",
        user_id,
        item_id,
        access_token
    )
    .execute(&pool)
    .await
    .unwrap();

    let exchange_body = format!(r#"{{"public_token":"{marker}"}}"#);
    let exchange = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(&exchange_body),
    )
    .await;
    let accounts = call(&app, "GET", "/api/plaid/accounts", &cookie, None).await;
    let sync = call(&app, "POST", "/api/plaid/transactions/sync", &cookie, None).await;

    cleanup(&pool, &email).await;

    for (name, (status, _)) in [
        ("exchange-token", exchange),
        ("accounts", accounts),
        ("sync", sync),
    ] {
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{name}");
    }
}

#[tokio::test]
async fn plaid_upstream_error_status_returns_500() {
    assert_plaid_failures_are_500(MOCK_FAIL_500, "plaid_up500").await;
}

#[tokio::test]
async fn plaid_unparseable_response_returns_500() {
    assert_plaid_failures_are_500(MOCK_FAIL_BODY, "plaid_badbody").await;
}
