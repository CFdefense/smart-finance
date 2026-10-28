//! Goal integration tests.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::{json, Value};

use super::fixtures::{
    authenticated_cookie, call, cleanup, seed_account, set_balance, test_app, test_pool,
    unique_email, user_id,
};

fn future_date(days: i64) -> String {
    (Utc::now().date_naive() + Duration::days(days)).to_string()
}

async fn create_goal(app: &axum::Router, cookie: &str, body: &Value) -> (StatusCode, Value) {
    call(app, "POST", "/api/goals", cookie, Some(&body.to_string())).await
}

async fn contribute(
    app: &axum::Router,
    cookie: &str,
    goal: i64,
    body: &Value,
) -> (StatusCode, Value) {
    call(
        app,
        "POST",
        &format!("/api/goals/{goal}/contributions"),
        cookie,
        Some(&body.to_string()),
    )
    .await
}

#[tokio::test]
async fn goals_require_authentication() {
    let app = test_app().await;
    for (method, uri) in [
        ("GET", "/api/goals"),
        ("POST", "/api/goals"),
        ("GET", "/api/goals/1"),
        ("PUT", "/api/goals/1"),
        ("DELETE", "/api/goals/1"),
        ("GET", "/api/goals/1/contributions"),
        ("POST", "/api/goals/1/contributions"),
        ("DELETE", "/api/goals/1/contributions/1"),
    ] {
        let (status, _) = call(&app, method, uri, "", Some("{}")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn goal_crud_lifecycle() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_crud");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let body = json!({
        "name": " Vacation ",
        "description": "  Beach  ",
        "target_amount": 1000.0,
        "target_date": future_date(100)
    });
    let (status, created) = create_goal(&app, &cookie, &body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["name"], "Vacation");
    assert_eq!(created["description"], "Beach");
    assert_eq!(created["current_amount"], 0.0);
    assert_eq!(created["progress_percent"], 0.0);
    assert_eq!(created["remaining"], 1000.0);
    assert_eq!(created["status"], "ACTIVE");
    assert_eq!(created["account_id"], Value::Null);
    let id = created["id"].as_i64().unwrap();

    let (status, got) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["id"], id);

    let (_, list) = call(&app, "GET", "/api/goals", &cookie, None).await;
    assert_eq!(list["goals"].as_array().unwrap().len(), 1);

    // Partial update keeps everything else
    let (status, updated) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(r#"{"target_amount": 2000.0, "name": "Big trip"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["target_amount"], 2000.0);
    assert_eq!(updated["name"], "Big trip");
    assert_eq!(updated["description"], "Beach");
    assert!(updated["target_date"].is_string());

    // null clears description and target_date; "" also clears description
    let (_, cleared) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(r#"{"description": null, "target_date": null}"#),
    )
    .await;
    assert_eq!(cleared["description"], Value::Null);
    assert_eq!(cleared["target_date"], Value::Null);
    assert_eq!(cleared["on_track"], Value::Null);
    let (_, set) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(r#"{"description": "again"}"#),
    )
    .await;
    assert_eq!(set["description"], "again");
    let (_, blank) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(r#"{"description": ""}"#),
    )
    .await;
    assert_eq!(blank["description"], Value::Null);

    let (status, _) = call(&app, "DELETE", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "DELETE", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn goal_validation() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_val");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let past = (Utc::now().date_naive() - Duration::days(1)).to_string();
    for bad in [
        json!({"name": "", "target_amount": 10.0}),
        json!({"name": "x", "target_amount": 0.0}),
        json!({"name": "x", "target_amount": -1.0}),
        json!({"name": "x", "target_amount": 10.0, "target_date": past}),
        json!({"name": "x", "target_amount": 10.0, "target_date": Utc::now().date_naive().to_string()}),
        json!({"name": "x", "target_amount": 10.0, "baseline_balance": 5.0}),
        json!({"name": "x", "target_amount": 10.0, "description": "d".repeat(501)}),
    ] {
        let (status, _) = create_goal(&app, &cookie, &bad).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // Malformed shapes are rejected by deserialization
    for bad in [
        json!({"target_amount": 10.0}),
        json!({"name": "x", "target_amount": "ten"}),
        json!({"name": "x", "target_amount": 10.0, "target_date": "not-a-date"}),
    ] {
        let (status, _) = create_goal(&app, &cookie, &bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    let (_, created) =
        create_goal(&app, &cookie, &json!({"name": "ok", "target_amount": 10.0})).await;
    let id = created["id"].as_i64().unwrap();
    for bad in [
        r#"{}"#,
        r#"{"name": " "}"#,
        r#"{"target_amount": 0}"#,
        r#"{"baseline_balance": 5.0}"#,
        r#"{"account_id": null, "baseline_balance": 5.0}"#,
    ] {
        let (status, _) = call(&app, "PUT", &format!("/api/goals/{id}"), &cookie, Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let past_update = format!(r#"{{"target_date": "{past}"}}"#);
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(&past_update),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn contributions_drive_progress_and_status() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_contrib");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (_, g) = create_goal(
        &app,
        &cookie,
        &json!({"name": "Bike", "target_amount": 200.0}),
    )
    .await;
    let id = g["id"].as_i64().unwrap();

    let (status, res) = contribute(
        &app,
        &cookie,
        id,
        &json!({"amount": 50.0, "note": " first "}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(res["contribution"]["amount"], 50.0);
    assert_eq!(res["contribution"]["note"], "first");
    assert_eq!(res["goal"]["current_amount"], 50.0);
    assert_eq!(res["goal"]["progress_percent"], 25.0);
    assert_eq!(res["goal"]["remaining"], 150.0);
    assert_eq!(res["goal"]["status"], "ACTIVE");

    // Reaching the target completes the goal
    let (_, res) = contribute(&app, &cookie, id, &json!({"amount": 150.0})).await;
    assert_eq!(res["goal"]["status"], "COMPLETED");
    assert_eq!(res["goal"]["progress_percent"], 100.0);
    let completed_id = res["contribution"]["id"].as_i64().unwrap();

    // Overshooting is allowed; progress stays capped at 100
    let (_, res) = contribute(&app, &cookie, id, &json!({"amount": 25.0})).await;
    assert_eq!(res["goal"]["current_amount"], 225.0);
    assert_eq!(res["goal"]["progress_percent"], 100.0);
    assert_eq!(res["goal"]["remaining"], 0.0);

    // Withdrawing drops it back to ACTIVE
    let (status, res) = contribute(&app, &cookie, id, &json!({"amount": -50.0})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(res["goal"]["current_amount"], 175.0);
    assert_eq!(res["goal"]["status"], "ACTIVE");

    // Deleting a contribution undoes it and re-completes the goal
    let (status, after) = call(
        &app,
        "DELETE",
        &format!(
            "/api/goals/{id}/contributions/{}",
            res["contribution"]["id"]
        ),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["current_amount"], 225.0);
    assert_eq!(after["status"], "COMPLETED");

    // Ledger lists newest first with all entries
    let (status, ledger) = call(
        &app,
        "GET",
        &format!("/api/goals/{id}/contributions"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entries = ledger["contributions"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries
        .iter()
        .any(|e| e["id"].as_i64() == Some(completed_id)));

    // Dates: explicit past date accepted, future date rejected
    let past = (Utc::now().date_naive() - Duration::days(3)).to_string();
    let (status, res) = contribute(&app, &cookie, id, &json!({"amount": 1.0, "date": past})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(res["contribution"]["date"], past);
    let (status, _) = contribute(
        &app,
        &cookie,
        id,
        &json!({"amount": 1.0, "date": future_date(2)}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn contribution_edge_cases() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_edges");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let (_, g) = create_goal(
        &app,
        &cookie,
        &json!({"name": "Fund", "target_amount": 100.0}),
    )
    .await;
    let id = g["id"].as_i64().unwrap();

    // Invalid amounts / notes
    for bad in [
        json!({"amount": 0.0}),
        json!({"amount": 0.001}),
        json!({"amount": 5.0, "note": "n".repeat(201)}),
    ] {
        let (status, _) = contribute(&app, &cookie, id, &bad).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (status, _) = contribute(&app, &cookie, id, &json!({"note": "x"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Cannot withdraw from an empty goal, nor more than was deposited
    let (status, _) = contribute(&app, &cookie, id, &json!({"amount": -1.0})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, dep) = contribute(&app, &cookie, id, &json!({"amount": 30.0})).await;
    let dep_id = dep["contribution"]["id"].as_i64().unwrap();
    let (status, _) = contribute(&app, &cookie, id, &json!({"amount": -30.01})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Withdrawing exactly everything is fine
    let (status, w) = contribute(&app, &cookie, id, &json!({"amount": -30.0})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(w["goal"]["current_amount"], 0.0);

    // Deleting the deposit now would leave the withdrawal uncovered
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/goals/{id}/contributions/{dep_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Deleting the withdrawal first is fine, then the deposit can go too
    let w_id = w["contribution"]["id"].as_i64().unwrap();
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/goals/{id}/contributions/{w_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, after) = call(
        &app,
        "DELETE",
        &format!("/api/goals/{id}/contributions/{dep_id}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["current_amount"], 0.0);

    // Unknown contribution / goal
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/goals/{id}/contributions/999999999"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = contribute(&app, &cookie, 999_999_999, &json!({"amount": 1.0})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "GET",
        "/api/goals/999999999/contributions",
        &cookie,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn archived_goals_reject_contributions_and_filter_by_status() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_arch");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (_, active) =
        create_goal(&app, &cookie, &json!({"name": "A", "target_amount": 100.0})).await;
    let (_, done) = create_goal(&app, &cookie, &json!({"name": "B", "target_amount": 10.0})).await;
    let (_, old) = create_goal(&app, &cookie, &json!({"name": "C", "target_amount": 10.0})).await;
    let done_id = done["id"].as_i64().unwrap();
    let old_id = old["id"].as_i64().unwrap();
    contribute(&app, &cookie, done_id, &json!({"amount": 10.0})).await;

    let (status, archived) = call(
        &app,
        "PUT",
        &format!("/api/goals/{old_id}"),
        &cookie,
        Some(r#"{"archived": true}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(archived["status"], "ARCHIVED");

    let (status, _) = contribute(&app, &cookie, old_id, &json!({"amount": 1.0})).await;
    assert_eq!(status, StatusCode::CONFLICT);

    for (filter, expected_id) in [
        ("ACTIVE", active["id"].as_i64().unwrap()),
        ("COMPLETED", done_id),
        ("ARCHIVED", old_id),
    ] {
        let (status, list) = call(
            &app,
            "GET",
            &format!("/api/goals?status={filter}"),
            &cookie,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let goals = list["goals"].as_array().unwrap();
        assert_eq!(goals.len(), 1, "{filter}");
        assert_eq!(goals[0]["id"].as_i64().unwrap(), expected_id, "{filter}");
    }
    let (_, all) = call(&app, "GET", "/api/goals", &cookie, None).await;
    assert_eq!(all["goals"].as_array().unwrap().len(), 3);
    let (status, _) = call(&app, "GET", "/api/goals?status=bogus", &cookie, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Unarchiving restores contributions; a completed goal that is archived reads ARCHIVED
    let (_, back) = call(
        &app,
        "PUT",
        &format!("/api/goals/{old_id}"),
        &cookie,
        Some(r#"{"archived": false}"#),
    )
    .await;
    assert_eq!(back["status"], "ACTIVE");
    let (_, arch_done) = call(
        &app,
        "PUT",
        &format!("/api/goals/{done_id}"),
        &cookie,
        Some(r#"{"archived": true}"#),
    )
    .await;
    assert_eq!(arch_done["status"], "ARCHIVED");

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn account_linked_goal_tracks_balance_growth() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_acct");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let savings = seed_account(&pool, uid, "depository", Some(1000.0)).await;

    // Default baseline = current balance, so a new goal starts at zero
    let (status, g) = create_goal(
        &app,
        &cookie,
        &json!({"name": "House", "target_amount": 500.0, "account_id": savings}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(g["account_id"], savings);
    assert_eq!(g["current_amount"], 0.0);
    let id = g["id"].as_i64().unwrap();

    // Balance grows: progress follows automatically
    set_balance(&pool, &savings, 1200.0).await;
    let (_, got) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(got["current_amount"], 200.0);
    assert_eq!(got["progress_percent"], 40.0);

    // Manual contributions add on top of account growth
    let (_, res) = contribute(&app, &cookie, id, &json!({"amount": 50.0})).await;
    assert_eq!(res["goal"]["current_amount"], 250.0);

    // Reaching the target via the account completes the goal without any write
    set_balance(&pool, &savings, 1500.0).await;
    let (_, list) = call(&app, "GET", "/api/goals?status=COMPLETED", &cookie, None).await;
    assert_eq!(list["goals"].as_array().unwrap().len(), 1);

    // A balance drop reduces progress and can reopen the goal
    set_balance(&pool, &savings, 1000.0).await;
    let (_, dropped) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(dropped["current_amount"], 50.0);
    assert_eq!(dropped["status"], "ACTIVE");

    // Unlinking settles growth into the ledger so progress is preserved
    set_balance(&pool, &savings, 1300.0).await;
    let (status, unlinked) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(r#"{"account_id": null}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unlinked["account_id"], Value::Null);
    assert_eq!(unlinked["current_amount"], 350.0);
    set_balance(&pool, &savings, 5000.0).await;
    let (_, still) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(
        still["current_amount"], 350.0,
        "unlinked goal ignores the old account"
    );

    // Re-link with an explicit baseline of 0 counts the whole balance
    let relink = json!({"account_id": savings, "baseline_balance": 0.0}).to_string();
    let (status, linked) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(&relink),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(linked["current_amount"], 5350.0);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn switching_accounts_preserves_progress() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_switch");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let first = seed_account(&pool, uid, "depository", Some(100.0)).await;
    let second = seed_account(&pool, uid, "depository", Some(900.0)).await;

    let (_, g) = create_goal(
        &app,
        &cookie,
        &json!({"name": "G", "target_amount": 1000.0, "account_id": first}),
    )
    .await;
    let id = g["id"].as_i64().unwrap();
    set_balance(&pool, &first, 160.0).await;

    let body = json!({"account_id": second}).to_string();
    let (status, switched) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(&body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(switched["account_id"], second);
    assert_eq!(
        switched["current_amount"], 60.0,
        "growth on the old account is kept"
    );

    set_balance(&pool, &second, 940.0).await;
    let (_, after) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(after["current_amount"], 100.0);

    // Re-sending the same account without a baseline is a no-op (no double settle)
    let (_, same) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie,
        Some(&body),
    )
    .await;
    assert_eq!(same["current_amount"], 100.0);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn goal_account_ownership_and_type_checks() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("goal_own_a");
    let email_b = unique_email("goal_own_b");
    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;
    let uid_a = user_id(&pool, &email_a).await;
    let uid_b = user_id(&pool, &email_b).await;
    let acct_a = seed_account(&pool, uid_a, "depository", Some(10.0)).await;
    let credit_b = seed_account(&pool, uid_b, "credit", Some(10.0)).await;

    // B cannot link A's account, nor an unknown one
    for account in [acct_a.as_str(), "no-such-account"] {
        let (status, _) = create_goal(
            &app,
            &cookie_b,
            &json!({"name": "x", "target_amount": 10.0, "account_id": account}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{account}");
    }
    // Credit accounts cannot be linked
    let (status, _) = create_goal(
        &app,
        &cookie_b,
        &json!({"name": "x", "target_amount": 10.0, "account_id": credit_b}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Same checks on update
    let (_, g) = create_goal(
        &app,
        &cookie_b,
        &json!({"name": "mine", "target_amount": 10.0}),
    )
    .await;
    let id = g["id"].as_i64().unwrap();
    let foreign = json!({"account_id": acct_a}).to_string();
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie_b,
        Some(&foreign),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let credit = json!({"account_id": credit_b}).to_string();
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie_b,
        Some(&credit),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A failed update is atomic: the goal is unchanged
    let (_, unchanged) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie_b, None).await;
    assert_eq!(unchanged["account_id"], Value::Null);

    // Goals themselves are private
    let (status, _) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie_a, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "PUT",
        &format!("/api/goals/{id}"),
        &cookie_a,
        Some(r#"{"archived": true}"#),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "DELETE", &format!("/api/goals/{id}"), &cookie_a, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = contribute(&app, &cookie_a, id, &json!({"amount": 5.0})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/goals/{id}/contributions"),
        &cookie_a,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list_a) = call(&app, "GET", "/api/goals", &cookie_a, None).await;
    assert_eq!(list_a["goals"].as_array().unwrap().len(), 0);

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
}

#[tokio::test]
async fn disconnected_account_keeps_manual_progress() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_disc");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let acct = seed_account(&pool, uid, "depository", Some(100.0)).await;

    let (_, g) = create_goal(
        &app,
        &cookie,
        &json!({"name": "G", "target_amount": 500.0, "account_id": acct}),
    )
    .await;
    let id = g["id"].as_i64().unwrap();
    contribute(&app, &cookie, id, &json!({"amount": 75.0})).await;

    // Removing the account (as a bank disconnect would) must not break the goal
    sqlx::query("DELETE FROM plaid_accounts WHERE account_id = $1")
        .bind(&acct)
        .execute(&pool)
        .await
        .unwrap();
    let (status, got) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["account_id"], Value::Null);
    assert_eq!(got["current_amount"], 75.0);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn on_track_reflects_pace_toward_target_date() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_pace");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    // Created today with a date in the future: nothing is expected yet
    let (_, g) = create_goal(
        &app,
        &cookie,
        &json!({"name": "G", "target_amount": 100.0, "target_date": future_date(30)}),
    )
    .await;
    assert_eq!(g["on_track"], true);

    // Backdate creation so half the window has elapsed; no progress means behind
    let id = g["id"].as_i64().unwrap();
    sqlx::query("UPDATE goals SET created_at = NOW() - INTERVAL '30 days' WHERE id = $1")
        .bind(id as i32)
        .execute(&pool)
        .await
        .unwrap();
    let (_, behind) = call(&app, "GET", &format!("/api/goals/{id}"), &cookie, None).await;
    assert_eq!(behind["on_track"], false);

    let (_, res) = contribute(&app, &cookie, id, &json!({"amount": 60.0})).await;
    assert_eq!(res["goal"]["on_track"], true);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn deleting_goal_removes_contributions() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("goal_cascade");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let (_, g) = create_goal(&app, &cookie, &json!({"name": "G", "target_amount": 100.0})).await;
    let id = g["id"].as_i64().unwrap();
    contribute(&app, &cookie, id, &json!({"amount": 5.0})).await;

    call(&app, "DELETE", &format!("/api/goals/{id}"), &cookie, None).await;
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM goal_contributions WHERE goal_id = $1")
            .bind(id as i32)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);

    cleanup(&pool, &email).await;
}
