//! HTTP request and response models for plaid routes.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to exchange a Plaid public token.
#[derive(Deserialize, ToSchema)]
pub struct ExchangeTokenRequest {
    /// Temporary public token returned by Plaid Link after successful linking.
    pub public_token: String,
}

/// Response containing a token used to initialize Plaid Link.
#[derive(Serialize, ToSchema)]
pub struct LinkTokenResponse {
    /// Short-lived token used by the frontend to launch Plaid Link.
    pub link_token: String,
}

/// Response confirming that a bank connection was saved.
#[derive(Serialize, ToSchema)]
pub struct ExchangeTokenResponse {
    /// Indicates whether the connection was successfully saved.
    pub success: bool,
    /// Plaid Item ID identifying the linked financial institution connection.
    pub item_id: String,
}
