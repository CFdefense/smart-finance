//! Goal endpoints — savings goals with manual contributions and optional account tracking.

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get},
    Extension, Json,
};
use chrono::{NaiveDate, Utc};
use sqlx::{PgConnection, PgPool};
use tracing::debug;
use utoipa::OpenApi;

use crate::{
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::{
        common::round_cents,
        http::goal::{
            ContributionCreatedResponse, ContributionResponse, ContributionsResponse,
            CreateContributionRequest, CreateGoalRequest, GoalResponse, GoalStatus, GoalsQuery,
            GoalsResponse, UpdateGoalRequest,
        },
        sql::goal::{ContributionRow, GoalRow},
    },
};

/// Tolerance when checking that the manual total stays non-negative (half a cent).
const EPSILON: f64 = 0.005;

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

/// Loads one goal (with derived inputs) owned by `user_id`.
async fn fetch_goal(conn: &mut PgConnection, id: i32, user_id: i32) -> ApiResult<GoalRow> {
    sqlx::query_as!(
        GoalRow,
        r#"
        SELECT g.id, g.user_id, g.name, g.description, g.target_amount, g.target_date,
               g.account_id, g.baseline_balance, g.archived, g.created_at, g.updated_at,
               COALESCE((SELECT SUM(c.amount) FROM goal_contributions c WHERE c.goal_id = g.id), 0)
                   AS "manual_total!",
               a.current_balance AS account_balance
        FROM goals g
        LEFT JOIN plaid_accounts a ON a.account_id = g.account_id
        WHERE g.id = $1 AND g.user_id = $2
        "#,
        id,
        user_id
    )
    .fetch_optional(conn)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)
}

/// Locks the goal row for the rest of the transaction, or `NotFound`.
async fn lock_goal(conn: &mut PgConnection, id: i32, user_id: i32) -> ApiResult<()> {
    sqlx::query_scalar!(
        "SELECT id FROM goals WHERE id = $1 AND user_id = $2 FOR UPDATE",
        id,
        user_id
    )
    .fetch_optional(conn)
    .await
    .map_err(AppError::from)?
    .map(|_| ())
    .ok_or(AppError::NotFound)
}

/// Sum of manual contributions for a goal.
async fn manual_total(conn: &mut PgConnection, goal_id: i32) -> ApiResult<f64> {
    let total = sqlx::query_scalar!(
        r#"SELECT COALESCE(SUM(amount), 0) AS "total!" FROM goal_contributions WHERE goal_id = $1"#,
        goal_id
    )
    .fetch_one(conn)
    .await
    .map_err(AppError::from)?;
    Ok(total)
}

/// Verifies `account_id` belongs to the user and is a depository account.
///
/// Returns its current balance (0 when Plaid reported none). Foreign or unknown accounts
/// are `NotFound` so existence is not leaked; other account types are a validation error.
async fn owned_depository_balance(
    conn: &mut PgConnection,
    user_id: i32,
    account_id: &str,
) -> ApiResult<f64> {
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
    Ok(account.current_balance.unwrap_or(0.0))
}

/// Create a goal, optionally tracking a linked account.
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
        (status = 201, description = "Goal created", body = GoalResponse),
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
) -> ApiResult<(StatusCode, Json<GoalResponse>)> {
    debug!("HANDLER ->> POST /api/goals user_id={}", user.id);
    let now = today();
    payload.validate(now).map_err(AppError::Validation)?;

    let mut tx = pool.begin().await.map_err(AppError::from)?;

    // Baseline is only meaningful with a linked account.
    let baseline = match &payload.account_id {
        Some(account_id) => {
            let balance = owned_depository_balance(&mut tx, user.id, account_id).await?;
            Some(payload.baseline_balance.unwrap_or(balance))
        }
        None => None,
    };

    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO goals (user_id, name, description, target_amount, target_date, account_id, baseline_balance)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        user.id,
        payload.name.trim(),
        payload
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty()),
        payload.target_amount,
        payload.target_date,
        payload.account_id,
        baseline
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from)?;

    let row = fetch_goal(&mut tx, id, user.id).await?;
    tx.commit().await.map_err(AppError::from)?;
    Ok((StatusCode::CREATED, Json(GoalResponse::new(&row, now))))
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
    let filter = match query.status.as_deref() {
        None => None,
        Some(s) => Some(GoalStatus::parse(s).ok_or_else(|| {
            AppError::Validation("status must be ACTIVE, COMPLETED or ARCHIVED".to_string())
        })?),
    };

    let rows = sqlx::query_as!(
        GoalRow,
        r#"
        SELECT g.id, g.user_id, g.name, g.description, g.target_amount, g.target_date,
               g.account_id, g.baseline_balance, g.archived, g.created_at, g.updated_at,
               COALESCE((SELECT SUM(c.amount) FROM goal_contributions c WHERE c.goal_id = g.id), 0)
                   AS "manual_total!",
               a.current_balance AS account_balance
        FROM goals g
        LEFT JOIN plaid_accounts a ON a.account_id = g.account_id
        WHERE g.user_id = $1
        ORDER BY g.created_at DESC, g.id DESC
        "#,
        user.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

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
    let mut conn = pool.acquire().await.map_err(AppError::from)?;
    let row = fetch_goal(&mut conn, id, user.id).await?;
    Ok(Json(GoalResponse::new(&row, today())))
}

/// Update a goal.
///
/// Changing or removing the linked account first settles progress earned so far into a
/// contribution, so switching accounts never loses or double-counts progress.
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
        description = "All fields optional; `null` clears description, target_date and account_id.",
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
    let now = today();
    payload.validate(now).map_err(AppError::Validation)?;

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    lock_goal(&mut tx, id, user.id).await?;
    let current = fetch_goal(&mut tx, id, user.id).await?;

    // Work out what happens to the linked account.
    let mut set_account = false;
    let mut new_account: Option<String> = None;
    let mut new_baseline: Option<f64> = None;
    match &payload.account_id {
        // Unlink (only does something if currently linked).
        Some(None) => {
            set_account = current.account_id.is_some();
        }
        // Link: a different account, or the same one with an explicit new baseline.
        Some(Some(account)) => {
            let changed = current.account_id.as_deref() != Some(account.as_str());
            if changed || payload.baseline_balance.is_some() {
                let balance = owned_depository_balance(&mut tx, user.id, account).await?;
                new_baseline = Some(payload.baseline_balance.unwrap_or(balance));
                new_account = Some(account.clone());
                set_account = true;
            }
        }
        None => {}
    }

    // Settle growth earned on the old account before moving the baseline.
    if set_account {
        if let (Some(_), Some(balance), Some(baseline)) = (
            &current.account_id,
            current.account_balance,
            current.baseline_balance,
        ) {
            let growth = round_cents(balance - baseline);
            if growth != 0.0 {
                sqlx::query!(
                    "INSERT INTO goal_contributions (goal_id, amount, contribution_date, note)
                     VALUES ($1, $2, $3, 'Settled from linked account')",
                    id,
                    growth,
                    now
                )
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;
            }
        }
    }

    let set_description = payload.description.is_some();
    let new_description = payload
        .description
        .flatten()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    let set_target_date = payload.target_date.is_some();
    let new_target_date = payload.target_date.flatten();

    sqlx::query!(
        r#"
        UPDATE goals SET
            name             = COALESCE($3, name),
            description      = CASE WHEN $4 THEN $5 ELSE description END,
            target_amount    = COALESCE($6, target_amount),
            target_date      = CASE WHEN $7 THEN $8 ELSE target_date END,
            account_id       = CASE WHEN $9 THEN $10 ELSE account_id END,
            baseline_balance = CASE WHEN $9 THEN $11 ELSE baseline_balance END,
            archived         = COALESCE($12, archived),
            updated_at       = NOW()
        WHERE id = $1 AND user_id = $2
        "#,
        id,
        user.id,
        payload.name.as_deref().map(str::trim),
        set_description,
        new_description,
        payload.target_amount,
        set_target_date,
        new_target_date,
        set_account,
        new_account,
        new_baseline,
        payload.archived
    )
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    let row = fetch_goal(&mut tx, id, user.id).await?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(GoalResponse::new(&row, now)))
}

/// Delete a goal and its contributions.
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

/// Record a deposit (positive) or withdrawal (negative) on a goal.
///
/// A withdrawal cannot take the manual total below zero (400). Archived goals reject
/// contributions (409). The goal's status flips to/from `COMPLETED` automatically.
///
/// `POST /api/goals/{id}/contributions`
#[utoipa::path(
    post,
    path = "/{id}/contributions",
    summary = "Add a contribution to a goal",
    params(("id" = i32, Path, description = "Goal ID")),
    request_body(
        content = CreateContributionRequest,
        content_type = "application/json",
        example = json!({"amount": 250.0, "note": "Birthday money"})
    ),
    responses(
        (status = 201, description = "Contribution recorded", body = ContributionCreatedResponse),
        (status = 400, description = "Validation failure or withdrawal exceeds contributions"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Goal not found"),
        (status = 409, description = "Goal is archived")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_add_contribution(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
    Json(payload): Json<CreateContributionRequest>,
) -> ApiResult<(StatusCode, Json<ContributionCreatedResponse>)> {
    debug!(
        "HANDLER ->> POST /api/goals/{id}/contributions user_id={}",
        user.id
    );
    let now = today();
    payload.validate(now).map_err(AppError::Validation)?;

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    lock_goal(&mut tx, id, user.id).await?;
    let goal = fetch_goal(&mut tx, id, user.id).await?;
    if goal.archived {
        return Err(AppError::Conflict(
            "archived goals cannot receive contributions".to_string(),
        ));
    }

    let amount = round_cents(payload.amount);
    if amount < 0.0 && goal.manual_total + amount < -EPSILON {
        return Err(AppError::Validation(
            "withdrawal exceeds the contributions made to this goal".to_string(),
        ));
    }

    let row = sqlx::query_as!(
        ContributionRow,
        r#"
        INSERT INTO goal_contributions (goal_id, amount, contribution_date, note)
        VALUES ($1, $2, $3, $4)
        RETURNING id, goal_id, amount, contribution_date, note, created_at
        "#,
        id,
        amount,
        payload.date.unwrap_or(now),
        payload
            .note
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from)?;

    let updated = fetch_goal(&mut tx, id, user.id).await?;
    tx.commit().await.map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(ContributionCreatedResponse {
            contribution: row.into(),
            goal: GoalResponse::new(&updated, now),
        }),
    ))
}

/// List a goal's contributions, newest first.
///
/// `GET /api/goals/{id}/contributions`
#[utoipa::path(
    get,
    path = "/{id}/contributions",
    summary = "List a goal's contributions",
    params(("id" = i32, Path, description = "Goal ID")),
    responses(
        (status = 200, description = "Contributions returned", body = ContributionsResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Goal not found")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_list_contributions(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<Json<ContributionsResponse>> {
    debug!(
        "HANDLER ->> GET /api/goals/{id}/contributions user_id={}",
        user.id
    );
    let mut conn = pool.acquire().await.map_err(AppError::from)?;
    fetch_goal(&mut conn, id, user.id).await?;

    let rows = sqlx::query_as!(
        ContributionRow,
        "SELECT id, goal_id, amount, contribution_date, note, created_at
         FROM goal_contributions WHERE goal_id = $1
         ORDER BY contribution_date DESC, id DESC",
        id
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::from)?;

    Ok(Json(ContributionsResponse {
        contributions: rows.into_iter().map(ContributionResponse::from).collect(),
    }))
}

/// Delete a contribution (undo a mistake).
///
/// Deleting a deposit that later withdrawals depend on is rejected (409), because it
/// would leave the manual total negative.
///
/// `DELETE /api/goals/{id}/contributions/{cid}`
#[utoipa::path(
    delete,
    path = "/{id}/contributions/{cid}",
    summary = "Delete a contribution",
    params(
        ("id" = i32, Path, description = "Goal ID"),
        ("cid" = i32, Path, description = "Contribution ID")
    ),
    responses(
        (status = 200, description = "Contribution deleted; updated goal returned", body = GoalResponse),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Goal or contribution not found"),
        (status = 409, description = "Deleting would leave the contributions negative")
    ),
    security(("auth-token" = [])),
    tag = "Goals"
)]
pub async fn api_delete_contribution(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path((id, cid)): Path<(i32, i32)>,
) -> ApiResult<Json<GoalResponse>> {
    debug!(
        "HANDLER ->> DELETE /api/goals/{id}/contributions/{cid} user_id={}",
        user.id
    );
    let now = today();
    let mut tx = pool.begin().await.map_err(AppError::from)?;
    lock_goal(&mut tx, id, user.id).await?;

    let amount = sqlx::query_scalar!(
        "SELECT amount FROM goal_contributions WHERE id = $1 AND goal_id = $2",
        cid,
        id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    let total = manual_total(&mut tx, id).await?;
    if amount > 0.0 && total - amount < -EPSILON {
        return Err(AppError::Conflict(
            "deleting this deposit would leave withdrawals uncovered".to_string(),
        ));
    }

    sqlx::query!("DELETE FROM goal_contributions WHERE id = $1", cid)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

    let row = fetch_goal(&mut tx, id, user.id).await?;
    tx.commit().await.map_err(AppError::from)?;
    Ok(Json(GoalResponse::new(&row, now)))
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
        api_delete_goal,
        api_add_contribution,
        api_list_contributions,
        api_delete_contribution
    ),
    components(schemas(
        CreateGoalRequest,
        UpdateGoalRequest,
        CreateContributionRequest,
        GoalResponse,
        GoalsResponse,
        GoalStatus,
        ContributionResponse,
        ContributionsResponse,
        ContributionCreatedResponse
    )),
    info(title = "Goal Routes", description = "Savings goals and contributions."),
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
        .route(
            "/{id}/contributions",
            get(api_list_contributions).post(api_add_contribution),
        )
        .route("/{id}/contributions/{cid}", delete(api_delete_contribution))
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
