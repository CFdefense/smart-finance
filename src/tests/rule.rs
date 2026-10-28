//! Rule integration tests.

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::fixtures::{
    authenticated_cookie, call, cleanup, seed_account, test_app, test_pool, unique_email, user_id,
};

async fn create_rule(app: &axum::Router, cookie: &str, body: &Value) -> (StatusCode, Value) {
    call(app, "POST", "/api/rules", cookie, Some(&body.to_string())).await
}

async fn put_rule(app: &axum::Router, cookie: &str, id: i64, body: &Value) -> (StatusCode, Value) {
    call(
        app,
        "PUT",
        &format!("/api/rules/{id}"),
        cookie,
        Some(&body.to_string()),
    )
    .await
}

fn descriptions(list: &Value) -> Vec<String> {
    list["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["description"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn rules_require_authentication() {
    let app = test_app().await;
    for (method, uri) in [
        ("GET", "/api/rules"),
        ("POST", "/api/rules"),
        ("GET", "/api/rules/1"),
        ("PUT", "/api/rules/1"),
        ("DELETE", "/api/rules/1"),
    ] {
        let (status, _) = call(&app, method, uri, "", Some("{}")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn rule_for_all_accounts_when_none_given() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rule_all");
    let cookie = authenticated_cookie(app.clone(), &email).await;

    let (status, created) = create_rule(
        &app,
        &cookie,
        &json!({"description": "  I don't want to spend ANY money on food  "}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        created["description"],
        "I don't want to spend ANY money on food"
    );
    assert_eq!(created["all_accounts"], true);
    assert_eq!(created["account_ids"], json!([]));
    assert_eq!(created["enabled"], true);

    // An explicit empty list means the same thing
    let (status, empty) = create_rule(
        &app,
        &cookie,
        &json!({"description": "no shopping", "account_ids": []}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(empty["all_accounts"], true);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn rule_targets_one_or_many_accounts() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rule_scope");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let checking = seed_account(&pool, uid, "depository", Some(1.0)).await;
    let savings = seed_account(&pool, uid, "depository", Some(2.0)).await;
    let card = seed_account(&pool, uid, "credit", Some(3.0)).await;

    let (status, one) = create_rule(
        &app,
        &cookie,
        &json!({"description": "no food from checking", "account_ids": [checking]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(one["all_accounts"], false);
    assert_eq!(one["account_ids"], json!([checking]));

    // Duplicates collapse; several accounts are all returned
    let (status, many) = create_rule(
        &app,
        &cookie,
        &json!({"description": "no food", "account_ids": [savings, card, savings]}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(many["all_accounts"], false);
    let mut ids: Vec<String> = many["account_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    ids.sort();
    let mut expected = vec![savings.clone(), card.clone()];
    expected.sort();
    assert_eq!(ids, expected);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn rule_crud_lifecycle() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rule_crud");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let a = seed_account(&pool, uid, "depository", Some(500.0)).await;
    let b = seed_account(&pool, uid, "depository", Some(500.0)).await;

    let (_, created) = create_rule(
        &app,
        &cookie,
        &json!({"description": "no food", "account_ids": [a]}),
    )
    .await;
    let id = created["id"].as_i64().unwrap();

    let (status, got) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["account_ids"], json!([a]));
    let (_, list) = call(&app, "GET", "/api/rules", &cookie, None).await;
    assert_eq!(list["rules"].as_array().unwrap().len(), 1);

    // PUT replaces the sentence and the scope; `enabled` is kept when omitted
    let (status, replaced) = put_rule(
        &app,
        &cookie,
        id,
        &json!({"description": "no shopping", "account_ids": [a, b]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replaced["description"], "no shopping");
    assert_eq!(replaced["account_ids"].as_array().unwrap().len(), 2);
    assert_eq!(replaced["enabled"], true);

    // Narrowing back to one account removes the other
    let (_, narrowed) = put_rule(
        &app,
        &cookie,
        id,
        &json!({"description": "no shopping", "account_ids": [b]}),
    )
    .await;
    assert_eq!(narrowed["account_ids"], json!([b]));

    // Empty list widens to all accounts
    let (_, widened) = put_rule(
        &app,
        &cookie,
        id,
        &json!({"description": "no shopping", "account_ids": []}),
    )
    .await;
    assert_eq!(widened["all_accounts"], true);
    assert_eq!(widened["account_ids"], json!([]));

    // Disable, then an omitted `enabled` leaves it disabled
    let (_, off) = put_rule(
        &app,
        &cookie,
        id,
        &json!({"description": "no shopping", "enabled": false}),
    )
    .await;
    assert_eq!(off["enabled"], false);
    let (_, still_off) = put_rule(&app, &cookie, id, &json!({"description": "edited"})).await;
    assert_eq!(still_off["enabled"], false);
    assert_eq!(still_off["description"], "edited");

    let (status, _) = call(&app, "DELETE", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "DELETE", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = put_rule(&app, &cookie, id, &json!({"description": "x"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn rule_validation() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rule_val");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let acct = seed_account(&pool, uid, "depository", Some(10.0)).await;

    let too_many: Vec<String> = (0..51).map(|i| format!("acct-{i}")).collect();
    for bad in [
        json!({"description": ""}),
        json!({"description": "   "}),
        json!({"description": "x".repeat(501)}),
        json!({"description": "d", "account_ids": [acct, " "]}),
        json!({"description": "d", "account_ids": [""]}),
        json!({"description": "d", "account_ids": too_many}),
    ] {
        let (status, _) = create_rule(&app, &cookie, &bad).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // Deserialization failures
    for bad in [
        json!({}),
        json!({"description": 5}),
        json!({"description": "d", "account_ids": "nope"}),
        json!({"description": "d", "enabled": "yes"}),
    ] {
        let (status, _) = create_rule(&app, &cookie, &bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    // Boundary: exactly 500 characters
    let (status, _) = create_rule(&app, &cookie, &json!({"description": "x".repeat(500)})).await;
    assert_eq!(status, StatusCode::CREATED);

    // The same validation applies to PUT
    let (_, r) = create_rule(&app, &cookie, &json!({"description": "d"})).await;
    let id = r["id"].as_i64().unwrap();
    let (status, _) = put_rule(&app, &cookie, id, &json!({"description": " "})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn rules_are_private_and_accounts_must_be_owned() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email_a = unique_email("rule_own_a");
    let email_b = unique_email("rule_own_b");
    let cookie_a = authenticated_cookie(app.clone(), &email_a).await;
    let cookie_b = authenticated_cookie(app.clone(), &email_b).await;
    let acct_a = seed_account(&pool, user_id(&pool, &email_a).await, "depository", None).await;
    let acct_b = seed_account(&pool, user_id(&pool, &email_b).await, "depository", None).await;

    // B cannot target A's account, an unknown one, or a mix that includes one
    for ids in [
        json!([acct_a]),
        json!(["nope"]),
        json!([acct_b, acct_a]),
        json!([acct_b, "nope"]),
    ] {
        let (status, _) = create_rule(
            &app,
            &cookie_b,
            &json!({"description": "d", "account_ids": ids}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{ids}");
    }
    let (_, none_created) = call(&app, "GET", "/api/rules", &cookie_b, None).await;
    assert_eq!(
        none_created["rules"].as_array().unwrap().len(),
        0,
        "failed creates must not leave a rule behind"
    );

    let (_, mine) = create_rule(
        &app,
        &cookie_b,
        &json!({"description": "mine", "account_ids": [acct_b]}),
    )
    .await;
    let id = mine["id"].as_i64().unwrap();

    // A failed replace leaves the rule exactly as it was
    let (status, _) = put_rule(
        &app,
        &cookie_b,
        id,
        &json!({"description": "changed", "account_ids": [acct_a]}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, still) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie_b, None).await;
    assert_eq!(still["description"], "mine");
    assert_eq!(still["account_ids"], json!([acct_b]));

    // A cannot see or touch B's rule
    let (status, _) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie_a, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = put_rule(&app, &cookie_a, id, &json!({"description": "hijack"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(&app, "DELETE", &format!("/api/rules/{id}"), &cookie_a, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list_a) = call(&app, "GET", "/api/rules", &cookie_a, None).await;
    assert_eq!(list_a["rules"].as_array().unwrap().len(), 0);

    cleanup(&pool, &email_a).await;
    cleanup(&pool, &email_b).await;
}

#[tokio::test]
async fn list_filters_by_account_and_enabled() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rule_list");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let checking = seed_account(&pool, uid, "depository", None).await;
    let savings = seed_account(&pool, uid, "depository", None).await;

    create_rule(&app, &cookie, &json!({"description": "everywhere"})).await;
    create_rule(
        &app,
        &cookie,
        &json!({"description": "checking only", "account_ids": [checking]}),
    )
    .await;
    create_rule(
        &app,
        &cookie,
        &json!({"description": "both", "account_ids": [checking, savings]}),
    )
    .await;
    create_rule(
        &app,
        &cookie,
        &json!({"description": "savings off", "account_ids": [savings], "enabled": false}),
    )
    .await;

    // Newest first
    let (_, all) = call(&app, "GET", "/api/rules", &cookie, None).await;
    assert_eq!(
        descriptions(&all),
        vec!["savings off", "both", "checking only", "everywhere"]
    );

    // Account filter includes all-account rules
    let (_, for_checking) = call(
        &app,
        "GET",
        &format!("/api/rules?account_id={checking}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(
        descriptions(&for_checking),
        vec!["both", "checking only", "everywhere"]
    );
    let (_, for_savings) = call(
        &app,
        "GET",
        &format!("/api/rules?account_id={savings}"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(
        descriptions(&for_savings),
        vec!["savings off", "both", "everywhere"]
    );

    // enabled filter, alone and combined
    let (_, enabled) = call(&app, "GET", "/api/rules?enabled=true", &cookie, None).await;
    assert_eq!(
        descriptions(&enabled),
        vec!["both", "checking only", "everywhere"]
    );
    let (_, disabled) = call(&app, "GET", "/api/rules?enabled=false", &cookie, None).await;
    assert_eq!(descriptions(&disabled), vec!["savings off"]);
    let (_, combined) = call(
        &app,
        "GET",
        &format!("/api/rules?account_id={savings}&enabled=true"),
        &cookie,
        None,
    )
    .await;
    assert_eq!(descriptions(&combined), vec!["both", "everywhere"]);

    // An unknown account id matches only the all-account rules
    let (_, unknown) = call(&app, "GET", "/api/rules?account_id=nope", &cookie, None).await;
    assert_eq!(descriptions(&unknown), vec!["everywhere"]);

    // Malformed filter value
    let (status, _) = call(&app, "GET", "/api/rules?enabled=maybe", &cookie, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup(&pool, &email).await;
}

#[tokio::test]
async fn deleting_an_account_narrows_a_rule_without_widening_it() {
    let app = test_app().await;
    let pool = test_pool().await;
    let email = unique_email("rule_cascade");
    let cookie = authenticated_cookie(app.clone(), &email).await;
    let uid = user_id(&pool, &email).await;
    let keep = seed_account(&pool, uid, "depository", Some(1.0)).await;
    let gone = seed_account(&pool, uid, "depository", Some(1.0)).await;

    let (_, r) = create_rule(
        &app,
        &cookie,
        &json!({"description": "d", "account_ids": [keep, gone]}),
    )
    .await;
    let id = r["id"].as_i64().unwrap();

    sqlx::query("DELETE FROM plaid_accounts WHERE account_id = $1")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();
    let (_, after) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(after["account_ids"], json!([keep]));

    // Losing the last targeted account leaves a rule that targets nothing, not "all accounts"
    sqlx::query("DELETE FROM plaid_accounts WHERE account_id = $1")
        .bind(&keep)
        .execute(&pool)
        .await
        .unwrap();
    let (status, orphan) = call(&app, "GET", &format!("/api/rules/{id}"), &cookie, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(orphan["all_accounts"], false);
    assert_eq!(orphan["account_ids"], json!([]));

    cleanup(&pool, &email).await;
}
