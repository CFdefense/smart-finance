//! Rule endpoints — user-written spending rules scoped to one or more accounts.
//!
//! Rules are plain sentences ("I don't want to spend any money on food"). The backend only
//! stores them and their account scope; the AI agent reads and applies them. A rule with no
//! linked accounts applies to all of the user's accounts.

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
    // Return if no ids
    if account_ids.is_empty() {
        return Ok(());
    }

    // Ensure every account is owned by the user
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

    // Ensure we have a matching number of accounts
    if usize::try_from(owned).ok() == Some(account_ids.len()) {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
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
        (status = 201, description = "Rule created"),
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
) -> ApiResult<StatusCode> {
    debug!("HANDLER ->> POST /api/rules user_id={}", user.id);

    // Validate the playload message
    payload.validate().map_err(AppError::Validation)?;

    // Get the accounts ids
    let mut account_ids = payload.unique_account_ids();
    account_ids.sort();

    // Start a transaction
    // We do this because were doing two subsequent queries and want to roll back in one fails and the other doesnt
    let mut tx = pool.begin().await.map_err(AppError::from)?;
    ensure_owned_accounts(&mut tx, user.id, &account_ids).await?;

    // Insert our rule
    let row = sqlx::query_as!(
        RuleRow,
        r#"
        INSERT INTO rules (user_id, description, enabled)
        VALUES ($1, $2, COALESCE($3, TRUE))
        RETURNING id, user_id, description, enabled, created_at, updated_at,
                  $4::text[] AS "account_ids!"
        "#,
        user.id,
        payload.description.trim(),
        payload.enabled,
        &account_ids
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from)?;

    // Add the accounts used to rule accounts
    sqlx::query!(
        "INSERT INTO rule_accounts (rule_id, account_id) SELECT $1, UNNEST($2::text[])",
        row.id,
        &account_ids
    )
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    // Commit the transaction
    tx.commit().await.map_err(AppError::from)?;

    Ok(StatusCode::CREATED)
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
    // `account_id` only filters the caller's own rules, so it needs no ownership check.
    // Rules with no linked accounts apply to all accounts and always match.
    let rows = sqlx::query_as!(
        RuleRow,
        r#"
        SELECT r.id, r.user_id, r.description, r.enabled, r.created_at, r.updated_at,
               COALESCE(array_agg(ra.account_id ORDER BY ra.account_id)
                        FILTER (WHERE ra.account_id IS NOT NULL), '{}') AS "account_ids!"
        FROM rules r
        LEFT JOIN rule_accounts ra ON ra.rule_id = r.id
        WHERE r.user_id = $1
          AND ($3::bool IS NULL OR r.enabled = $3)
        GROUP BY r.id
        HAVING $2::text IS NULL
            OR COUNT(ra.account_id) = 0
            OR $2 = ANY(array_agg(ra.account_id))
        ORDER BY r.created_at DESC, r.id DESC
        "#,
        user.id,
        query.account_id,
        query.enabled
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    Ok(Json(RulesResponse {
        rules: rows.iter().map(RuleResponse::new).collect(),
    }))
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

    // Get the specific rule
    let row = sqlx::query_as!(
        RuleRow,
        r#"
        SELECT r.id, r.user_id, r.description, r.enabled, r.created_at, r.updated_at,
               COALESCE(array_agg(ra.account_id ORDER BY ra.account_id)
                        FILTER (WHERE ra.account_id IS NOT NULL), '{}') AS "account_ids!"
        FROM rules r
        LEFT JOIN rule_accounts ra ON ra.rule_id = r.id
        WHERE r.id = $1 AND r.user_id = $2
        GROUP BY r.id
        "#,
        id,
        user.id
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(RuleResponse::new(&row)))
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

    // Validate the payload
    payload.validate().map_err(AppError::Validation)?;

    // Get the account ids
    let mut account_ids = payload.unique_account_ids();
    account_ids.sort();

    // Start a transaction
    // We do this because were doing two subsequent queries and want to roll back in one fails and the other doesnt
    let mut tx = pool.begin().await.map_err(AppError::from)?;

    // Ensure the account ids provided are actually owned by the user
    ensure_owned_accounts(&mut tx, user.id, &account_ids).await?;

    // Update the rule
    let row = sqlx::query_as!(
        RuleRow,
        r#"
        UPDATE rules SET
            description = $3,
            enabled     = COALESCE($4, enabled),
            updated_at  = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, description, enabled, created_at, updated_at,
                  $5::text[] AS "account_ids!"
        "#,
        id,
        user.id,
        payload.description.trim(),
        payload.enabled,
        &account_ids
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    // Delete rule accounts if detached
    sqlx::query!("DELETE FROM rule_accounts WHERE rule_id = $1", id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

    // Insert new rule accounts if added
    sqlx::query!(
        "INSERT INTO rule_accounts (rule_id, account_id) SELECT $1, UNNEST($2::text[])",
        id,
        &account_ids
    )
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    // Commit the transaction
    tx.commit().await.map_err(AppError::from)?;

    Ok(Json(RuleResponse::new(&row)))
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

    // Delete the rule
    let result = sqlx::query!(
        "DELETE FROM rules WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .execute(&pool)
    .await
    .map_err(AppError::from)?;

    // Error if no rows affected from delete
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
