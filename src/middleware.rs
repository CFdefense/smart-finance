//! Axum middleware for cookie-based authentication.

use crate::error::AppError;
use axum::{extract::Request, middleware::Next, response::IntoResponse};
use chrono::Utc;
use sqlx::PgPool;
use tower_cookies::{
    cookie::{
        time::{Duration, OffsetDateTime},
        Cookie, Key, SameSite,
    },
    Cookies,
};

/// Inserted into request extensions on authenticated requests.
#[derive(Clone, Copy, Debug)]
pub struct AuthUser {
    /// The authenticated user's database ID.
    pub id: i32,
}

/// Parses a raw `auth-token` value into `(user_id, expiry_unix_seconds)`.
///
/// Expected format: `user-<id>.<exp>.sign`
/// Returns `None` if the token is malformed.
pub fn parse_auth_token(token: &str) -> Option<(i32, i64)> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 || parts[2] != "sign" || !parts[0].starts_with("user-") {
        return None;
    }
    let user_id: i32 = parts[0][5..].parse().ok()?;
    let exp: i64 = parts[1].parse().ok()?;
    Some((user_id, exp))
}

/// Axum middleware that authenticates requests via a private `auth-token` cookie.
///
/// Decrypts the cookie with the `Key` from request extensions, validates the
/// embedded expiry and that the user exists in the database, then inserts
/// [`AuthUser`] into extensions. Returns 401 on any failure.
pub async fn middleware_auth(cookies: Cookies, mut req: Request, next: Next) -> impl IntoResponse {
    let key = match req.extensions().get::<Key>() {
        Some(k) => k.clone(),
        None => return AppError::Unauthorized.into_response(),
    };
    let pool = match req.extensions().get::<PgPool>() {
        Some(p) => p.clone(),
        None => return AppError::Unauthorized.into_response(),
    };

    let Some(decrypted) = cookies.private(&key).get("auth-token") else {
        return AppError::Unauthorized.into_response();
    };
    let token = decrypted.value().to_string();

    let Some((user_id, exp)) = parse_auth_token(&token) else {
        return AppError::Unauthorized.into_response();
    };

    let now = Utc::now().timestamp();
    if now > exp {
        return AppError::Unauthorized.into_response();
    }

    // Refresh cookie if it expires within the next session window
    let session_secs = crate::global::SESSION_DURATION_SECS;
    if exp - now < session_secs {
        let new_exp = now + session_secs;
        let new_token = format!("user-{user_id}.{new_exp}.sign");
        let domain = option_env!("DOMAIN").unwrap_or("localhost");
        let on_production = option_env!("APP_ENV").unwrap_or("development") == "production";

        let new_cookie = Cookie::build(("auth-token", new_token))
            .domain(domain.to_string())
            .path("/")
            .secure(on_production)
            .http_only(true)
            .same_site(if on_production {
                SameSite::Strict
            } else {
                SameSite::Lax
            })
            .expires(OffsetDateTime::now_utc().saturating_add(Duration::seconds(session_secs)))
            .max_age(Duration::seconds(session_secs))
            .build();

        cookies.private(&key).add(new_cookie);
    }

    // Verify the user exists in the database
    // Note: update table name to match your users migration
    let exists: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_token() {
        assert_eq!(
            parse_auth_token("user-42.9999999999.sign"),
            Some((42, 9_999_999_999))
        );
    }

    #[test]
    fn parse_wrong_suffix() {
        assert_eq!(parse_auth_token("user-42.9999999999.nope"), None);
    }

    #[test]
    fn parse_missing_user_prefix() {
        assert_eq!(parse_auth_token("42.9999999999.sign"), None);
    }

    #[test]
    fn parse_non_numeric_id() {
        assert_eq!(parse_auth_token("user-abc.9999999999.sign"), None);
    }

    #[test]
    fn parse_non_numeric_exp() {
        assert_eq!(parse_auth_token("user-42.abc.sign"), None);
    }

    #[test]
    fn parse_wrong_part_count() {
        assert_eq!(parse_auth_token("user-42.sign"), None);
    }

    #[test]
    fn parse_empty_string() {
        assert_eq!(parse_auth_token(""), None);
    }
}
