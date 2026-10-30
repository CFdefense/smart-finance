//! Plaid Financial Service Endpoints

use axum::{
    extract::{Path, Query},
    routing::{get, post, put},
    Extension, Json,
};
use sqlx::PgPool;
use tracing::debug;
use utoipa::OpenApi;

use crate::{
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::{
        http::plaid::{
            AccountResponse, AccountsResponse, ExchangeTokenRequest, ExchangeTokenResponse,
            LinkTokenResponse, TransactionResponse, TransactionSyncResponse, TransactionsQuery,
            TransactionsResponse, UpdateCategoryRequest,
        },
        sql::plaid::{PlaidLinkRow, PlaidSyncCursorRow},
    },
    plaid::client::PlaidClient,
};

/// Create a Plaid Link token.
///
/// Generates a short-lived Link token for the authenticated user to initialize
/// Plaid Link and begin connecting a financial institution.
///
/// The token is returned to the frontend, which uses it to launch the Plaid Link
/// UI. The backend configures the supported products, country codes, and webhook
/// URL when creating the token with Plaid.
///
/// The authenticated user's ID is used as the Plaid `client_user_id` to associate
/// the linking session with the correct application user.
///
/// `POST /api/plaid/link-token`
#[utoipa::path(
    post,
    path = "/link-token",
    summary = "Creates a Plaid Link token to begin bank linking",
    description = "Creates a Link token for the authenticated user. The frontend uses this token to initialize Plaid Link and allow the user to select and connect their financial institution.",
    responses(
        (status = 200, description = "Link token created successfully", body = LinkTokenResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Plaid"
)]
pub async fn api_plaid_link_token(
    Extension(user): Extension<AuthUser>,
    Extension(plaid): Extension<PlaidClient>,
) -> ApiResult<Json<LinkTokenResponse>> {
    debug!("HANDLER ->> POST /api/plaid/link-token user_id={}", user.id);

    // Ask Plaid for a short-lived Link token tied to this user's id.
    let link_token = plaid.create_link_token(user.id).await?;

    // Hand the token to the frontend so it can launch Plaid Link.
    Ok(Json(LinkTokenResponse { link_token }))
}

/// Exchange a Plaid public token.
///
/// Exchanges the temporary public token returned by Plaid Link for a Plaid
/// access token and Item ID.
///
/// The access token is securely stored and associated with the authenticated
/// user so the backend can retrieve financial data and process webhooks for
/// the linked financial institution. The access token is never returned to
/// the frontend.
///
/// An initial data synchronization can be scheduled after the connection is
/// saved successfully.
///
/// `POST /api/plaid/exchange-token`
#[utoipa::path(
    post,
    path = "/exchange-token",
    summary = "Exchanges a Plaid public token for an access token",
    description = "Exchanges the public token returned by Plaid Link for an access token and Item ID. The access token is securely stored and associated with the authenticated user so the backend can retrieve financial data and process webhooks.",
    request_body(
        content = ExchangeTokenRequest,
        content_type = "application/json",
        description = "The public token returned after the user successfully completes Plaid Link.",
        example = json!({
            "public_token": "public-sandbox-example"
        })
    ),
    responses(
        (status = 200, description = "Bank connection created successfully", body = ExchangeTokenResponse),
        (status = 400, description = "Invalid public token or exchange failed"),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Plaid"
)]
pub async fn api_plaid_exchange_token(
    Extension(user): Extension<AuthUser>,
    Extension(plaid): Extension<PlaidClient>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<ExchangeTokenRequest>,
) -> ApiResult<Json<ExchangeTokenResponse>> {
    debug!(
        "HANDLER ->> POST /api/plaid/exchange-token user_id={}",
        user.id
    );

    // Swap the one-time public token with Plaid for a long-lived access token
    // and the Item id identifying the linked bank connection.
    let mut exchanged = plaid.exchange_public_token(&payload.public_token).await?;

    // Persist the Item
    sqlx::query!(
        "INSERT INTO plaid_links (user_id, item_id, access_token) VALUES ($1, $2, $3)",
        user.id,
        exchanged.item_id,
        exchanged.access_token
    )
    .execute(&pool)
    .await
    .map_err(AppError::from)?;

    // Seed the sync cursor row (cursor = NULL means first sync not yet run)
    sqlx::query!(
        "INSERT INTO plaid_sync_cursors (item_id, cursor, has_more) VALUES ($1, NULL, TRUE)",
        exchanged.item_id
    )
    .execute(&pool)
    .await
    .map_err(AppError::from)?;

    // The access token is skip_serializing, so it never leaves the backend.
    exchanged.success = true;
    Ok(Json(exchanged))
}

/// Retrieve financial accounts for the authenticated user.
///
/// Fetches account information from Plaid for the user's linked financial
/// institutions using their stored access tokens. Returns account details such
/// as account names, types, identifiers, and available balances where supported.
///
/// `GET /api/plaid/accounts`
#[utoipa::path(
    get,
    path = "/accounts",
    summary = "Retrieves the user's linked financial accounts",
    description = "Fetches account information from Plaid for the authenticated user's linked financial institutions. Returns account names, types, identifiers, and available balances where supported.",
    responses(
        (status = 200, description = "Financial accounts retrieved successfully", body = AccountsResponse),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Failed to retrieve financial accounts")
    ),
    tag = "Plaid"
)]
pub async fn api_plaid_get_accounts(
    Extension(user): Extension<AuthUser>,
    Extension(plaid): Extension<PlaidClient>,
    Extension(pool): Extension<PgPool>,
) -> ApiResult<Json<AccountsResponse>> {
    debug!("HANDLER ->> GET /api/plaid/accounts user_id={}", user.id);

    // Load the user's linked bank connections (and their access tokens).
    let items = sqlx::query_as!(
        PlaidLinkRow,
        "SELECT id, user_id, item_id, access_token, created_at FROM plaid_links WHERE user_id = $1",
        user.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    // Collect accounts across all of the user's linked connections.
    let mut accounts: Vec<AccountResponse> = Vec::new();

    // Fetch the live accounts from Plaid for each linked connection.
    for item in items {
        let plaid_accounts = plaid.get_accounts(&item.access_token).await?;

        // Upsert each account locally so transactions can reference it (FK) and
        // balances stay current.
        for a in &plaid_accounts {
            sqlx::query!(
                r#"
                INSERT INTO plaid_accounts
                    (account_id, item_id, name, official_name, account_type, account_subtype,
                     mask, current_balance, available_balance, currency_code, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
                ON CONFLICT (account_id) DO UPDATE SET
                    name              = EXCLUDED.name,
                    official_name     = EXCLUDED.official_name,
                    account_type      = EXCLUDED.account_type,
                    account_subtype   = EXCLUDED.account_subtype,
                    mask              = EXCLUDED.mask,
                    current_balance   = EXCLUDED.current_balance,
                    available_balance = EXCLUDED.available_balance,
                    currency_code     = EXCLUDED.currency_code,
                    updated_at        = NOW()
                "#,
                a.account_id,
                a.item_id,
                a.name,
                a.official_name,
                a.account_type,
                a.account_subtype,
                a.mask,
                a.current_balance,
                a.available_balance,
                a.currency_code
            )
            .execute(&pool)
            .await
            .map_err(AppError::from)?;
        }

        // The client already returns the API model (no access tokens exposed).
        accounts.extend(plaid_accounts);
    }

    // Return the combined list of accounts.
    Ok(Json(AccountsResponse { accounts }))
}

/// Synchronize transactions for the authenticated user's linked financial
/// institutions.
///
/// Uses Plaid's Transactions Sync API to retrieve added, modified, and removed
/// transactions. Processes pagination, persists transaction changes, and updates
/// the synchronization cursor for each Plaid Item to support incremental syncs.
///
/// `POST /api/plaid/transactions/sync`
#[utoipa::path(
    post,
    path = "/transactions/sync",
    summary = "Synchronizes the user's financial transactions",
    description = "Fetches transaction changes from Plaid for the authenticated user's linked financial institutions, processes added, modified, and removed transactions, and updates synchronization cursors.",
    responses(
        (status = 200, description = "Transactions synchronized successfully", body = TransactionSyncResponse),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Failed to synchronize transactions")
    ),
    tag = "Plaid"
)]
#[allow(clippy::too_many_lines)]
pub async fn api_plaid_sync_transactions(
    Extension(user): Extension<AuthUser>,
    Extension(plaid): Extension<PlaidClient>,
    Extension(pool): Extension<PgPool>,
) -> ApiResult<Json<TransactionSyncResponse>> {
    debug!(
        "HANDLER ->> POST /api/plaid/transactions/sync user_id={}",
        user.id
    );

    // Load the user's linked bank connections (and their access tokens).
    let items = sqlx::query_as!(
        PlaidLinkRow,
        "SELECT id, user_id, item_id, access_token, created_at FROM plaid_links WHERE user_id = $1",
        user.id
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    // Running totals across every linked connection.
    let mut total_added: usize = 0;
    let mut total_modified: usize = 0;
    let mut total_removed: usize = 0;

    // Sync each linked connection independently.
    for item in items {
        // Ensure accounts exist before syncing transactions (FK requirement)
        let plaid_accounts = plaid.get_accounts(&item.access_token).await?;
        for a in &plaid_accounts {
            // Upsert the account so the local copy stays current.
            sqlx::query!(
                r#"
                INSERT INTO plaid_accounts
                    (account_id, item_id, name, official_name, account_type, account_subtype,
                     mask, current_balance, available_balance, currency_code, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
                ON CONFLICT (account_id) DO UPDATE SET
                    name              = EXCLUDED.name,
                    official_name     = EXCLUDED.official_name,
                    account_type      = EXCLUDED.account_type,
                    account_subtype   = EXCLUDED.account_subtype,
                    mask              = EXCLUDED.mask,
                    current_balance   = EXCLUDED.current_balance,
                    available_balance = EXCLUDED.available_balance,
                    currency_code     = EXCLUDED.currency_code,
                    updated_at        = NOW()
                "#,
                a.account_id,
                a.item_id,
                a.name,
                a.official_name,
                a.account_type,
                a.account_subtype,
                a.mask,
                a.current_balance,
                a.available_balance,
                a.currency_code
            )
            .execute(&pool)
            .await
            .map_err(AppError::from)?;
        }

        // Load the stored sync cursor for this connection.
        let cursor_row = sqlx::query_as!(
            PlaidSyncCursorRow,
            "SELECT id, item_id, cursor, has_more, updated_at FROM plaid_sync_cursors WHERE item_id = $1",
            item.item_id
        )
        .fetch_optional(&pool)
        .await
        .map_err(AppError::from)?;

        // Resume from the stored cursor; None means a full initial sync.
        let mut cursor: Option<String> = cursor_row.and_then(|r| r.cursor);

        // Page through Plaid's sync API until all pending changes are consumed.
        loop {
            // Fetch the next page of changes from Plaid.
            let page = plaid
                .sync_transactions(&item.access_token, cursor.clone())
                .await?;

            // Tally this page's counts for the response.
            total_added += page.added.len();
            total_modified += page.modified.len();
            total_removed += page.removed.len();

            // Upsert newly added transactions; ON CONFLICT keeps this idempotent if a
            // page is replayed.
            for tx in &page.added {
                sqlx::query!(
                    r#"
                    INSERT INTO plaid_transactions
                        (transaction_id, account_id, name, amount, date, pending, category, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
                    ON CONFLICT (transaction_id) DO UPDATE SET
                        name       = EXCLUDED.name,
                        amount     = EXCLUDED.amount,
                        date       = EXCLUDED.date,
                        pending    = EXCLUDED.pending,
                        category   = CASE
                                         WHEN plaid_transactions.category_overridden
                                         THEN plaid_transactions.category
                                         ELSE EXCLUDED.category
                                     END,
                        updated_at = NOW()
                    "#,
                    tx.transaction_id,
                    tx.account_id,
                    tx.name,
                    tx.amount,
                    tx.date,
                    tx.pending,
                    tx.category().as_str()
                )
                .execute(&pool)
                .await
                .map_err(AppError::from)?;
            }

            // Upsert modified transactions the same way.
            for tx in &page.modified {
                sqlx::query!(
                    r#"
                    INSERT INTO plaid_transactions
                        (transaction_id, account_id, name, amount, date, pending, category, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
                    ON CONFLICT (transaction_id) DO UPDATE SET
                        name       = EXCLUDED.name,
                        amount     = EXCLUDED.amount,
                        date       = EXCLUDED.date,
                        pending    = EXCLUDED.pending,
                        category   = CASE
                                         WHEN plaid_transactions.category_overridden
                                         THEN plaid_transactions.category
                                         ELSE EXCLUDED.category
                                     END,
                        updated_at = NOW()
                    "#,
                    tx.transaction_id,
                    tx.account_id,
                    tx.name,
                    tx.amount,
                    tx.date,
                    tx.pending,
                    tx.category().as_str()
                )
                .execute(&pool)
                .await
                .map_err(AppError::from)?;
            }

            // Delete transactions that Plaid reports as removed.
            for removed in &page.removed {
                sqlx::query!(
                    "DELETE FROM plaid_transactions WHERE transaction_id = $1",
                    removed.transaction_id
                )
                .execute(&pool)
                .await
                .map_err(AppError::from)?;
            }

            // Persist the new cursor after every page so a later sync resumes
            // incrementally instead of starting over.
            sqlx::query!(
                r#"
                INSERT INTO plaid_sync_cursors (item_id, cursor, has_more, updated_at)
                VALUES ($1, $2, $3, NOW())
                ON CONFLICT (item_id) DO UPDATE SET
                    cursor     = EXCLUDED.cursor,
                    has_more   = EXCLUDED.has_more,
                    updated_at = NOW()
                "#,
                item.item_id,
                page.next_cursor,
                page.has_more
            )
            .execute(&pool)
            .await
            .map_err(AppError::from)?;

            // Stop once Plaid reports there are no more pages for this connection.
            if !page.has_more {
                break;
            }
            // Otherwise continue from the cursor Plaid returned.
            cursor = Some(page.next_cursor);
        }
    }

    // Report totals across all connections; every page has been consumed, so
    // has_more is always false.
    Ok(Json(TransactionSyncResponse {
        added: total_added,
        modified: total_modified,
        removed: total_removed,
        has_more: false,
    }))
}

/// Retrieve stored transactions for the authenticated user.
///
/// Returns transactions previously synchronized from Plaid for the user's linked
/// financial institutions. Supports filtering and pagination as needed, without
/// requiring a new Plaid API request for every read.
///
/// `GET /api/plaid/transactions`
#[utoipa::path(
    get,
    path = "/transactions",
    summary = "Retrieves the user's synchronized transactions",
    description = "Returns previously synchronized transactions for the authenticated user's linked financial institutions. Supports filtering and pagination without making a new Plaid API request for every read.",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of transactions to return"),
        ("offset" = Option<i64>, Query, description = "Number of transactions to skip")
    ),
    responses(
        (status = 200, description = "Transactions retrieved successfully", body = TransactionsResponse),
        (status = 401, description = "User is not authenticated"),
        (status = 500, description = "Failed to retrieve transactions")
    ),
    tag = "Plaid"
)]
pub async fn api_plaid_get_transactions(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Query(params): Query<TransactionsQuery>,
) -> ApiResult<Json<TransactionsResponse>> {
    debug!(
        "HANDLER ->> GET /api/plaid/transactions user_id={}",
        user.id
    );

    // Default to the first 50 rows when no paging params are supplied.
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    // Reject negative paging values.
    if limit < 0 || offset < 0 {
        return Err(AppError::Validation(
            "limit and offset must be non-negative".to_string(),
        ));
    }
    // Cap the page size to protect the database.
    let limit = limit.min(200);

    // Fetch one page of transactions, newest first. Ownership is enforced by
    // joining transactions -> accounts -> links and filtering on the user id.
    let transactions = sqlx::query_as!(
        TransactionResponse,
        r#"
        SELECT
            t.transaction_id,
            t.account_id,
            t.name,
            t.amount,
            t.date,
            t.pending,
            t.category
        FROM plaid_transactions t
        JOIN plaid_accounts a ON a.account_id = t.account_id
        JOIN plaid_links i ON i.item_id = a.item_id
        WHERE i.user_id = $1
        ORDER BY t.date DESC, t.id DESC
        LIMIT $2 OFFSET $3
        "#,
        user.id,
        limit,
        offset
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::from)?;

    // Count all of the user's transactions (ignoring limit/offset) so clients
    // can paginate.
    let total: Option<i64> = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)
        FROM plaid_transactions t
        JOIN plaid_accounts a ON a.account_id = t.account_id
        JOIN plaid_links i ON i.item_id = a.item_id
        WHERE i.user_id = $1
        "#,
        user.id
    )
    .fetch_one(&pool)
    .await
    .map_err(AppError::from)?;

    // Return the page along with its paging metadata.
    Ok(Json(TransactionsResponse {
        transactions,
        total,
        limit,
        offset,
    }))
}

/// Override the category of one of the authenticated user's transactions.
///
/// The override is sticky: later syncs will not replace a user-chosen category.
///
/// `PUT /api/plaid/transactions/{transaction_id}/category`
#[utoipa::path(
    put,
    path = "/transactions/{transaction_id}/category",
    summary = "Sets the category of a transaction",
    params(("transaction_id" = String, Path, description = "Plaid transaction ID")),
    request_body(
        content = UpdateCategoryRequest,
        content_type = "application/json",
        example = json!({"category": "GROCERIES"})
    ),
    responses(
        (status = 200, description = "Category updated", body = TransactionResponse),
        (status = 401, description = "User is not authenticated"),
        (status = 404, description = "Transaction not found")
    ),
    security(("auth-token" = [])),
    tag = "Plaid"
)]
pub async fn api_plaid_update_transaction_category(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Path(transaction_id): Path<String>,
    Json(payload): Json<UpdateCategoryRequest>,
) -> ApiResult<Json<TransactionResponse>> {
    debug!(
        "HANDLER ->> PUT /api/plaid/transactions/{transaction_id}/category user_id={}",
        user.id
    );

    // Ownership is enforced by joining transactions -> accounts -> links.
    let updated = sqlx::query_as!(
        TransactionResponse,
        r#"
        UPDATE plaid_transactions t
        SET category = $3, category_overridden = TRUE, updated_at = NOW()
        FROM plaid_accounts a
        JOIN plaid_links i ON i.item_id = a.item_id
        WHERE t.transaction_id = $1
          AND a.account_id = t.account_id
          AND i.user_id = $2
        RETURNING t.transaction_id, t.account_id, t.name, t.amount, t.date, t.pending, t.category
        "#,
        transaction_id,
        user.id,
        payload.category.as_str()
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::from)?;

    updated.map(Json).ok_or(AppError::NotFound)
}

/// `OpenAPI` doc for Plaid financial service routes.
///
/// Constructed only by the `#[openapi]` macro and the root [`crate::swagger::ApiDoc`].
#[derive(OpenApi)]
#[openapi(
    paths(
        api_plaid_link_token,
        api_plaid_exchange_token,
        api_plaid_get_accounts,
        api_plaid_sync_transactions,
        api_plaid_get_transactions,
        api_plaid_update_transaction_category
    ),
    components(schemas(UpdateCategoryRequest)),
    info(title = "Plaid Routes", description = "Plaid financial service endpoints."),
    tags((name = "Plaid"))
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct PlaidApiDoc;

/// Builds the Plaid route group.
///
/// All routes require a valid `auth-token` cookie — the `middleware_auth` layer
/// is applied to the entire group.
pub fn plaid_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/link-token", post(api_plaid_link_token))
        .route("/exchange-token", post(api_plaid_exchange_token))
        .route("/accounts", get(api_plaid_get_accounts))
        .route("/transactions/sync", post(api_plaid_sync_transactions))
        .route("/transactions", get(api_plaid_get_transactions))
        .route(
            "/transactions/{transaction_id}/category",
            put(api_plaid_update_transaction_category),
        )
        .route_layer(axum::middleware::from_fn(middleware_auth))
}
