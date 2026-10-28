//! Budget integration tests.

use axum::http::StatusCode;
use chrono::{Datelike, NaiveDate, Utc};
use serde_json::json;

use super::fixtures::{
    authenticated_cookie, call, cleanup, seed_account, seed_transaction, test_app, test_pool,
    unique_email, user_id,
};

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

fn this_month() -> String {
    format!("{:04}-{:02}", today().year(), today().month())
}

#[tokio::test]
async fn budgets_require_authentication() {
    let app = test_app().await;
    for (method, uri) in [
        ("GET", "/api/budgets"),
        ("POST", "/api/budgets"),
        ("GET", "/api/budgets/1"),
        ("PUT", "/api/budgets/1"),
        ("DELETE", "/api/budgets/1"),
    ] {
        let (status, _) = call(&app, method, uri, "", Some("{}")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn budget_crud_lifecycle() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("bud_crud");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let body = json!({"name": " Eating out ", "category": "FOOD_DINING", "monthly_limit": 400.0});
    let (status, created) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(&body.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["name"], "Eating out");
    assert_eq!(created["category"], "FOOD_DINING");
    assert_eq!(created["active"], true);
    assert_eq!(created["spent"], 0.0);
    assert_eq!(created["remaining"], 400.0);
    assert_eq!(created["over_budget"], false);
    assert_eq!(created["month"], this_month());
    let id = created["id"].as_i64().unwrap();

    let (status, got) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["id"], id);

    let (status, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["budgets"].as_array().unwrap().len(), 1);
    assert_eq!(list["month"], this_month());

    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/budgets/{id}"),
        &cookie,
        Some(r#"{"monthly_limit": 500.0, "name": "Dining", "active": false}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["monthly_limit"], 500.0);
    assert_eq!(updated["name"], "Dining");
    assert_eq!(updated["active"], false);
    assert_eq!(updated["category"], "FOOD_DINING");

    // Partial update leaves other fields alone
    let (_, partial) = call(
        &app,
        "PUT",
        &format!("/api/budgets/{id}"),
        &cookie,
        Some(r#"{"active": true}"#),
    )
    .await;
    assert_eq!(partial["monthly_limit"], 500.0);
    assert_eq!(partial["active"], true);

    let (status, _) = call(&app, "DELETE", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "DELETE", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn budget_validation_and_duplicates() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("bud_val");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    for bad in [
        json!({"name": "", "category": "GROCERIES", "monthly_limit": 10.0}),
        json!({"name": "x", "category": "GROCERIES", "monthly_limit": 0.0}),
        json!({"name": "x", "category": "GROCERIES", "monthly_limit": -5.0}),
        json!({"name": "x".repeat(101), "category": "GROCERIES", "monthly_limit": 10.0}),
    ] {
        let (status, _) = call(
            &app,
            "POST",
            "/api/budgets",
            &cookie,
            Some(&bad.to_string()),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // Unknown category / missing fields are rejected by deserialization
    for bad in [
        json!({"name": "x", "category": "NOPE", "monthly_limit": 10.0}),
        json!({"name": "x", "monthly_limit": 10.0}),
    ] {
        let (status, _) = call(
            &app,
            "POST",
            "/api/budgets",
            &cookie,
            Some(&bad.to_string()),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    let ok = json!({"name": "Groceries", "category": "GROCERIES", "monthly_limit": 300.0});
    let (status, first) = call(&app, "POST", "/api/budgets", &cookie, Some(&ok.to_string())).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = call(&app, "POST", "/api/budgets", &cookie, Some(&ok.to_string())).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Update validation
    let id = first["id"].as_i64().unwrap();
    for bad in [r#"{}"#, r#"{"monthly_limit": 0}"#, r#"{"name": "  "}"#] {
        let (status, _) = call(
            &app,
            "PUT",
            &format!("/api/budgets/{id}"),
            &cookie,
            Some(bad),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn budget_is_private_to_its_owner() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("bud_own_a");
    let email_b = unique_email("bud_own_b");
    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;

    let body = json!({"name": "Fun", "category": "ENTERTAINMENT", "monthly_limit": 50.0});
    let (_, created) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie_a,
        Some(&body.to_string()),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, _) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie_b, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/budgets/{id}"),
        &cookie_b,
        Some(r#"{"active": false}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/budgets/{id}"),
        &cookie_b,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list_b) = call(&app, "GET", "/api/budgets", &cookie_b, None).await;
    assert_eq!(list_b["budgets"].as_array().unwrap().len(), 0);

    // B can create their own budget for the same category (uniqueness is per user)
    let (status, _) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie_b,
        Some(&body.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // A's budget is untouched
    let (_, still) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie_a, None).await;
    assert_eq!(still["active"], true);

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
}

#[tokio::test]
async fn budget_spent_is_computed_from_transactions() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("bud_spent");
    let other = unique_email("bud_spent_other");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    authenticated_cookie(app.clone(), &other).await;
    let uid = user_id(&pool, &email).await;
    let other_uid = user_id(&pool, &other).await;
    let acct = seed_account(&pool, uid, "depository", Some(1000.0)).await;
    let other_acct = seed_account(&pool, other_uid, "depository", Some(1000.0)).await;

    let body = json!({"name": "Food", "category": "FOOD_DINING", "monthly_limit": 100.0});
    let (_, created) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(&body.to_string()),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let d = today();
    seed_transaction(&pool, &acct, 30.0, d, "FOOD_DINING").await;
    seed_transaction(&pool, &acct, 45.5, d, "FOOD_DINING").await;
    // Money in (negative) must not reduce spending
    seed_transaction(&pool, &acct, -200.0, d, "FOOD_DINING").await;
    // Different category is ignored
    seed_transaction(&pool, &acct, 99.0, d, "GROCERIES").await;
    // Another user's spending is ignored
    seed_transaction(&pool, &other_acct, 500.0, d, "FOOD_DINING").await;
    // Previous month is ignored for the current month
    let last_month = d.with_day(1).unwrap().pred_opt().unwrap();
    seed_transaction(&pool, &acct, 60.0, last_month, "FOOD_DINING").await;

    let (_, now) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(now["spent"], 75.5);
    assert_eq!(now["remaining"], 24.5);
    assert_eq!(now["percent_used"], 75.5);
    assert_eq!(now["over_budget"], false);

    // Pushing spending over the limit flips over_budget
    seed_transaction(&pool, &acct, 40.0, d, "FOOD_DINING").await;
    let (_, over) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(over["spent"], 115.5);
    assert_eq!(over["remaining"], 0.0);
    assert_eq!(over["over_budget"], true);

    // ?month= selects another month on both the item and list routes
    let prev = format!("{:04}-{:02}", last_month.year(), last_month.month());
    let (status, prior) = call(
        &app,
        "GET",
        &format!("/api/budgets/{id}?month={prev}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(prior["spent"], 60.0);
    assert_eq!(prior["month"], prev);
    let (_, list) = call(
        &app,
        "GET",
        &format!("/api/budgets?month={prev}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(list["month"], prev);
    assert_eq!(list["budgets"][0]["spent"], 60.0);

    // A month with no spending is zero, not an error
    let (_, empty) = call(
        &app,
        "GET",
        &format!("/api/budgets/{id}?month=1999-01"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(empty["spent"], 0.0);

    // Re-categorising a transaction moves it between budgets
    let tx = seed_transaction(&pool, &acct, 10.0, d, "OTHER").await;
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/plaid/transactions/{tx}/category"),
        &cookie,
        Some(r#"{"category":"FOOD_DINING"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, moved) = call(&app, "GET", &format!("/api/budgets/{id}"), &cookie, None).await;
    assert_eq!(moved["spent"], 125.5);

    cleanup(&pool, &email).await;
    cleanup(&pool, &other).await;
}

#[tokio::test]
async fn budget_rejects_bad_month() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("bud_month");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let body = json!({"name": "Bills", "category": "UTILITIES", "monthly_limit": 150.0});
    let (_, created) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(&body.to_string()),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    for uri in [
        "/api/budgets?month=2024-13".to_string(),
        "/api/budgets?month=nope".to_string(),
        format!("/api/budgets/{id}?month=2024-1"),
    ] {
        let (status, _) = call(&app, "GET", &uri, &cookie, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn inactive_budgets_are_still_listed() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("bud_inactive");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let body =
        json!({"name": "Shop", "category": "SHOPPING", "monthly_limit": 80.0, "active": false});
    let (status, created) = call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(&body.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["active"], false);
    let (_, list) = call(&app, "GET", "/api/budgets", &cookie, None).await;
    assert_eq!(list["budgets"][0]["active"], false);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn deleting_user_removes_budgets() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("bud_cascade");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;

    let body = json!({"name": "Fun", "category": "ENTERTAINMENT", "monthly_limit": 50.0});
    call(
        &app,
        "POST",
        "/api/budgets",
        &cookie,
        Some(&body.to_string()),
    )
    .await;
    cleanup(&pool, &email).await;

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM budgets WHERE user_id = $1")
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}
