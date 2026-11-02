//! Integration tests for rule endpoints (`/api/rules`).

use axum::http::StatusCode;
use serde_json::json;

use super::fixtures::{authenticated_cookie, call, cleanup, test_app, test_pool, unique_email};

/// Links a mock Plaid account and returns its `account_id` (`"acct-<suffix>"`).
///
/// The mock server derives account IDs from the public token suffix, so each
/// unique `suffix` yields an isolated account.
async fn link_account(app: &axum::Router, cookie: &str, suffix: &str) -> String {
    let public_token = format!("public-{suffix}");
    let (status, _) = call(
        app,
        "POST",
        "/api/plaid/exchange-token",
        cookie,
        Some(json!({"public_token": public_token})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "exchange-token failed for suffix={suffix}"
    );
    // Fetching accounts persists them into plaid_accounts, which is required
    // before they can be used as rule account_ids (FK constraint).
    let (status, _) = call(app, "GET", "/api/plaid/accounts", cookie, None).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "get accounts failed for suffix={suffix}"
    );
    format!("acct-{suffix}")
}

// ─── Auth-gate tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn list_rules_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/rules", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_rule_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/rules",
        "",
        Some(json!({"description": "no food"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_rule_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/rules/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_rule_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "PUT",
        "/api/rules/1",
        "",
        Some(json!({"description": "no food"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_rule_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "DELETE", "/api/rules/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── Create rule ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_rule_no_accounts() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_create_no_acct");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "no food spending"})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn create_rule_with_account() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_create_with_acct");

    // Clean up any leftover mock item from a prior panicked run
    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-rules_cwa'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "rules_cwa").await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "no food", "account_ids": [account_id]})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn create_rule_empty_description_is_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_create_empty_desc");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "   "})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_rule_unknown_account_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_create_unknown_acct");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "no food", "account_ids": ["nonexistent-account-id"]})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_rule_duplicate_accounts_are_accepted() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_create_dup_acct");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-rules_cda'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "rules_cda").await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "no food", "account_ids": [&account_id, &account_id]})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::CREATED);
}

// ─── List rules ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_rules_empty_for_new_user() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_list_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, json) = call(&app, "GET", "/api/rules", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["rules"], json!([]));
}

#[tokio::test]
async fn list_rules_returns_created_rule() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_list_created");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "no food", "enabled": true})),
    )
    .await;

    let (status, json) = call(&app, "GET", "/api/rules", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["rules"].as_array().unwrap().len(), 1);
    assert_eq!(json["rules"][0]["description"], "no food");
    assert_eq!(json["rules"][0]["enabled"], true);
    assert_eq!(json["rules"][0]["account_ids"], json!([]));
}

#[tokio::test]
async fn list_rules_filter_by_enabled() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_list_filter_enabled");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "enabled rule", "enabled": true})),
    )
    .await;
    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "disabled rule", "enabled": false})),
    )
    .await;

    let (status, json) = call(&app, "GET", "/api/rules?enabled=true", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    let rules = json["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["description"], "enabled rule");
}

#[tokio::test]
async fn list_rules_filter_by_account_includes_global_rules() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_list_filter_acct");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-rules_lfa'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "rules_lfa").await;

    // Global rule (no account scope) — should always appear
    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "global rule"})),
    )
    .await;
    // Scoped rule — should appear when filtering by this account
    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "scoped rule", "account_ids": [&account_id]})),
    )
    .await;

    let uri = format!("/api/rules?account_id={account_id}");
    let (status, json) = call(&app, "GET", &uri, &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    let rules = json["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 2, "both global and scoped rules must appear");
}

// ─── Get rule ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_rule_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_get_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "GET", "/api/rules/999999", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_rule_returns_correct_rule() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_get_ok");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (_, list_before) = call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "no coffee"})),
    )
    .await;
    let _ = list_before; // create returns 201 with no body

    let (_, json) = call(&app, "GET", "/api/rules", &cookie, None).await;
    let id = json["rules"][0]["id"].as_i64().unwrap();

    let (status, rule) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rule["id"], id);
    assert_eq!(rule["description"], "no coffee");
}

#[tokio::test]
async fn get_rule_owned_by_other_user_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("rules_isolation_a");
    let email_b = unique_email("rules_isolation_b");

    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    // User A creates a rule
    call(
        &app,
        "POST",
        "/api/rules",
        &cookie_a,
        Some(json!({"description": "user A rule"})),
    )
    .await;

    let (_, json) = call(&app, "GET", "/api/rules", &cookie_a, None).await;
    let id = json["rules"][0]["id"].as_i64().unwrap();

    // User B tries to fetch it
    let (status, _) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie_b, None).await;

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Update rule ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_rule_replaces_description() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_update_desc");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "original"})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/rules", &cookie, None).await;
    let id = list["rules"][0]["id"].as_i64().unwrap();

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/rules/{id}"),
        &cookie,
        Some(json!({"description": "updated"})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["description"], "updated");
}

#[tokio::test]
async fn update_rule_enabled_flag_is_preserved_when_omitted() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_update_enabled_preserved");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "rule", "enabled": false})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/rules", &cookie, None).await;
    let id = list["rules"][0]["id"].as_i64().unwrap();

    // Update with no `enabled` field — the current value should be kept
    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/rules/{id}"),
        &cookie,
        Some(json!({"description": "updated rule"})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["enabled"], false);
}

#[tokio::test]
async fn update_rule_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_update_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "PUT",
        "/api/rules/999999",
        &cookie,
        Some(json!({"description": "ghost"})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_rule_replaces_account_scope() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_update_acct_scope");

    sqlx::query!("DELETE FROM plaid_links WHERE item_id = 'item-rules_uas'")
        .execute(&pool)
        .await
        .ok();

    let cookie = authenticated_cookie(app.clone(), &email).await;
    let account_id = link_account(&app, &cookie, "rules_uas").await;

    // Create a rule scoped to the account
    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "scoped", "account_ids": [&account_id]})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/rules", &cookie, None).await;
    let id = list["rules"][0]["id"].as_i64().unwrap();

    // Update to global (no accounts)
    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/rules/{id}"),
        &cookie,
        Some(json!({"description": "now global"})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["account_ids"], json!([]));
}

// ─── Delete rule ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_rule_removes_it() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_delete_ok");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/rules",
        &cookie,
        Some(json!({"description": "to delete"})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/rules", &cookie, None).await;
    let id = list["rules"][0]["id"].as_i64().unwrap();

    let (status, _) = call(&app, "DELETE", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list_after) = call(&app, "GET", "/api/rules", &cookie, None).await;
    cleanup(&pool, &email).await;
    assert_eq!(list_after["rules"], json!([]));
}

#[tokio::test]
async fn delete_rule_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rules_delete_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "DELETE", "/api/rules/999999", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_rule_owned_by_other_user_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("rules_del_isolation_a");
    let email_b = unique_email("rules_del_isolation_b");

    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    // User A creates a rule
    call(
        &app,
        "POST",
        "/api/rules",
        &cookie_a,
        Some(json!({"description": "user A rule"})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/rules", &cookie_a, None).await;
    let id = list["rules"][0]["id"].as_i64().unwrap();

    // User B tries to delete it
    let (status, _) = call(&app, "DELETE", &format!("/api/rules/{id}"), &cookie_b, None).await;

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
