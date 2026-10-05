//! User authentication controller — login, signup, logout.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{routing::post, Extension, Json};
use sqlx::PgPool;
use tower_cookies::{
    cookie::{
        time::{Duration, OffsetDateTime},
        Key, SameSite,
    },
    Cookie, Cookies,
};
use tracing::debug;
use utoipa::OpenApi;

use crate::{
    controllers::AxumRouter,
    error::{ApiResult, AppError},
    middleware::{middleware_auth, AuthUser},
    models::http::user::{LoginRequest, SignupRequest},
    models::sql::user::UserRow,
};

/// `OpenAPI` doc for user authentication routes.
#[derive(OpenApi)]
#[openapi(
    paths(api_signup, api_login, api_logout),
    info(title = "User Routes", description = "Authentication endpoints."),
    tags((name = "User"))
)]
pub struct UserApiDoc;

/// Abstracts cookie writing so handlers can be tested without a real cookie jar.
pub trait CookieStore {
    /// Add a private (encrypted) cookie using the given key.
    fn private_add(&mut self, key: &Key, cookie: Cookie<'static>);
}

impl CookieStore for Cookies {
    fn private_add(&mut self, key: &Key, cookie: Cookie<'static>) {
        self.private(key).add(cookie);
    }
}

/// Sets or expires the `auth-token` private cookie.
///
/// Token format: `user-<id>.<exp_unix_seconds>.sign`.
/// Expiry: 3 days from now, or `UNIX_EPOCH` when `expired = true`.
fn set_cookie(user_id: i32, is_expired: bool, cookies: &mut impl CookieStore, key: &Key) {
    let domain = option_env!("DOMAIN").unwrap_or("localhost");
    let on_production = option_env!("APP_ENV").unwrap_or("development") == "production";

    let (expires, max_age) = if is_expired {
        (OffsetDateTime::UNIX_EPOCH, Duration::days(0))
    } else {
        let age = Duration::seconds(crate::global::SESSION_DURATION_SECS);
        (OffsetDateTime::now_utc() + age, age)
    };

    let token = format!("user-{}.{}.sign", user_id, expires.unix_timestamp());

    debug!("set_cookie: token={token} production={on_production}");

    let cookie = Cookie::build(("auth-token", token))
        .domain(domain.to_string())
        .path("/")
        .secure(on_production)
        .http_only(true)
        .same_site(if on_production {
            SameSite::Strict
        } else {
            SameSite::Lax
        })
        .expires(Some(expires))
        .max_age(max_age)
        .build();

    cookies.private_add(key, cookie);
}

/// Create a new user account.
///
/// Validates the payload, checks email uniqueness, hashes the password with Argon2,
/// inserts the user, and sets the `auth-token` cookie on success.
///
/// `POST /api/user/signup`
#[utoipa::path(
    post,
    path = "/signup",
    summary = "Create a new user account",
    request_body(
        content = SignupRequest,
        content_type = "application/json",
        description = "Email must be unique. Password is validated server-side.",
        example = json!({"email": "alice@example.com", "password": "Secret_123"})
    ),
    responses(
        (status = 200, description = "Account created and cookie set"),
        (status = 400, description = "Validation failure"),
        (status = 409, description = "Email already in use"),
        (status = 500, description = "Internal server error")
    ),
    tag = "User"
)]
pub async fn api_signup(
    mut cookies: Cookies,
    Extension(key): Extension<Key>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<SignupRequest>,
) -> ApiResult<()> {
    debug!("HANDLER ->> POST /api/user/signup payload={payload:?}");

    if let Err(e) = payload.validate() {
        return Err(AppError::Validation(e));
    }

    let email = payload.email.trim().to_lowercase();

    // Check email uniqueness
    let existing: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&pool)
        .await
        .map_err(AppError::from)?;

    if existing.is_some() {
        return Err(AppError::Conflict("email already exists".to_string()));
    }

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("password hash error: {e:?}")))?
        .to_string();

    // Insert user
    let row: (i32,) =
        sqlx::query_as("INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id")
            .bind(&email)
            .bind(&password_hash)
            .fetch_one(&pool)
            .await
            .map_err(AppError::from)?;

    set_cookie(row.0, false, &mut cookies, &key);
    Ok(())
}

/// Authenticate a user.
///
/// Verifies the password against the stored Argon2 hash and sets the `auth-token` cookie.
///
/// `POST /api/user/login`
#[utoipa::path(
    post,
    path = "/login",
    summary = "Authenticate a user",
    request_body(
        content = LoginRequest,
        content_type = "application/json",
        example = json!({"email": "alice@example.com", "password": "Secret_123"})
    ),
    responses(
        (status = 200, description = "Login successful, cookie set"),
        (status = 400, description = "Invalid credentials"),
        (status = 500, description = "Internal server error")
    ),
    tag = "User"
)]
pub async fn api_login(
    mut cookies: Cookies,
    Extension(key): Extension<Key>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<LoginRequest>,
) -> ApiResult<()> {
    debug!("HANDLER ->> POST /api/user/login email={}", payload.email);

    let user_result = sqlx::query_as!(
        UserRow,
        "SELECT id, email, password_hash FROM users WHERE email = $1",
        payload.email
    )
    .fetch_optional(&pool)
    .await;

    match user_result {
        Ok(Some(user)) => {
            debug!("login: found user id={} email={}", user.id, user.email);

            let parsed = PasswordHash::new(&user.password_hash)
                .map_err(|e| AppError::Internal(format!("hash parse error: {e:?}")))?;

            Argon2::default()
                .verify_password(payload.password.as_bytes(), &parsed)
                .map_err(|_| AppError::BadRequest("invalid credentials".to_string()))?;

            set_cookie(user.id, false, &mut cookies, &key);
            Ok(())
        }
        Ok(None) | Err(_) => Err(AppError::BadRequest("invalid credentials".to_string())),
    }
}

/// Log out the current user.
///
/// Sets the `auth-token` cookie to expired, invalidating the session.
///
/// `POST /api/user/logout`
#[utoipa::path(
    post,
    path = "/logout",
    summary = "Log out the current user",
    responses(
        (status = 200, description = "Logged out, cookie expired"),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "User"
)]
pub async fn api_logout(
    mut cookies: Cookies,
    Extension(key): Extension<Key>,
    Extension(user): Extension<AuthUser>,
) -> ApiResult<()> {
    debug!("HANDLER ->> POST /api/user/logout user_id={}", user.id);
    set_cookie(user.id, true, &mut cookies, &key);
    Ok(())
}

/// Builds the user route group.
///
/// Public routes: `POST /signup`, `POST /login`.
/// Protected route (requires valid `auth-token` cookie): `POST /logout`.
pub fn user_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/logout", post(api_logout))
        .route_layer(axum::middleware::from_fn(middleware_auth))
        .route("/signup", post(api_signup))
        .route("/login", post(api_login))
}
