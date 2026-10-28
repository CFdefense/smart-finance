//! User authentication and profile controller — login, signup, logout, profile.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
#[allow(unused_imports)] // `put` used only in test/release builds (plain axum::Router)
use axum::{
    routing::{get, post, put},
    Extension, Json,
};
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
    models::http::user::{LoginRequest, ProfileResponse, SignupRequest, UpdateProfileRequest},
    models::sql::user::{UserProfileRow, UserRow},
};

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

    debug!("set_cookie: production={on_production}");

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
        example = json!({"first_name": "Alice", "last_name": "Smith", "email": "alice@example.com", "password": "Secret_123"})
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
    debug!("HANDLER ->> POST /api/user/signup");

    if let Err(e) = payload.validate() {
        return Err(AppError::Validation(e));
    }

    let email = payload.email.trim().to_lowercase();

    // Check email uniqueness
    let existing = sqlx::query_as!(
        UserRow,
        "SELECT id, email, password_hash, account_status FROM users WHERE email = $1",
        email
    )
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
    let user = sqlx::query_as!(
        UserRow,
        r#"
        INSERT INTO users (first_name, last_name, email, password_hash) 
        VALUES ($1, $2, $3, $4) 
        RETURNING id, email, password_hash, account_status
        "#,
        payload.first_name.trim(),
        payload.last_name.trim(),
        email,
        password_hash
    )
    .fetch_one(&pool)
    .await
    .map_err(AppError::from)?;

    set_cookie(user.id, false, &mut cookies, &key);
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
        example = json!({
            "email": "alice@example.com",
            "password": "Secret_123"})
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
        "SELECT id, email, password_hash, account_status FROM users WHERE email = $1",
        payload.email
    )
    .fetch_optional(&pool)
    .await;

    match user_result {
        Ok(Some(user)) => {
            debug!("login: found user id={} email={}", user.id, user.email);
            if user.account_status != "active" {
                return Err(AppError::BadRequest("invalid credentials".to_string()));
            }

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

/// Check whether the current session is authenticated.
///
/// GET /api/user/me
#[utoipa::path(
    get,
    path = "/me",
    summary = "Check the current authenticated session",
    responses(
        (status = 200, description = "Authenticated"),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "User"
)]
pub async fn api_me(Extension(_user): Extension<AuthUser>) -> ApiResult<()> {
    Ok(())
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

/// Return the authenticated user's notification preferences.
///
/// `GET /api/user/profile`
#[utoipa::path(
    get,
    path = "/profile",
    summary = "Get the current user's notification preferences",
    responses(
        (status = 200, description = "Profile returned", body = ProfileResponse),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "User"
)]
pub async fn api_get_profile(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
) -> ApiResult<Json<ProfileResponse>> {
    debug!("HANDLER ->> GET /api/user/profile user_id={}", user.id);

    let row = sqlx::query_as!(
        UserProfileRow,
        r#"
        SELECT email, email_notifications, phone_number, sms_notifications, sms_carrier_gateway
        FROM users
        WHERE id = $1
        "#,
        user.id
    )
    .fetch_one(&pool)
    .await
    .map_err(AppError::from)?;

    Ok(Json(ProfileResponse {
        email: row.email,
        email_notifications: row.email_notifications,
        phone_number: row.phone_number,
        sms_notifications: row.sms_notifications,
        sms_carrier_gateway: row.sms_carrier_gateway,
    }))
}

/// Update the authenticated user's notification preferences.
///
/// All fields are optional; omitted fields are left unchanged.
/// Send `phone_number: ""` to clear the stored phone number.
///
/// `PUT /api/user/profile`
#[utoipa::path(
    put,
    path = "/profile",
    summary = "Update the current user's notification preferences",
    request_body(
        content = UpdateProfileRequest,
        content_type = "application/json",
        description = "All fields optional. Omit a field to leave it unchanged.",
        example = json!({"email_notifications": true, "phone_number": "+15551234567", "sms_notifications": true})
    ),
    responses(
        (status = 200, description = "Profile updated"),
        (status = 400, description = "Validation failure"),
        (status = 401, description = "Not authenticated")
    ),
    security(("auth-token" = [])),
    tag = "User"
)]
pub async fn api_update_profile(
    Extension(user): Extension<AuthUser>,
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<UpdateProfileRequest>,
) -> ApiResult<()> {
    debug!("HANDLER ->> PUT /api/user/profile user_id={}", user.id);

    let (existing_phone, existing_gateway, existing_sms): (Option<String>, Option<String>, bool) =
        sqlx::query!(
            "SELECT phone_number, sms_carrier_gateway, sms_notifications FROM users WHERE id = $1",
            user.id
        )
        .fetch_one(&pool)
        .await
        .map(|r| (r.phone_number, r.sms_carrier_gateway, r.sms_notifications))
        .map_err(AppError::from)?;

    if let Err(e) = payload.validate(
        existing_phone.as_deref(),
        existing_gateway.as_deref(),
        existing_sms,
    ) {
        return Err(AppError::Validation(e));
    }

    // Determine the phone update intent:
    //   None in payload  => no change (COALESCE keeps current value)
    //   Some("")         => clear (set to NULL, force sms off)
    //   Some(v)          => set to v
    let clear_phone = payload.phone_number.as_deref() == Some("");
    let new_phone: Option<String> = payload.phone_number.filter(|p| !p.is_empty());
    let clear_gateway = payload.sms_carrier_gateway.as_deref() == Some("");
    let new_gateway: Option<String> = payload.sms_carrier_gateway.filter(|g| !g.is_empty());

    sqlx::query!(
        r#"
        UPDATE users SET
          email_notifications  = COALESCE($1, email_notifications),
          phone_number         = CASE
                                   WHEN $2 THEN NULL
                                   WHEN $3::TEXT IS NOT NULL THEN $3::TEXT
                                   ELSE phone_number
                                 END,
          sms_notifications    = CASE
                                   WHEN $2 THEN false
                                   ELSE COALESCE($4, sms_notifications)
                                 END,
          sms_carrier_gateway  = CASE
                                   WHEN $5 THEN NULL
                                   WHEN $6::TEXT IS NOT NULL THEN $6::TEXT
                                   ELSE sms_carrier_gateway
                                 END
        WHERE id = $7
        "#,
        payload.email_notifications, // $1 Option<bool>
        clear_phone,                 // $2 bool — true => NULL phone + false SMS
        new_phone,                   // $3 Option<String>
        payload.sms_notifications,   // $4 Option<bool>
        clear_gateway,               // $5 bool — true => NULL gateway
        new_gateway,                 // $6 Option<String>
        user.id                      // $7 i32
    )
    .execute(&pool)
    .await
    .map_err(AppError::from)?;

    Ok(())
}

/// `OpenAPI` doc for user authentication and profile routes.
///
/// Constructed only by the `#[openapi]` macro and the root [`crate::swagger::ApiDoc`].
#[derive(OpenApi)]
#[openapi(
    paths(api_signup, api_login, api_logout, api_me, api_get_profile, api_update_profile),
    components(schemas(ProfileResponse, UpdateProfileRequest)),
    info(title = "User Routes", description = "Authentication endpoints."),
    tags((name = "User"))
)]
#[allow(dead_code)] // only instantiated by the #[derive(OpenApi)] macro — never constructed directly
pub struct UserApiDoc;

/// Builds the user route group.
///
/// Public routes: `POST /signup`, `POST /login`.
/// Protected routes (require valid `auth-token` cookie):
/// `POST /logout`, `GET /me`, `GET /profile`, `PUT /profile`.
pub fn user_routes() -> AxumRouter {
    AxumRouter::new()
        .route("/logout", post(api_logout))
        .route("/me", get(api_me))
        .route("/profile", get(api_get_profile).put(api_update_profile))
        .route_layer(axum::middleware::from_fn(middleware_auth))
        .route("/signup", post(api_signup))
        .route("/login", post(api_login))
}

#[cfg(test)]
mod test {
    use super::*;
    use tower_cookies::cookie::time::OffsetDateTime;

    /// A minimal [`CookieStore`] that records every cookie added to it.
    #[derive(Default)]
    struct MockCookies {
        added: Vec<Cookie<'static>>,
    }

    impl CookieStore for MockCookies {
        fn private_add(&mut self, _key: &Key, cookie: Cookie<'static>) {
            self.added.push(cookie);
        }
    }

    fn test_key() -> Key {
        // Key::generate() needs 64 bytes; derive from a fixed seed so tests are reproducible.
        Key::from(&[42u8; 64])
    }

    // ------------------------------------------------------------------
    // set_cookie — active session
    // ------------------------------------------------------------------

    #[test]
    fn set_cookie_active_sets_correct_name_and_path() {
        let mut store = MockCookies::default();
        set_cookie(1, false, &mut store, &test_key());

        assert_eq!(store.added.len(), 1);
        let c = &store.added[0];
        assert_eq!(c.name(), "auth-token");
        assert_eq!(c.path(), Some("/"));
    }

    #[test]
    fn set_cookie_active_token_contains_user_id() {
        let mut store = MockCookies::default();
        set_cookie(42, false, &mut store, &test_key());

        let value = store.added[0].value().to_string();
        assert!(
            value.starts_with("user-42."),
            "expected token to start with 'user-42.', got '{value}'"
        );
    }

    #[test]
    fn set_cookie_active_token_ends_with_sign_suffix() {
        let mut store = MockCookies::default();
        set_cookie(7, false, &mut store, &test_key());

        let value = store.added[0].value().to_string();
        assert!(
            value.strip_suffix(".sign").is_some(),
            "expected token to end with '.sign', got '{value}'"
        );
    }

    #[test]
    fn set_cookie_active_max_age_matches_session_duration() {
        let mut store = MockCookies::default();
        set_cookie(1, false, &mut store, &test_key());

        let max_age = store.added[0]
            .max_age()
            .expect("max_age should be set for an active cookie");
        assert_eq!(
            max_age.whole_seconds(),
            crate::global::SESSION_DURATION_SECS
        );
    }

    #[test]
    fn set_cookie_active_expiry_is_in_the_future() {
        let before = OffsetDateTime::now_utc();
        let mut store = MockCookies::default();
        set_cookie(1, false, &mut store, &test_key());
        let after = OffsetDateTime::now_utc();

        let expires = store.added[0]
            .expires()
            .and_then(tower_cookies::cookie::Expiration::datetime)
            .expect("expiry should be set for an active cookie");

        assert!(
            expires > before,
            "expiry {expires} should be after test start {before}"
        );
        assert!(
            expires <= after + Duration::seconds(crate::global::SESSION_DURATION_SECS),
            "expiry {expires} should not exceed session duration past test end {after}"
        );
    }

    // ------------------------------------------------------------------
    // set_cookie — expired session (logout)
    // ------------------------------------------------------------------

    #[test]
    fn set_cookie_expired_max_age_is_zero() {
        let mut store = MockCookies::default();
        set_cookie(1, true, &mut store, &test_key());

        let max_age = store.added[0]
            .max_age()
            .expect("max_age should be set even for an expired cookie");
        assert_eq!(max_age.whole_seconds(), 0);
    }

    #[test]
    fn set_cookie_expired_expiry_is_unix_epoch() {
        let mut store = MockCookies::default();
        set_cookie(1, true, &mut store, &test_key());

        let expires = store.added[0]
            .expires()
            .and_then(tower_cookies::cookie::Expiration::datetime)
            .expect("expiry should be set for an expired cookie");

        assert_eq!(
            expires.unix_timestamp(),
            OffsetDateTime::UNIX_EPOCH.unix_timestamp(),
            "expired cookie should have UNIX_EPOCH expiry"
        );
    }

    #[test]
    fn set_cookie_expired_token_still_contains_user_id() {
        let mut store = MockCookies::default();
        set_cookie(99, true, &mut store, &test_key());

        let value = store.added[0].value().to_string();
        assert!(
            value.starts_with("user-99."),
            "expected token to start with 'user-99.', got '{value}'"
        );
    }

    // ------------------------------------------------------------------
    // set_cookie — http_only and domain
    // ------------------------------------------------------------------

    #[test]
    fn set_cookie_is_http_only() {
        let mut store = MockCookies::default();
        set_cookie(1, false, &mut store, &test_key());
        assert!(store.added[0].http_only().unwrap_or(false));
    }

    #[test]
    fn set_cookie_domain_defaults_to_localhost() {
        let mut store = MockCookies::default();
        set_cookie(1, false, &mut store, &test_key());
        assert_eq!(store.added[0].domain(), Some("localhost"));
    }
}
