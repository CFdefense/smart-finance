//! Axum middleware for cookie-based authentication.

use crate::error::AppError;
use axum::{extract::Request, middleware::Next, response::IntoResponse};
use chrono::Utc;
use sqlx::PgPool;
use tower_cookies::{
    Cookies,
    cookie::{
        Cookie, Key, SameSite,
        time::{Duration, OffsetDateTime},
    },
};

/// Inserted into request extensions on authenticated requests.
#[derive(Clone, Copy, Debug)]
pub struct AuthUser {
    /// The authenticated user's database ID.
    pub id: i32,
}

/// Axum middleware that authenticates requests via a private `auth-token` cookie.
///
/// Decrypts the cookie with the `Key` from request extensions, validates the
/// embedded expiry and that the user exists in the database, then inserts
/// [`AuthUser`] into extensions. Returns 401 on any failure.
pub async fn middleware_auth(
    cookies: Cookies,
    mut req: Request,
    next: Next,
) -> impl IntoResponse {
    let key = match req.extensions().get::<Key>() {
        Some(k) => k.clone(),
        None => return AppError::Unauthorized.into_response(),
    };
    let pool = match req.extensions().get::<PgPool>() {
        Some(p) => p.clone(),
        None => return AppError::Unauthorized.into_response(),
    };

    let decrypted = match cookies.private(&key).get("auth-token") {
        Some(c) => c,
        None => return AppError::Unauthorized.into_response(),
    };
    let token = decrypted.value().to_string();

    // Expected format: user-<id>.<exp>.sign
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 || parts[2] != "sign" || !parts[0].starts_with("user-") {
        return AppError::Unauthorized.into_response();
    }

    let user_id: i32 = match parts[0][5..].parse() {
        Ok(v) => v,
        Err(_) => return AppError::Unauthorized.into_response(),
    };
    let exp: i64 = match parts[1].parse() {
        Ok(v) => v,
        Err(_) => return AppError::Unauthorized.into_response(),
    };

    let now = Utc::now().timestamp();
    if now > exp {
        return AppError::Unauthorized.into_response();
    }

    // Refresh cookie if it expires within the next hour
    let one_hour: i64 = 3600;
    if exp - now < one_hour {
        let new_exp = now + one_hour;
        let new_token = format!("user-{user_id}.{new_exp}.sign");
        let domain = option_env!("DOMAIN").unwrap_or("localhost");
        let on_production = option_env!("APP_ENV").unwrap_or("development") == "production";

        let new_cookie = Cookie::build(("auth-token", new_token))
            .domain(domain.to_string())
            .path("/")
            .secure(on_production)
            .http_only(true)
            .same_site(if on_production { SameSite::Strict } else { SameSite::Lax })
            .expires(OffsetDateTime::now_utc().saturating_add(Duration::hours(1)))
            .max_age(Duration::hours(1))
            .build();

        cookies.private(&key).add(new_cookie);
    }

    // Verify the user exists in the database
    // Note: update table name to match your users migration
    let exists: (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap_or((false,));

    if !exists.0 {
        return AppError::Unauthorized.into_response();
    }

    req.extensions_mut().insert(AuthUser { id: user_id });
    next.run(req).await
}
