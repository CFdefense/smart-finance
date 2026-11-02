//! Integration tests for goal endpoints (`/api/goals`).

use axum::http::StatusCode;
use serde_json::json;

use super::fixtures::{authenticated_cookie, call, cleanup, test_app, test_pool, unique_email};

/// Links a mock Plaid account and returns its `account_id` (`"acct-<suffix>"`).
///
/// Exchange populates `plaid_links`; the subsequent accounts fetch populates
/// `plaid_accounts`, which is required before the account can be used in a goal.
async fn link_account(app: &axum::Router, cookie: &str, suffix: &str) -> String {
    let (status, _) = call(
        app,
        "POST",
        "/api/plaid/exchange-token",
        cookie,
        Some(json!({"public_token": format!("public-{suffix}")})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "exchange-token failed for {suffix}");

    let (status, _) = call(app, "GET", "/api/plaid/accounts", cookie, None).await;
    assert_eq!(status, StatusCode::OK, "get-accounts failed for {suffix}");

    format!("acct-{suffix}")
}

// ─── Auth-gate tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn list_goals_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/goals", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_goal_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/goals",
        "",
        Some(json!({"name": "Fund", "target_amount": 5000.0, "account_id": "acct-x"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_goal_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/goals/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_goal_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "PUT",
        "/api/goals/1",
        "",
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_goal_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "DELETE", "/api/goals/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── Create goal ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_goal_succeeds() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_create_ok");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_cok'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_cok").await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({
            "name": "Emergency fund",
            "target_amount": 5000.0,
            "account_id": account_id
        })),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn create_goal_unknown_account_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_create_noacct");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({
            "name": "Emergency fund",
            "target_amount": 5000.0,
            "account_id": "nonexistent-account"
        })),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_goal_invalid_name_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_create_badname");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({
            "name": "  ",
            "target_amount": 5000.0,
            "account_id": "acct-x"
        })),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_goal_zero_target_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_create_zerotarget");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({
            "name": "Fund",
            "target_amount": 0.0,
            "account_id": "acct-x"
        })),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_goal_target_not_above_baseline_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_create_lowtarget");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_clt'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_clt").await;

    // The mock account reports current_balance = 100.0; baseline defaults to that.
    // A target of 50 is below the baseline, so it must be rejected.
    let (status, _) = call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({
            "name": "Fund",
            "target_amount": 50.0,
            "account_id": account_id
        })),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── List goals ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_goals_empty_for_new_user() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_list_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, json) = call(&app, "GET", "/api/goals", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["goals"], json!([]));
}

#[tokio::test]
async fn list_goals_returns_created_goal() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_list_created");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_lc'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_lc").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({
            "name": "Holiday savings",
            "target_amount": 2000.0,
            "account_id": account_id
        })),
    )
    .await;

    let (status, json) = call(&app, "GET", "/api/goals", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["goals"].as_array().unwrap().len(), 1);
    assert_eq!(json["goals"][0]["name"], "Holiday savings");
    assert_eq!(json["goals"][0]["target_amount"], 2000.0);
    assert_eq!(json["goals"][0]["status"], "ACTIVE");
}

#[tokio::test]
async fn list_goals_filter_by_invalid_status_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_list_badstatus");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "GET", "/api/goals?status=INVALID", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_goals_filter_by_archived_status() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_list_archived");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_la'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_la").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({"name": "Active goal", "target_amount": 2000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    // Archive the goal
    call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(json!({"archived": true})),
    )
    .await;

    let (status, json) = call(&app, "GET", "/api/goals?status=ARCHIVED", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["goals"].as_array().unwrap().len(), 1);
    assert_eq!(json["goals"][0]["status"], "ARCHIVED");
}

// ─── Get goal ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_goal_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_get_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "GET", "/api/goals/999999", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_goal_returns_correct_goal() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_get_ok");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_gok'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_gok").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({"name": "Rainy day", "target_amount": 3000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, goal) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(goal["id"], id);
    assert_eq!(goal["name"], "Rainy day");
    assert_eq!(goal["target_amount"], 3000.0);
}

#[tokio::test]
async fn get_goal_owned_by_other_user_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("goal_isolation_a");
    let email_b = unique_email("goal_isolation_b");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_isa'")
        .execute(&pool)
        .await
        .ok();

    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    let account_id = link_account(&app, &cookie_a, "goal_isa").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie_a,
        Some(json!({"name": "User A goal", "target_amount": 1000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie_a, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, _) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie_b, None).await;

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Update goal ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_goal_changes_name() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_update_name");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_un'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_un").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({"name": "Old name", "target_amount": 2000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(json!({"name": "New name"})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "New name");
}

#[tokio::test]
async fn update_goal_archives_it() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_update_archive");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_ua'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_ua").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({"name": "Fund", "target_amount": 2000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(json!({"archived": true})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["status"], "ARCHIVED");
}

#[tokio::test]
async fn update_goal_empty_body_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_update_empty");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_ue'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_ue").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({"name": "Fund", "target_amount": 2000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(json!({})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_goal_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_update_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "PUT",
        "/api/goals/999999",
        &cookie,
        Some(json!({"archived": true})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Delete goal ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_goal_removes_it() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_delete_ok");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_dok'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "goal_dok").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie,
        Some(json!({"name": "Fund", "target_amount": 2000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, _) = call(&app, "DELETE", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list_after) = call(&app, "GET", "/api/goals", &cookie, None).await;
    cleanup(&pool, &email).await;
    assert_eq!(list_after["goals"], json!([]));
}

#[tokio::test]
async fn delete_goal_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_delete_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "DELETE", "/api/goals/999999", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_goal_owned_by_other_user_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("goal_del_isolation_a");
    let email_b = unique_email("goal_del_isolation_b");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-goal_dib'")
        .execute(&pool)
        .await
        .ok();

    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    let account_id = link_account(&app, &cookie_a, "goal_dib").await;

    call(
        &app,
        "POST",
        "/api/goals",
        &cookie_a,
        Some(json!({"name": "User A goal", "target_amount": 1000.0, "account_id": account_id})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/goals", &cookie_a, None).await;
    let id = list["goals"][0]["id"].as_i64().unwrap();

    let (status, _) = call(&app, "DELETE", &format!("/api/goals/{id}"), &cookie_b, None).await;

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
