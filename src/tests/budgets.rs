//! Integration tests for budget endpoints (`/api/budgets`).

use axum::http::StatusCode;
use serde_json::json;

use super::fixtures::{authenticated_cookie, call, cleanup, test_app, test_pool, unique_email};

// ─── Auth-gate tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn list_budgets_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/budgets", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_budget_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "POST",
        "/api/budgets",
        "",
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_budget_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "GET", "/api/budgets/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_budget_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(
        &app,
        "PUT",
        "/api/budgets/1",
        "",
        Some(json!({"monthly_limit": 500.0})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_budget_unauthenticated() {
    let app = test_app().await;
    let (status, _) = call(&app, "DELETE", "/api/budgets/1", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─── Create budget ────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_budget_succeeds() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_create_ok");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Eating out", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn create_budget_defaults_active_to_true() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_create_active");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Groceries", "category": "GROCERIES", "monthly_limit": 300.0})),
    )
    .await;

    let (_, json) = call(&app, "GET", "/api/budgets", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(json["budgets"][0]["active"], true);
}

#[tokio::test]
async fn create_budget_invalid_limit_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_create_bad_limit");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 0.0})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_budget_empty_name_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_create_bad_name");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "  ", "category": "FOOD_DINING", "monthly_limit": 100.0})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_budget_duplicate_category_is_conflict() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_create_dup");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;

    let (status, _) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food again", "category": "FOOD_DINING", "monthly_limit": 200.0})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

// ─── List budgets ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_budgets_empty_for_new_user() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_list_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, json) = call(&app, "GET", "/api/budgets", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["budgets"], json!([]));
}

#[tokio::test]
async fn list_budgets_returns_created_budget() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_list_created");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Eating out", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;

    let (status, json) = call(&app, "GET", "/api/budgets", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["budgets"].as_array().unwrap().len(), 1);
    assert_eq!(json["budgets"][0]["name"], "Eating out");
    assert_eq!(json["budgets"][0]["category"], "FOOD_DINING");
    assert_eq!(json["budgets"][0]["monthly_limit"], 400.0);
    assert_eq!(json["budgets"][0]["spent"], 0.0);
}

#[tokio::test]
async fn list_budgets_includes_month_label() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_list_month");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, json) = call(&app, "GET", "/api/budgets", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    // Month must be present and match YYYY-MM format
    let month = json["month"].as_str().unwrap_or("");
    assert_eq!(month.len(), 7);
    assert_eq!(month.as_bytes()[4], b'-');
}

#[tokio::test]
async fn list_budgets_invalid_month_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_list_bad_month");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "GET", "/api/budgets?month=not-a-month", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── Get budget ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_budget_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_get_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "GET", "/api/budgets/999999", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_budget_returns_correct_budget() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_get_ok");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Transport", "category": "TRANSPORTATION", "monthly_limit": 150.0})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, budget) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(budget["id"], id);
    assert_eq!(budget["name"], "Transport");
    assert_eq!(budget["category"], "TRANSPORTATION");
}

#[tokio::test]
async fn get_budget_owned_by_other_user_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("budget_isolation_a");
    let email_b = unique_email("budget_isolation_b");

    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie_a,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;

    let (_, list) = call(&app, "GET", "/api/budgets", &cookie_a, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, _) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie_b, None).await;

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Update budget ────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_budget_changes_limit() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_update_limit");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/budgets/{id}"),
        &cookie,
        Some(json!({"monthly_limit": 500.0})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["monthly_limit"], 500.0);
}

#[tokio::test]
async fn update_budget_active_flag() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_update_active");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/budgets/{id}"),
        &cookie,
        Some(json!({"active": false})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["active"], false);
}

#[tokio::test]
async fn update_budget_empty_body_rejected() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_update_empty");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/budgets/{id}"),
        &cookie,
        Some(json!({})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_budget_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_update_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(
        &app,
        "PUT",
        "/api/budgets/999999",
        &cookie,
        Some(json!({"monthly_limit": 100.0})),
    )
    .await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Delete budget ────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_budget_removes_it() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_delete_ok");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, _) = call(&app, "DELETE", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, list_after) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    cleanup(&pool, &email).await;
    assert_eq!(list_after["budgets"], json!([]));
}

#[tokio::test]
async fn delete_budget_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("budget_delete_notfound");

    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, _) = call(&app, "DELETE", "/api/budgets/999999", &cookie, None).await;

    cleanup(&pool, &email).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_budget_owned_by_other_user_is_not_found() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("budget_del_isolation_a");
    let email_b = unique_email("budget_del_isolation_b");

    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie_a,
        Some(json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 400.0})),
    )
    .await;
    let (_, list) = call(&app, "GET", "/api/budgets", &cookie_a, None).await;
    let id = list["budgets"][0]["id"].as_i64().unwrap();

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/budgets/{id}"),
        &cookie_b,
        None,
    )
    .await;

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
