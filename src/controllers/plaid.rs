//! Plaid Financial Service Endpoints

use axum::{routing::post, Json};

use crate::{controllers::AxumRouter, error::ApiResult, models::http::plaid::*};

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
pub async fn api_plaid_link_token() -> ApiResult<Json<LinkTokenResponse>> {
    todo!()
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
    Json(payload): Json<ExchangeTokenRequest>,
) -> ApiResult<Json<ExchangeTokenResponse>> {
    todo!()
}

/// Builds the user route group.
///
/// Public routes: `POST /signup`, `POST /login`.
/// Protected route (requires valid `auth-token` cookie): `POST /logout`.
pub fn plaid_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/link-token", post(api_plaid_link_token))
        .route("/exchange-token", post(api_plaid_exchange_token))
}

#[cfg(test)]
mod test {
    pub fn test_happy_link_path() {}
    pub fn test_happy_exchange_path() {}
    pub fn test_happy_token_path() {}
}
