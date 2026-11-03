//! Plaid integration tests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use super::fixtures::{
    authenticated_cookie, call, cleanup, test_app, test_pool, unique_email, MOCK_FAIL_500,
    MOCK_FAIL_BODY,
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
        Some(json!({"public_token": "public-mock"})),
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
        Some(json!({"public_token": "public-mock"})),
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

    let exchange = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(json!({"public_token": marker})),
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

#[tokio::test]
async fn remove_item_unauthenticated() {
    // Create our test application.
    let app = test_app().await;

    // Attempt to remove a bank connection without an auth cookie.
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/plaid/items/test-item")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The backend must reject the unauthenticated request.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn remove_item_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("plaid_remove_missing");

    // Create a test user and obtain their authentication cookie.
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Request removal of a bank connection that does not exist.
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/plaid/items/nonexistent-item")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Clean up the test user.
    cleanup(&pool, &email).await;

    // The backend should return 404 rather than contacting Plaid.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn remove_item_cannot_delete_another_users_connection() {
    let app = test_app().await;
    let pool = test_pool().await;

    let email_a = unique_email("remove_owner_a");
    let email_b = unique_email("remove_owner_b");

    // Create two different authenticated users.
    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    // User B connects a bank through our mocked Plaid API.
    let (exchange_status, _) = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie_b,
        Some(json!({"public_token": format!("public-{email_b}")})),
    )
    .await;

    assert_eq!(exchange_status, StatusCode::OK);

    // Our mock derives the Item ID from the public token.
    let item_id = format!("item-{email_b}");

    // User A attempts to remove User B's connection.
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/plaid/items/{item_id}"),
        &cookie_a,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);

    // Verify the connection still exists in PostgreSQL.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plaid_links WHERE item_id = $1")
        .bind(&item_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1);

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
}

#[tokio::test]
async fn remove_item_deletes_owned_connection() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("remove_success");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Connect a test bank using the existing exchange endpoint.
    let (exchange_status, _) = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(json!({"public_token": format!("public-{email}")})),
    )
    .await;

    assert_eq!(exchange_status, StatusCode::OK);

    let item_id = format!("item-{email}");

    // Remove the bank through our new endpoint.
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/plaid/items/{item_id}"),
        &cookie,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    // The connection should no longer exist locally.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plaid_links WHERE item_id = $1")
        .bind(&item_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 0);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn remove_item_plaid_failure_preserves_connection() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("remove_failure");

    // Create an authenticated test user.
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // The mock generates an access token ending in "fail-500".
    // This causes our fake Plaid API to return HTTP 500.
    let public_token = format!("public-{email}");
    let item_id = format!("item-{email}");

    // Connect the test bank through our existing exchange endpoint.
    let (exchange_status, _) = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(json!({ "public_token": public_token })),
    )
    .await;

    assert_eq!(exchange_status, StatusCode::OK);

    // The connection was created successfully with a normal token.
    // Now change its stored access token to trigger a Plaid failure
    // specifically when we attempt to disconnect it.
    sqlx::query("UPDATE plaid_links SET access_token = $1 WHERE item_id = $2")
        .bind("access-fail-500")
        .bind(&item_id)
        .execute(&pool)
        .await
        .unwrap();

    // Attempt removal. The mocked Plaid API will reject it.
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/plaid/items/{item_id}"),
        &cookie,
        None,
    )
    .await;

    // Our backend must report an error instead of claiming success.
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    // Most importantly, the connection must still exist in PostgreSQL.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plaid_links WHERE item_id = $1")
        .bind(&item_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1);

    // Clean up the test user's data.
    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn remove_item_cascades_to_accounts_and_transactions() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("remove_cascade");

    // Create a user and obtain an authenticated session.
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Connect a simulated bank using our existing Plaid mock.
    let (status, response) = call(
        &app,
        "POST",
        "/api/plaid/exchange-token",
        &cookie,
        Some(json!({
            "public_token": format!("public-{email}")
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let item_id = response["item_id"].as_str().unwrap().to_string();
    let account_id = format!("acct-{email}");
    let transaction_id = format!("tx1-{email}");

    // Retrieve accounts so the backend saves them in PostgreSQL.
    let (status, _) = call(&app, "GET", "/api/plaid/accounts", &cookie, None).await;

    assert_eq!(status, StatusCode::OK);

    // Synchronize transactions so the account has transaction history.
    let (status, _) = call(&app, "POST", "/api/plaid/transactions/sync", &cookie, None).await;

    assert_eq!(status, StatusCode::OK);

    // Confirm the account and transaction exist before removal.
    let account_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plaid_accounts WHERE account_id = $1")
            .bind(&account_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let transaction_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plaid_transactions WHERE transaction_id = $1")
            .bind(&transaction_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(account_count, 1);
    assert_eq!(transaction_count, 1);

    // Disconnect the bank through our actual DELETE endpoint.
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/plaid/items/{item_id}"),
        &cookie,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify that PostgreSQL's ON DELETE CASCADE removed the
    // connected account and its associated transaction history.
    let remaining_accounts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plaid_accounts WHERE account_id = $1")
            .bind(&account_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let remaining_transactions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plaid_transactions WHERE transaction_id = $1")
            .bind(&transaction_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let remaining_cursors: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plaid_sync_cursors WHERE item_id = $1")
            .bind(&item_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(remaining_accounts, 0);
    assert_eq!(remaining_transactions, 0);
    assert_eq!(remaining_cursors, 0);

    // Remove the test user and any remaining test data.
    cleanup(&pool, &email).await;
}
