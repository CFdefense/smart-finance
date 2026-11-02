//! Goal endpoints — account-balance goals ("get this account to $X").
//!
//! A goal links one depository account and records its balance at creation as the baseline.
//! Progress is derived on read from the baseline, the account's current balance and the target.

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
        common::today,
        http::goals::{
            CreateGoalRequest, GoalResponse, GoalStatus, GoalsQuery, GoalsResponse,
            UpdateGoalRequest,
        },
        sql::goals::GoalRow,
    },
};

/// Verifies `account_id` belongs to the user and is a depository account.
///
/// Returns its current balance (`None` when Plaid reported none). Foreign or unknown accounts
/// are `NotFound` so existence is not leaked; other account types are a validation error.
async fn owned_depository_balance(
    conn: &mut PgConnection,
    user_id: i32,
    account_id: &str,
) -> ApiResult<Option<f64>> {
    let account = sqlx::query!(
        r#"
        SELECT a.account_type, a.current_balance
        FROM plaid_accounts a
        JOIN plaid_links i ON i.item_id = a.item_id
        WHERE a.account_id = $1 AND i.user_id = $2
        "#,
        account_id,
        user_id
    )
    .fetch_optional(conn)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    if account.account_type != "depository" {
        return Err(AppError::Validation(
            "only depository (checking/savings) accounts can be linked to a goal".to_string(),
        ));
    }

    Ok(account.current_balance)
}

/// A goal only makes sense when the target is above where the account started.
fn check_target_above_baseline(target: f64, baseline: f64) -> ApiResult<()> {
    if target <= baseline {
        return Err(AppError::Validation(
            "target_amount must be greater than the account's starting balance".to_string(),
        ));
    }
    Ok(())
}

/// Create a goal for an account. The baseline defaults to the account's current balance.
///
/// `POST /api/goals`
#[utoipa::path(
    post,
    path = "",
    summary = "Create a goal",
    request_body(
        content = CreateGoalRequest,
        content_type = "application/json",
        example = json!({"name": "Emergency fund", "target_amount": 5000.0, "target_date": "2030-01-01", "account_id": "plaid-account-id"})
    ),
    responses(
        (status = 201, description = "Goal created"),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Linked account not found")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_create_goal(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<CreateGoalRequest>,
) -> ApiResult<StatusCode> {
    debug!("HANDLER ->> POST /api/goals user_id={}", user.id);

    payload.validate(today()).map_err(AppError::Validation)?;

    let mut tx = pool.begin().await.map_err(AppError::from)?;

    let balance = owned_depository_balance(&mut tx, user.id, &payload.account_id).await?;
    let baseline = payload.baseline_balance.unwrap_or(balance.unwrap_or(0.0));
    check_target_above_baseline(payload.target_amount, baseline)?;

    sqlx::query!(
        r#"
        INSERT INTO goals (user_id, account_id, name, description, target_amount, baseline_balance, target_date)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        user.id,
        payload.account_id,
        payload.name.trim(),
        payload
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty()),
        payload.target_amount,
        baseline,
        payload.target_date
    )
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    tx.commit().await.map_err(AppError::from)?;

    Ok(StatusCode::CREATED)
}

/// List the user's goals, optionally filtered by derived status.
///
/// `GET /api/goals`
#[utoipa::path(
    get,
    path = "",
    summary = "List goals",
    params(("status" = Option<String>, Query, description = "ACTIVE, COMPLETED or ARCHIVED")),
    responses(
        (status = 200, description = "Goals returned", body = GoalsResponse),
        (status = 400, description = "Invalid status filter"),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_list_goals(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Query(query): Query<GoalsQuery>,
) -> ApiResult<Json<GoalsResponse>> {
    debug!("HANDLER ->> GET /api/goals user_id={}", user.id);

    let filter = query
        .status
        .as_deref()
        .map(|s| {
            GoalStatus::parse(s).ok_or_else(|| {
                AppError::Validation("status must be ACTIVE, COMPLETED or ARCHIVED".to_string())
            })
        })
        .transpose()?;

    let rows = sqlx::query_as!(
        GoalRow,
        r#"
        SELECT g.id, g.user_id, g.account_id, g.name, g.description, g.target_amount,
               g.baseline_balance, g.target_date, g.archived, g.created_at, g.updated_at,
               a.current_balance AS account_balance
        FROM goals g
        JOIN plaid_accounts a ON a.account_id = g.account_id
        WHERE g.user_id = $1
        ORDER BY g.created_at DESC, g.id DESC
        "#,
        user.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    // Status is derived, so it is filtered after computing each response.
    let now = today();
    let goals = rows
        .iter()
        .map(|r| GoalResponse::new(r, now))
        .filter(|g| filter.map_or(true, |f| g.status == f))
        .collect();

    Ok(Json(GoalsResponse { goals }))
}

/// Fetch one goal with derived progress.
///
/// `GET /api/goals/{id}`
#[utoipa::path(
    get,
    path = "/{id}",
    summary = "Get a goal",
    params(("id" = i32, Path, description = "Goal ID")),
    responses(
        (status = 200, description = "Goal returned", body = GoalResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Goal not found")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_get_goal(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<Json<GoalResponse>> {
    debug!("HANDLER ->> GET /api/goals/{id} user_id={}", user.id);

    let row = sqlx::query_as!(
        GoalRow,
        r#"
        SELECT g.id, g.user_id, g.account_id, g.name, g.description, g.target_amount,
               g.baseline_balance, g.target_date, g.archived, g.created_at, g.updated_at,
               a.current_balance AS account_balance
        FROM goals g
        JOIN plaid_accounts a ON a.account_id = g.account_id
        WHERE g.id = $1 AND g.user_id = $2
        "#,
        id,
        user.id
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(GoalResponse::new(&row, today())))
}

/// Update a goal.
///
/// Switching to a different account (or passing an explicit `baseline_balance`) re-baselines
/// the goal, so progress restarts from the new starting balance.
///
/// `PUT /api/goals/{id}`
#[utoipa::path(
    put,
    path = "/{id}",
    summary = "Update a goal",
    params(("id" = i32, Path, description = "Goal ID")),
    request_body(
        content = UpdateGoalRequest,
        content_type = "application/json",
        description = "All fields optional; `null` clears description and target_date.",
        example = json!({"target_amount": 6000.0, "archived": false})
    ),
    responses(
        (status = 200, description = "Goal updated", body = GoalResponse),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Goal or account not found")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_update_goal(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateGoalRequest>,
) -> ApiResult<Json<GoalResponse>> {
    debug!("HANDLER ->> PUT /api/goals/{id} user_id={}", user.id);

    payload.validate(today()).map_err(AppError::Validation)?;

    let mut tx = pool.begin().await.map_err(AppError::from)?;

    // Read the goal and lock its row for the rest of the transaction.
    let current = sqlx::query_as!(
        GoalRow,
        r#"
        SELECT g.id, g.user_id, g.account_id, g.name, g.description, g.target_amount,
               g.baseline_balance, g.target_date, g.archived, g.created_at, g.updated_at,
               a.current_balance AS account_balance
        FROM goals g
        JOIN plaid_accounts a ON a.account_id = g.account_id
        WHERE g.id = $1 AND g.user_id = $2
        FOR UPDATE OF g
        "#,
        id,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    // A different account, or the same one with an explicit baseline, re-baselines the goal.
    let relink = match &payload.account_id {
        Some(account) if *account != current.account_id || payload.baseline_balance.is_some() => {
            let balance = owned_depository_balance(&mut tx, user.id, account).await?;
            let baseline = payload.baseline_balance.unwrap_or(balance.unwrap_or(0.0));
            Some((account.clone(), baseline))
        }
        _ => None,
    };

    let new_baseline = relink
        .as_ref()
        .map_or(current.baseline_balance, |(_, baseline)| *baseline);
    let new_target = payload.target_amount.unwrap_or(current.target_amount);
    check_target_above_baseline(new_target, new_baseline)?;

    let set_account = relink.is_some();
    let (new_account, _) = relink.unwrap_or_default();

    let set_description = payload.description.is_some();
    let new_description = payload
        .description
        .flatten()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    let set_target_date = payload.target_date.is_some();
    let new_target_date = payload.target_date.flatten();

    let row = sqlx::query_as!(
        GoalRow,
        r#"
        UPDATE goals SET
            name             = COALESCE($3, name),
            description      = CASE WHEN $4 THEN $5 ELSE description END,
            target_amount    = $6,
            target_date      = CASE WHEN $7 THEN $8 ELSE target_date END,
            account_id       = CASE WHEN $9 THEN $10 ELSE account_id END,
            baseline_balance = $11,
            archived         = COALESCE($12, archived),
            updated_at       = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, account_id, name, description, target_amount,
                  baseline_balance, target_date, archived, created_at, updated_at,
                  (SELECT a.current_balance FROM plaid_accounts a
                   WHERE a.account_id = goals.account_id) AS account_balance
        "#,
        id,
        user.id,
        payload.name.as_deref().map(str::trim),
        set_description,
        new_description,
        new_target,
        set_target_date,
        new_target_date,
        set_account,
        new_account,
        new_baseline,
        payload.archived
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from)?;

    tx.commit().await.map_err(AppError::from)?;

    Ok(Json(GoalResponse::new(&row, today())))
}

/// Delete a goal.
///
/// `DELETE /api/goals/{id}`
#[utoipa::path(
    delete,
    path = "/{id}",
    summary = "Delete a goal",
    params(("id" = i32, Path, description = "Goal ID")),
    responses(
        (status = 204, description = "Goal deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Goal not found")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_delete_goal(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    debug!("HANDLER ->> DELETE /api/goals/{id} user_id={}", user.id);

    let result = sqlx::query!(
        "DELETE FROM goals WHERE id = $1 AND user_id = $2",
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

/// `OpenAPI` doc for goal routes.
///
/// Constructed only by the `#[openapi]` macro and the root [`crate::swagger::ApiDoc`].
#[derive(OpenApi)]
#[openapi(
    paths(
        api_create_goal,
        api_list_goals,
        api_get_goal,
        api_update_goal,
        api_delete_goal
    ),
    components(schemas(
        CreateGoalRequest,
        UpdateGoalRequest,
        GoalResponse,
        GoalsResponse,
        GoalStatus
    )),
    info(title = "Goal Routes", description = "Account-balance goals."),
    tags((name = "Goals"))
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct GoalApiDoc;

/// Builds the goal route group. All routes require a valid `auth-token` cookie.
pub fn goal_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/", get(api_list_goals).post(api_create_goal))
        .route(
            "/{id}",
            get(api_get_goal)
                .put(api_update_goal)
                .delete(api_delete_goal),
        )
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
