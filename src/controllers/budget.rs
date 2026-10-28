//! Budget endpoints — recurring monthly spending limits per category.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
    Extension, Json,
};
use chrono::Utc;
use sqlx::PgPool;
use tracing::debug;
use utoipa::OpenApi;

use crate::{
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::{
        category::Category,
        http::budget::{
            parse_month, BudgetQuery, BudgetResponse, BudgetsResponse, CreateBudgetRequest,
            MonthWindow, UpdateBudgetRequest,
        },
        sql::budget::BudgetRow,
    },
};

/// Sums money out (positive amounts) per category for the user within `window`.
async fn spent_by_category(
    pool: &PgPool,
    user_id: i32,
    window: &MonthWindow,
) -> ApiResult<HashMap<Category, f64>> {
    let rows = sqlx::query!(
        r#"
        SELECT t.category AS "category!", COALESCE(SUM(t.amount), 0) AS "spent!"
        FROM plaid_transactions t
        JOIN plaid_accounts a ON a.account_id = t.account_id
        JOIN plaid_links i ON i.item_id = a.item_id
        WHERE i.user_id = $1
          AND t.amount > 0
          AND t.date >= $2
          AND t.date < $3
        GROUP BY t.category
        "#,
        user_id,
        window.start,
        window.end
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::from)?;

    rows.into_iter()
        .map(|r| {
            let category = r
                .category
                .parse::<Category>()
                .map_err(|_| AppError::Internal("Failed to parse category".to_string()))?;

            Ok((category, r.spent))
        })
        .collect()
}

/// Create a monthly budget for a category.
///
/// Each user may have one budget per category; a second one returns 409.
///
/// `POST /api/budgets`
#[utoipa::path(
    post,
    path = "",
    summary = "Create a monthly budget",
    request_body(
        content = CreateBudgetRequest,
        content_type = "application/json",
        example = json!({"name": "Eating out", "category": "FOOD_DINING", "monthly_limit": 400.0})
    ),
    responses(
        (status = 201, description = "Budget created"),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated"),
        (status = 409, description = "A budget for this category already exists")
    ),
    security(("auth-token" = [])),
    tag = "Budgets"
)]
pub async fn api_create_budget(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<CreateBudgetRequest>,
) -> ApiResult<StatusCode> {
    debug!("HANDLER ->> POST /api/budgets user_id={}", user.id);
    payload.validate().map_err(AppError::Validation)?;

    // Create the budget
    let result = sqlx::query_as!(
        BudgetRow,
        r#"
        INSERT INTO budgets (user_id, name, category, monthly_limit, active)
        VALUES ($1, $2, $3, $4, COALESCE($5, TRUE))
        RETURNING id, user_id, name, category, monthly_limit, active, created_at, updated_at
        "#,
        user.id,
        payload.name.trim(),
        payload.category.as_str(),
        payload.monthly_limit,
        payload.active
    )
    .fetch_one(&pool)
    .await;

    // This table enforces uniqueness by category, so sqlx::Error::Database returned if duplicate
    match result {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => Err(
            AppError::Conflict("Email address already in use".to_string()),
        ),
        Err(e) => Err(AppError::from(e)),
    }
}

/// List the user's budgets with spending for a month.
///
/// `GET /api/budgets`
#[utoipa::path(
    get,
    path = "",
    summary = "List budgets with spending for a month",
    params(("month" = Option<String>, Query, description = "Month as YYYY-MM; defaults to the current month")),
    responses(
        (status = 200, description = "Budgets returned", body = BudgetsResponse),
        (status = 400, description = "Invalid month"),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "Budgets"
)]
pub async fn api_list_budgets(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Query(query): Query<BudgetQuery>,
) -> ApiResult<Json<BudgetsResponse>> {
    debug!("HANDLER ->> GET /api/budgets user_id={}", user.id);

    // Validate the month
    let window = parse_month(query.month.as_deref(), Utc::now().date_naive())
        .map_err(AppError::Validation)?;

    // Select budget from the database
    let rows = sqlx::query_as!(
        BudgetRow,
        "SELECT id, user_id, name, category, monthly_limit, active, created_at, updated_at
         FROM budgets WHERE user_id = $1 ORDER BY category",
        user.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    // Calculate spent
    let spent = spent_by_category(&pool, user.id, &window).await?;
    let budgets = rows
        .iter()
        .map(|r| {
            let category = Category::parse(&r.category);

            BudgetResponse::new(
                r,
                &window,
                category
                    .as_ref()
                    .and_then(|c| spent.get(c))
                    .copied()
                    .unwrap_or(0.0),
            )
        })
        .collect();

    Ok(Json(BudgetsResponse {
        month: window.label(),
        budgets,
    }))
}

/// Fetch one budget with spending for a month.
///
/// `GET /api/budgets/{id}`
#[utoipa::path(
    get,
    path = "/{id}",
    summary = "Get a budget",
    params(
        ("id" = i32, Path, description = "Budget ID"),
        ("month" = Option<String>, Query, description = "Month as YYYY-MM; defaults to the current month")
    ),
    responses(
        (status = 200, description = "Budget returned", body = BudgetResponse),
        (status = 400, description = "Invalid month"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Budget not found")
    ),
    security(("auth-token" = [])),
    tag = "Budgets"
)]
pub async fn api_get_budget(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
    Query(query): Query<BudgetQuery>,
) -> ApiResult<Json<BudgetResponse>> {
    debug!("HANDLER ->> GET /api/budgets/{id} user_id={}", user.id);

    // Parse the month
    let window = parse_month(query.month.as_deref(), Utc::now().date_naive())
        .map_err(AppError::Validation)?;

    // Look up the budget id in the database
    let row = sqlx::query_as!(
        BudgetRow,
        "SELECT id, user_id, name, category, monthly_limit, active, created_at, updated_at
         FROM budgets WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    // Calculate spending across the user's accounts for the requested month
    let spent = spent_by_category(&pool, user.id, &window).await?;
    let category = Category::parse(&row.category);

    let spent_amount = category
        .as_ref()
        .and_then(|c| spent.get(c))
        .copied()
        .unwrap_or(0.0);

    Ok(Json(BudgetResponse::new(&row, &window, spent_amount)))
}

/// Update a budget's name, limit or active flag.
///
/// The category cannot be changed; delete and recreate the budget instead.
///
/// `PUT /api/budgets/{id}`
#[utoipa::path(
    put,
    path = "/{id}",
    summary = "Update a budget",
    params(("id" = i32, Path, description = "Budget ID")),
    request_body(
        content = UpdateBudgetRequest,
        content_type = "application/json",
        description = "All fields optional; at least one is required.",
        example = json!({"monthly_limit": 450.0})
    ),
    responses(
        (status = 200, description = "Budget updated", body = BudgetResponse),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Budget not found")
    ),
    security(("auth-token" = [])),
    tag = "Budgets"
)]
pub async fn api_update_budget(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateBudgetRequest>,
) -> ApiResult<Json<BudgetResponse>> {
    debug!("HANDLER ->> PUT /api/budgets/{id} user_id={}", user.id);
    payload.validate().map_err(AppError::Validation)?;

    // Attempt update
    let row = sqlx::query_as!(
        BudgetRow,
        r#"
        UPDATE budgets SET
            name          = COALESCE($3, name),
            monthly_limit = COALESCE($4, monthly_limit),
            active        = COALESCE($5, active),
            updated_at    = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, name, category, monthly_limit, active,
                  created_at, updated_at
        "#,
        id,
        user.id,
        payload.name.as_deref().map(str::trim),
        payload.monthly_limit,
        payload.active
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::from)?
    .ok_or(AppError::NotFound)?;

    // Validate the month
    let window = parse_month(None, Utc::now().date_naive()).map_err(AppError::Validation)?;

    // Calculate spending across the user's accounts for the requested month
    let spent = spent_by_category(&pool, user.id, &window).await?;
    let category = Category::parse(&row.category);

    let spent_amount = category
        .as_ref()
        .and_then(|c| spent.get(c))
        .copied()
        .unwrap_or(0.0);

    Ok(Json(BudgetResponse::new(&row, &window, spent_amount)))
}

/// Delete a budget.
///
/// `DELETE /api/budgets/{id}`
#[utoipa::path(
    delete,
    path = "/{id}",
    summary = "Delete a budget",
    params(("id" = i32, Path, description = "Budget ID")),
    responses(
        (status = 204, description = "Budget deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Budget not found")
    ),
    security(("auth-token" = [])),
    tag = "Budgets"
)]
pub async fn api_delete_budget(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    debug!("HANDLER ->> DELETE /api/budgets/{id} user_id={}", user.id);

    // Attempt delete
    let result = sqlx::query!(
        "DELETE FROM budgets WHERE id = $1 AND user_id = $2",
        id,
        user.id
    )
    .execute(&pool)
    .await
    .map_err(AppError::from)?;

    // Error if we failed to find the item to delete
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `OpenAPI` doc for budget routes.
///
/// Constructed only by the `#[openapi]` macro and the root [`crate::swagger::ApiDoc`].
#[derive(OpenApi)]
#[openapi(
    paths(
        api_create_budget,
        api_list_budgets,
        api_get_budget,
        api_update_budget,
        api_delete_budget
    ),
    components(schemas(
        CreateBudgetRequest,
        UpdateBudgetRequest,
        BudgetResponse,
        BudgetsResponse,
        Category
    )),
    info(title = "Budget Routes", description = "Monthly category budgets."),
    tags((name = "Budgets"))
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct BudgetApiDoc;

/// Builds the budget route group. All routes require a valid `auth-token` cookie.
pub fn budget_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/", get(api_list_budgets).post(api_create_budget))
        .route(
            "/{id}",
            get(api_get_budget)
                .put(api_update_budget)
                .delete(api_delete_budget),
        )
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
