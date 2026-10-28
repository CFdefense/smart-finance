//! Rule endpoints — user-written spending rules scoped to one or more accounts.
//!
//! Rules are plain sentences ("I don't want to spend any money on food"). The backend only
//! stores them and their account scope; the AI agent reads and applies them.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
    Extension, Json,
};
use sqlx::{PgConnection, PgPool};
use tracing::debug;
use utoipa::OpenApi;

use crate::{
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::{
        http::rule::{ListQuery, RuleRequest, RuleResponse, RulesResponse},
        sql::rule::RuleRow,
    },
};

/// Confirms every account in `account_ids` belongs to the user; any foreign or unknown
/// account makes the whole request `NotFound`.
async fn ensure_owned_accounts(
    conn: &mut PgConnection,
    user_id: i32,
    account_ids: &[String],
) -> ApiResult<()> {
    if account_ids.is_empty() {
        return Ok(());
    }
    let owned = sqlx::query_scalar!(
        r#"
        SELECT COUNT(DISTINCT a.account_id) AS "count!"
        FROM plaid_accounts a
        JOIN plaid_links i ON i.item_id = a.item_id
        WHERE a.account_id = ANY($1) AND i.user_id = $2
        "#,
        account_ids,
        user_id
    )
    .fetch_one(conn)
    .await
    .map_err(AppError::from)?;
    if usize::try_from(owned).ok() == Some(account_ids.len()) {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// Replaces the rule's targeted accounts.
async fn set_rule_accounts(
    conn: &mut PgConnection,
    rule_id: i32,
    account_ids: &[String],
) -> ApiResult<()> {
    sqlx::query!("DELETE FROM rule_accounts WHERE rule_id = $1", rule_id)
        .execute(&mut *conn)
        .await
        .map_err(AppError::from)?;
    if !account_ids.is_empty() {
        sqlx::query!(
            "INSERT INTO rule_accounts (rule_id, account_id) SELECT $1, UNNEST($2::text[])",
            rule_id,
            account_ids
        )
        .execute(&mut *conn)
        .await
        .map_err(AppError::from)?;
    }
    Ok(())
}

/// Loads the targeted accounts of the given rules, keyed by rule id.
async fn load_accounts(
    conn: &mut PgConnection,
    rule_ids: &[i32],
) -> ApiResult<HashMap<i32, Vec<String>>> {
    let rows = sqlx::query!(
        "SELECT rule_id, account_id FROM rule_accounts WHERE rule_id = ANY($1) ORDER BY account_id",
        rule_ids
    )
    .fetch_all(conn)
    .await
    .map_err(AppError::from)?;
    let mut map: HashMap<i32, Vec<String>> = HashMap::new();
    for row in rows {
        map.entry(row.rule_id).or_default().push(row.account_id);
    }
    Ok(map)
}

async fn respond(conn: &mut PgConnection, row: &RuleRow) -> ApiResult<RuleResponse> {
    let mut accounts = load_accounts(conn, &[row.id]).await?;
    Ok(RuleResponse::new(
        row,
        accounts.remove(&row.id).unwrap_or_default(),
    ))
}

/// Create a rule.
///
/// `POST /api/rules`
#[utoipa::path(
    post,
    path = "",
    summary = "Create a rule",
    description = "A rule is a sentence in the user's own words. `account_ids` lists the accounts it applies to; omit it (or send an empty list) to apply the rule to all accounts.",
    request_body(
        content = RuleRequest,
        content_type = "application/json",
        example = json!({"description": "I don't want to spend any money on food", "account_ids": ["plaid-checking-id"]})
    ),
    responses(
        (status = 201, description = "Rule created", body = RuleResponse),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "An account was not found")
    ),
    security(("auth-token" = [])),
    tag = "Rules"
)]
pub async fn api_create_rule(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<RuleRequest>,
) -> ApiResult<(StatusCode, Json<RuleResponse>)> {
    debug!("HANDLER ->> POST /api/rules user_id={}", user.id);
    payload.validate().map_err(AppError::Validation)?;
    let account_ids = payload.unique_account_ids();

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    ensure_owned_accounts(&mut tx, user.id, &account_ids).await?;

    let row = sqlx::query_as!(
        RuleRow,
        r#"
        INSERT INTO rules (user_id, description, all_accounts, enabled)
        VALUES ($1, $2, $3, COALESCE($4, TRUE))
        RETURNING id, user_id, description, all_accounts, enabled, created_at, updated_at
        "#,
        user.id,
        payload.description.trim(),
        account_ids.is_empty(),
        payload.enabled
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from)?;
    set_rule_accounts(&mut tx, row.id, &account_ids).await?;
    let response = respond(&mut tx, &row).await?;
    tx.commit().await.map_err(AppError::from)?;

    Ok((StatusCode::CREATED, Json(response)))
}

/// List the user's rules.
///
/// `GET /api/rules`
#[utoipa::path(
    get,
    path = "",
    summary = "List rules",
    description = "Newest first. Filter by `account_id` to get the rules that apply to one account (rules covering all accounts are included), and/or by `enabled`.",
    params(
        ("account_id" = Option<String>, Query, description = "Only rules that apply to this account"),
        ("enabled" = Option<bool>, Query, description = "Only enabled or disabled rules")
    ),
    responses(
        (status = 200, description = "Rules returned", body = RulesResponse),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "Rules"
)]
pub async fn api_list_rules(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<RulesResponse>> {
    debug!("HANDLER ->> GET /api/rules user_id={}", user.id);
    let rows = sqlx::query_as!(
        RuleRow,
        r#"
        SELECT r.id, r.user_id, r.description, r.all_accounts, r.enabled,
               r.created_at, r.updated_at
        FROM rules r
        WHERE r.user_id = $1
          AND ($2::text IS NULL
               OR r.all_accounts
               OR EXISTS (SELECT 1 FROM rule_accounts ra
                          WHERE ra.rule_id = r.id AND ra.account_id = $2))
          AND ($3::bool IS NULL OR r.enabled = $3)
        ORDER BY r.created_at DESC, r.id DESC
        "#,
        user.id,
        query.account_id,
        query.enabled
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    let ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
    let mut conn = pool.acquire().await.map_err(AppError::from)?;
    let mut accounts = load_accounts(&mut conn, &ids).await?;
    let rules = rows
        .iter()
        .map(|r| RuleResponse::new(r, accounts.remove(&r.id).unwrap_or_default()))
        .collect();
    Ok(Json(RulesResponse { rules }))
}

/// Fetch one rule.
///
/// `GET /api/rules/{id}`
#[utoipa::path(
    get,
    path = "/{id}",
    summary = "Get a rule",
    params(("id" = i32, Path, description = "Rule ID")),
    responses(
        (status = 200, description = "Rule returned", body = RuleResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Rule not found")
    ),
    security(("auth-token" = [])),
    tag = "Rules"
)]
pub async fn api_get_rule(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<Json<RuleResponse>> {
    debug!("HANDLER ->> GET /api/rules/{id} user_id={}", user.id);
    let row = sqlx::query_as!(
        RuleRow,
        "SELECT id, user_id, description, all_accounts, enabled, created_at, updated_at
         FROM rules WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;
    let mut conn = pool.acquire().await.map_err(AppError::from)?;
    Ok(Json(respond(&mut conn, &row).await?))
}

/// Replace a rule.
///
/// The sentence and the account scope are replaced; `enabled` is kept when omitted.
///
/// `PUT /api/rules/{id}`
#[utoipa::path(
    put,
    path = "/{id}",
    summary = "Replace a rule",
    params(("id" = i32, Path, description = "Rule ID")),
    request_body(
        content = RuleRequest,
        content_type = "application/json",
        example = json!({"description": "Don't spend any money on food", "account_ids": [], "enabled": true})
    ),
    responses(
        (status = 200, description = "Rule replaced", body = RuleResponse),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Rule or account not found")
    ),
    security(("auth-token" = [])),
    tag = "Rules"
)]
pub async fn api_update_rule(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
    Json(payload): Json<RuleRequest>,
) -> ApiResult<Json<RuleResponse>> {
    debug!("HANDLER ->> PUT /api/rules/{id} user_id={}", user.id);
    payload.validate().map_err(AppError::Validation)?;
    let account_ids = payload.unique_account_ids();

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    ensure_owned_accounts(&mut tx, user.id, &account_ids).await?;

    let row = sqlx::query_as!(
        RuleRow,
        r#"
        UPDATE rules SET
            description  = $3,
            all_accounts = $4,
            enabled      = COALESCE($5, enabled),
            updated_at   = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, description, all_accounts, enabled, created_at, updated_at
        "#,
        id,
        user.id,
        payload.description.trim(),
        account_ids.is_empty(),
        payload.enabled
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;
    set_rule_accounts(&mut tx, row.id, &account_ids).await?;
    let response = respond(&mut tx, &row).await?;
    tx.commit().await.map_err(AppError::from)?;

    Ok(Json(response))
}

/// Delete a rule.
///
/// `DELETE /api/rules/{id}`
#[utoipa::path(
    delete,
    path = "/{id}",
    summary = "Delete a rule",
    params(("id" = i32, Path, description = "Rule ID")),
    responses(
        (status = 204, description = "Rule deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Rule not found")
    ),
    security(("auth-token" = [])),
    tag = "Rules"
)]
pub async fn api_delete_rule(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    debug!("HANDLER ->> DELETE /api/rules/{id} user_id={}", user.id);
    let result = sqlx::query!(
        "DELETE FROM rules WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .execute(&pool)
    .await
    .map_err(AppError::from)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `OpenAPI` doc for rule routes.
///
/// Constructed only by the `#[openapi]` macro and the root [`crate::swagger::ApiDoc`].
#[derive(OpenApi)]
#[openapi(
    paths(
        api_create_rule,
        api_list_rules,
        api_get_rule,
        api_update_rule,
        api_delete_rule
    ),
    components(schemas(RuleRequest, RuleResponse, RulesResponse)),
    info(title = "Rule Routes", description = "User-written spending rules."),
    tags((name = "Rules"))
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct RuleApiDoc;

/// Builds the rule route group. All routes require a valid `auth-token` cookie.
pub fn rule_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/", get(api_list_rules).post(api_create_rule))
        .route(
            "/{id}",
            get(api_get_rule)
                .put(api_update_rule)
                .delete(api_delete_rule),
        )
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
