//! Integration test fixtures and helper functions.

use axum::body::Body;
use serde_json::json;
use sqlx::PgPool;
use tower_cookies::{cookie::Key, CookieManagerLayer};

use crate::{controllers::user::user_routes, controllers::AxumRouter, db};

/// Builds the full application router for testing.
///
/// Identical to `main()` minus CORS and Swagger UI.
pub async fn test_app() -> (axum::Router, PgPool) {
    dotenvy::dotenv().ok();
    let pool = db::create_pool().await;
    let key = Key::generate();

    let api_router = AxumRouter::new().nest("/user", user_routes());
    let api_router = AxumRouter::new().nest("/api", api_router);

    let app = axum::Router::new()
        .merge(api_router)
        .layer(axum::Extension(pool.clone()))
        .layer(axum::Extension(key))
        .layer(CookieManagerLayer::new());

    (app, pool)
}

/// Generates a unique test email to prevent conflicts between parallel test runs.
pub fn unique_email(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("test+{prefix}{nanos}@example.com")
}

/// Deletes a test user by email to clean up after each test.
pub async fn cleanup(pool: &PgPool, email: &str) {
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .ok();
}

/// Builds a JSON body for signup/login requests.
pub fn auth_body(email: &str, password: &str) -> Body {
    Body::from(json!({"email": email, "password": password}).to_string())
}
