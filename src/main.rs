//! Smart Finance API — application entry point.

mod controllers;
mod db;
mod error;
mod global;
mod log;
mod middleware;
mod models;
mod swagger;

#[cfg(test)]
mod tests;

use axum::Extension;
use controllers::{user::user_routes, AxumRouter};
use http::{header::HeaderValue, Method};
use std::{env, net::SocketAddr, str::FromStr};
use tower_cookies::{cookie::Key, CookieManagerLayer};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    log::init_panic_handler();
    log::init_logger();

    let pool = db::create_pool().await;

    let frontend_url = env::var("FRONTEND_URL").expect("FRONTEND_URL must be set");
    let bind_address = env::var("BIND_ADDRESS").expect("BIND_ADDRESS must be set");

    let cookie_key = Key::generate();

    // Build API router
    let api_router = AxumRouter::new().nest("/user", user_routes());
    let api_router = AxumRouter::new().nest("/api", api_router);

    // Attach Swagger UI in dev builds only
    #[cfg(all(not(test), debug_assertions))]
    let api_router = swagger::merge_swagger(api_router);

    // CORS
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

    let app = axum::Router::new()
        .merge(api_router)
        .layer(Extension(pool))
        .layer(Extension(cookie_key))
        .layer(CookieManagerLayer::new())
        .layer(cors);

    let addr = SocketAddr::from_str(&bind_address).expect("Invalid BIND_ADDRESS format");
    tracing::info!("Server starting on {bind_address}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
