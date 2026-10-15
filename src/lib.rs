//! Smart Finance API — library crate.
//!
//! Exposes application modules for use by integration tests and helper binaries.

pub mod controllers;
pub mod db;
pub mod error;
pub mod global;
pub mod log;
pub mod middleware;
pub mod models;
pub mod swagger;
#[cfg(test)]
mod tests;

use axum::{Extension, Router};
use controllers::AxumRouter;
use http::{header::HeaderValue, Method};
use tower_cookies::{cookie::Key, CookieManagerLayer};
use tower_http::cors::CorsLayer;

/// Create the API router.
///
/// Creates an API router containing:
///   - DB Pool Connection
///   - Nested Router
///   - CORS Layer
///
/// # Panics
///
/// Panics if `frontend_url` cannot be parsed into a valid [`HeaderValue`].
pub async fn create_router(frontend_url: &str) -> Router {
    let pool = db::create_pool().await;
    let cookie_key = Key::generate();

    let routes = AxumRouter::new()
        .nest("/user", controllers::user::user_routes())
        .nest("/plaid", controllers::plaid::plaid_routes());
    let api_router = AxumRouter::new().nest("/api", routes);

    #[cfg(all(not(test), debug_assertions))]
    let api_router = swagger::merge_swagger(api_router);

    let cors = CorsLayer::new()
        .allow_origin(
            frontend_url
                .parse::<HeaderValue>()
                .expect("Invalid FRONTEND_URL format"),
        )
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::ACCEPT,
            http::header::AUTHORIZATION,
            http::header::HeaderName::from_static("x-requested-with"),
        ]);

    Router::new()
        .merge(api_router)
        .layer(Extension(pool))
        .layer(Extension(cookie_key))
        .layer(CookieManagerLayer::new())
        .layer(cors)
}
