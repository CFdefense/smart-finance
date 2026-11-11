//! Account-related endpoints and net-worth persistence.
//!
//! Plaid retrieves live balances, while this module owns
//! the application's historical financial account data.

use axum::{routing::get, Extension, Json};
use sqlx::PgPool;
use tracing::debug;
use utoipa::OpenApi;

use crate::{
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::http::{
        account::{NetWorthHistoryResponse, NetWorthSnapshot},
        plaid::AccountResponse,
    },
};

/// Record today's linked-account net worth for one authenticated user.
///
/// This helper is called only after accounts have been
/// successfully retrieved from Plaid.
///
/// Calculation rules:
/// - Depository and investment balances count as assets.
/// - Credit and loan balances count as liabilities.
/// - Missing balances, unsupported types, and non-USD
///   accounts are excluded rather than guessed.
///
/// We use the same rules as the Angular net-worth overview.
///
/// Every dashboard refresh updates the current day's row.
/// Older dates remain unchanged so they can form a graph.
pub(crate) async fn record_net_worth_snapshot(
    pool: &PgPool,
    user_id: i32,
    accounts: &[AccountResponse],
) -> ApiResult<()> {
    let mut assets = 0.0_f64;
    let mut liabilities = 0.0_f64;
    let mut included = 0_i32;
    let mut excluded = 0_i32;

    // Calculate the current linked-account totals.
    for account in accounts {
        let Some(balance) = account.current_balance else {
            excluded += 1;
            continue;
        };

        // Reject invalid numeric values.
        if !balance.is_finite() {
            excluded += 1;
            continue;
        }

        // Net-worth snapshots currently use USD only.
        if account.currency_code.as_deref() != Some("USD") {
            excluded += 1;
            continue;
        }

        match account.account_type.to_lowercase().as_str() {
            "depository" | "investment" => {
                assets += balance;
                included += 1;
            }
            "credit" | "loan" => {
                liabilities += balance;
                included += 1;
            }
            _ => {
                excluded += 1;
            }
        }
    }

    // Do not record a fake zero if there are no valid balances.
    if included == 0 {
        return Ok(());
    }

    let net_worth = assets - liabilities;

    // Protect the database against numeric overflow.
    if !net_worth.is_finite() {
        return Err(AppError::Validation(
            "Calculated net worth is not finite".to_string(),
        ));
    }

    // Insert today's snapshot or update it if one already exists.
    //
    // CURRENT_DATE uses the database server's calendar date.
    // No fabricated historical values are inserted.
    sqlx::query(
        r"
        INSERT INTO net_worth_snapshots
            (user_id, snapshot_date, amount,
             included_accounts, excluded_accounts)
        VALUES ($1, CURRENT_DATE, $2, $3, $4)
        ON CONFLICT (user_id, snapshot_date)
        DO UPDATE SET
            amount = EXCLUDED.amount,
            included_accounts = EXCLUDED.included_accounts,
            excluded_accounts = EXCLUDED.excluded_accounts,
            updated_at = NOW()
        ",
    )
    .bind(user_id)
    .bind(net_worth)
    .bind(included)
    .bind(excluded)
    .execute(pool)
    .await
    .map_err(AppError::from)?;

    Ok(())
}

/// Retrieve historical net-worth snapshots for the signed-in user.
///
/// Only previously recorded daily values are returned.
/// This endpoint never guesses past account balances.
///
/// GET /api/accounts/net-worth/history
#[utoipa::path(
    get,
    path = "/net-worth/history",
    summary = "Retrieves the user's recorded net-worth history",
    description = "Returns daily linked-account net-worth snapshots in chronological order.",
    responses(
        (status = 200, description = "Net-worth history retrieved", body = NetWorthHistoryResponse),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Failed to retrieve net-worth history")
    ),
    tag = "Account"
)]
pub async fn api_get_net_worth_history(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
) -> ApiResult<Json<NetWorthHistoryResponse>> {
    debug!(
        "HANDLER ->> GET /api/accounts/net-worth/history user_id={}",
        user.id
    );

    // Retrieve only snapshots belonging to the signed-in user.
    //
    // The is_demo flag allows Angular to distinguish
    // simulated sandbox history from recorded balances.
    let rows: Vec<(chrono::NaiveDate, f64, i32, i32, bool)> = sqlx::query_as(
        r"
            SELECT
                snapshot_date,
                amount,
                included_accounts,
                excluded_accounts,
                is_demo
            FROM net_worth_snapshots
            WHERE user_id = $1
            ORDER BY snapshot_date ASC
            ",
    )
    .bind(user.id)
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    // Map database tuples into our typed API objects.
    let snapshots = rows
        .into_iter()
        .map(
            |(snapshot_date, amount, included_accounts, excluded_accounts, is_demo)| {
                NetWorthSnapshot {
                    snapshot_date,
                    amount,
                    included_accounts,
                    excluded_accounts,
                    is_demo,
                }
            },
        )
        .collect();

    Ok(Json(NetWorthHistoryResponse { snapshots }))
}

/// `OpenAPI` documentation for account endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(api_get_net_worth_history),
    components(schemas(NetWorthHistoryResponse, NetWorthSnapshot)),
    info(
        title = "Account Routes",
        description = "Account financial history."
    ),
    tags((name = "Account"))
)]
#[allow(dead_code)]
pub struct AccountApiDoc;

/// Builds authenticated account routes.
///
/// These routes operate on financial account information
/// belonging to the currently signed-in user.
pub fn account_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/net-worth/history", get(api_get_net_worth_history))
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
