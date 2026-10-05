# Testing, CI/CD & Swagger Pages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development (recommended) or executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add unit tests, integration tests, a full CI/CD pipeline with lint and coverage gates, and a self-contained GitHub Pages Swagger UI deployment.

**Architecture:** Unit tests live in `#[cfg(test)]` modules in the files they test; integration tests live in `src/tests.rs` and talk to a real Postgres DB via `tower::ServiceExt::oneshot`. CI runs as a single sequential job on all pushes/PRs; a dependent deploy job publishes the OpenAPI spec to GitHub Pages on `main` merges only.

**Tech Stack:** Rust/Axum, sqlx/Postgres, cargo-tarpaulin, cargo-clippy, rustfmt, GitHub Actions, GitHub Pages, Swagger UI (vendored).

## Global Constraints

- Rust edition 2021, `rust-version = "1.80"` — do not use features stabilised after 1.80
- `unsafe_code = "forbid"` — no unsafe blocks anywhere
- All Clippy warnings are errors (`-D warnings`) — new code must be clean
- `missing_docs = "warn"` — all public items need doc comments
- Dev-dependencies: `tower = { version = "0.5", features = ["util"] }`, `http-body-util = "0.1"` only — do not add extra crates
- Coverage thresholds: 90% for `--run-types Tests`, 75% for `--run-types Bins`
- GitHub repo: `https://github.com/CFdefense/smart-finance` (org is `CFdefense`)
- GitHub Pages URL: `https://cfdefense.github.io/smart-finance/`
- Swagger UI assets sourced from `swagger-ui-dist` npm — vendor into `docs/swagger/`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `Cargo.toml` | Modify | Add `tower` and `http-body-util` to `[dev-dependencies]` |
| `src/swagger.rs` | Modify | Make `ApiDoc` and `SecurityAddon` `pub`; move them outside the `#[cfg(all(not(test), debug_assertions))]` gate |
| `src/bin/gen-openapi.rs` | Create | Binary that writes `docs/openapi.json` by calling `ApiDoc::openapi()` |
| `src/middleware.rs` | Modify | Extract `parse_auth_token` as a pure public function; add unit tests |
| `src/models/http/user.rs` | Modify | Add unit tests for `validate()`, `validate_email()`, `validate_password()` |
| `src/tests.rs` | Modify | Full integration test suite for signup/login/logout |
| `docs/swagger/index.html` | Create | Vendored Swagger UI entry point |
| `docs/swagger/swagger-ui-bundle.js` | Create | Vendored from swagger-ui-dist |
| `docs/swagger/swagger-ui-standalone-preset.js` | Create | Vendored from swagger-ui-dist |
| `docs/swagger/swagger-ui.css` | Create | Vendored from swagger-ui-dist |
| `docs/swagger/favicon-32x32.png` | Create | Vendored from swagger-ui-dist |
| `tarpaulin.toml` | Modify | Remove `fail-under` (thresholds passed via CLI flags in CI) |
| `.github/workflows/ci.yml` | Replace | Full CI job + deploy job |

---

## Task 1: Add dev-dependencies and update `tarpaulin.toml`

**Files:**
- Modify: `Cargo.toml`
- Modify: `tarpaulin.toml`

**Interfaces:**
- Produces: `tower::ServiceExt` and `http_body_util::BodyExt` available in `#[cfg(test)]` code

- [ ] **Step 1: Add dev-dependencies to `Cargo.toml`**

In `Cargo.toml`, replace the empty `[dev-dependencies]` section:

```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

- [ ] **Step 2: Remove `fail-under` from `tarpaulin.toml`**

The CI will pass `--fail-under` via CLI flags with different values for unit vs integration runs. Remove it from the config file so it doesn't conflict:

```toml
[run]
run-types = ["Bins"]
all-features = true
force-clean = false
skip-clean = true
locked = true

[report]
output-dir = "docs"
out = ["Stdout", "Html"]
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check
```

Expected: 0 errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml tarpaulin.toml
git commit -m "chore: add tower and http-body-util dev-deps; remove tarpaulin fail-under"
```

---

## Task 2: Expose `ApiDoc` publicly and create `gen-openapi` binary

**Files:**
- Modify: `src/swagger.rs`
- Create: `src/bin/gen-openapi.rs`

**Interfaces:**
- Produces: `swagger::ApiDoc` — a `pub` struct implementing `utoipa::OpenApi`, accessible from `src/bin/gen-openapi.rs`
- Produces: `cargo run --bin gen-openapi` writes `docs/openapi.json`

- [ ] **Step 1: Make `ApiDoc` and `SecurityAddon` public in `src/swagger.rs`**

The `ApiDoc` struct and `SecurityAddon` struct are currently private and only compiled under `#[cfg(all(not(test), debug_assertions))]`. Move them outside that gate so they compile in all configurations. Only `merge_swagger()` keeps the gate.

Replace the entire `src/swagger.rs` with:

```rust
//! Swagger / OpenAPI documentation configuration.

use axum::Router;
use std::{fs, io::Write, path::PathBuf};
use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use crate::controllers::user::UserApiDoc;

/// Security scheme modifier — adds the `auth-token` cookie scheme to the OpenAPI spec.
pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "auth-token",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                    "auth-token",
                    "An HTTP-only private cookie encoding user id and expiry.",
                ))),
            );
        }
    }
}

/// Root OpenAPI document for the Smart Finance API.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    security((), ("auth-token" = [])),
    info(
        title = "Smart Finance API",
        description = "The public API documentation for the Smart Finance web application."
    ),
    nest(
        (path = "/api/user", api = UserApiDoc)
    ),
    servers(
        (url = "http://localhost:3001", description = "Local development server")
    )
)]
pub struct ApiDoc;

/// Merges Swagger UI into the router and writes `docs/openapi.json` to disk.
///
/// Only compiled in non-test debug builds.
#[cfg(all(not(test), debug_assertions))]
pub fn merge_swagger(router: OpenApiRouter) -> Router {
    let doc = ApiDoc::openapi();

    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    fs::create_dir_all(&docs_path).expect("Could not create docs directory");
    let mut file =
        fs::File::create(docs_path.join("openapi.json")).expect("Could not create openapi.json");
    file.write_all(
        doc.to_pretty_json()
            .expect("Could not serialise OpenAPI doc")
            .as_bytes(),
    )
    .expect("Could not write openapi.json");

    let (router, api) = OpenApiRouter::with_openapi(doc)
        .merge(router)
        .split_for_parts();
    router.merge(SwaggerUi::new("/swagger").url("/docs/openapi.json", api))
}
```

- [ ] **Step 2: Create `src/bin/gen-openapi.rs`**

```rust
//! Standalone binary that generates `docs/openapi.json` from the API definition.
//!
//! Used in CI to produce the OpenAPI spec for GitHub Pages deployment.
//! Requires no database, no environment variables, and no running server.

use smart_finance::swagger::ApiDoc;
use std::{fs, io::Write, path::PathBuf};
use utoipa::OpenApi;

fn main() {
    let doc = ApiDoc::openapi();
    let json = doc
        .to_pretty_json()
        .expect("Could not serialise OpenAPI doc");

    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    fs::create_dir_all(&docs_path).expect("Could not create docs directory");

    let out_path = docs_path.join("openapi.json");
    let mut file = fs::File::create(&out_path).expect("Could not create openapi.json");
    file.write_all(json.as_bytes())
        .expect("Could not write openapi.json");

    println!("Written: {}", out_path.display());
}
```

Note: `src/bin/gen-openapi.rs` refers to `smart_finance::swagger::ApiDoc`. For this to work the library crate must expose these modules. Check `src/lib.rs` — this project is a binary crate (`src/main.rs`), so there is no `lib.rs`. The binary crate approach for `src/bin/` in a binary-only project requires the modules to be accessible. Two options:

**Option A (simpler):** Keep `gen-openapi.rs` self-contained — duplicate the `ApiDoc` definition inline rather than importing from `main.rs`.

**Option B (cleaner):** Add a `src/lib.rs` that re-exports the necessary modules, then both `main.rs` and `bin/gen-openapi.rs` use `use smart_finance::...`.

Use **Option A** for this task to avoid restructuring the crate. The `gen-openapi` binary defines its own `ApiDoc` that mirrors `src/swagger.rs`:

```rust
//! Standalone binary that generates `docs/openapi.json` from the API definition.
//!
//! Used in CI to produce the OpenAPI spec for GitHub Pages deployment.
//! Requires no database, no environment variables, and no running server.

use std::{fs, io::Write, path::PathBuf};
use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
    Modify, OpenApi,
};

/// Mirrors `SecurityAddon` in `src/swagger.rs`.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "auth-token",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                    "auth-token",
                    "An HTTP-only private cookie encoding user id and expiry.",
                ))),
            );
        }
    }
}

/// Mirrors `ApiDoc` in `src/swagger.rs` — kept in sync manually.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    security((), ("auth-token" = [])),
    info(
        title = "Smart Finance API",
        description = "The public API documentation for the Smart Finance web application."
    ),
    servers(
        (url = "http://localhost:3001", description = "Local development server")
    )
)]
struct ApiDoc;

fn main() {
    let doc = ApiDoc::openapi();
    let json = doc
        .to_pretty_json()
        .expect("Could not serialise OpenAPI doc");

    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    fs::create_dir_all(&docs_path).expect("Could not create docs directory");

    let out_path = docs_path.join("openapi.json");
    let mut file = fs::File::create(&out_path).expect("Could not create openapi.json");
    file.write_all(json.as_bytes())
        .expect("Could not write openapi.json");

    println!("Written: {}", out_path.display());
}
```

Note: This mirrors `ApiDoc` without the `nest(UserApiDoc)` — the routes won't be in this generated spec. To include them, the `UserApiDoc` and all handler path macros need to be accessible here too. Revisit in Task 2 verification: if the spec is missing routes, restructure to a lib crate (add `src/lib.rs`).

- [ ] **Step 3: Verify compilation**

```bash
cargo check
cargo build --bin gen-openapi
```

Expected: 0 errors. Fix any Clippy warnings (`cargo clippy -- -D warnings`).

- [ ] **Step 4: Run `gen-openapi` and verify output**

```bash
cargo run --bin gen-openapi
cat docs/openapi.json
```

Expected: valid JSON with `"title": "Smart Finance API"`.

- [ ] **Step 5: Commit**

```bash
git add src/swagger.rs src/bin/gen-openapi.rs docs/openapi.json
git commit -m "feat(swagger): expose ApiDoc publicly; add gen-openapi binary"
```

---

## Task 3: Restructure as lib+bin crate to share modules with `gen-openapi`

**Context:** Task 2 noted that `src/bin/gen-openapi.rs` cannot import from `src/main.rs`. To include user routes in the generated spec, add `src/lib.rs` which re-exports the necessary modules. `src/main.rs` then uses `use smart_finance::...` instead of `mod ...`. This is the standard Rust pattern for a crate that is both a library and a binary.

**Files:**
- Create: `src/lib.rs`
- Modify: `src/main.rs`
- Modify: `src/bin/gen-openapi.rs`
- Modify: `src/swagger.rs` (ensure `ApiDoc` is `pub`)

**Interfaces:**
- Produces: `smart_finance::swagger::ApiDoc` — importable from `gen-openapi`
- Produces: `smart_finance::controllers::user::UserApiDoc` — importable from `gen-openapi`

- [ ] **Step 1: Create `src/lib.rs`**

```rust
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
```

- [ ] **Step 2: Update `src/main.rs` to use `smart_finance::`**

Replace the `mod` declarations at the top of `src/main.rs` with `use` imports from the library crate:

```rust
//! Smart Finance API — application entry point.

use smart_finance::{controllers, db, log, middleware, swagger};
use smart_finance::controllers::user::user_routes;
use axum::Extension;
use http::{Method, header::HeaderValue};
use std::{env, net::SocketAddr, str::FromStr};
use tower_cookies::{CookieManagerLayer, cookie::Key};
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
    let api_router = smart_finance::controllers::AxumRouter::new().nest("/user", user_routes());
    let api_router = smart_finance::controllers::AxumRouter::new().nest("/api", api_router);

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
```

- [ ] **Step 3: Update `src/bin/gen-openapi.rs` to import from lib**

```rust
//! Standalone binary that generates `docs/openapi.json` from the live API definition.
//!
//! Used in CI to produce the OpenAPI spec for GitHub Pages deployment.
//! Requires no database, no environment variables, and no running server.

use smart_finance::swagger::ApiDoc;
use std::{fs, io::Write, path::PathBuf};
use utoipa::OpenApi;

fn main() {
    let doc = ApiDoc::openapi();
    let json = doc
        .to_pretty_json()
        .expect("Could not serialise OpenAPI doc");

    let docs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs");
    fs::create_dir_all(&docs_path).expect("Could not create docs directory");

    let out_path = docs_path.join("openapi.json");
    let mut file = fs::File::create(&out_path).expect("Could not create openapi.json");
    file.write_all(json.as_bytes())
        .expect("Could not write openapi.json");

    println!("Written: {}", out_path.display());
}
```

- [ ] **Step 4: Verify compilation and Clippy**

```bash
cargo check
cargo clippy -- -D warnings
cargo build --bin gen-openapi
```

Expected: 0 errors, 0 warnings.

- [ ] **Step 5: Run and confirm routes appear in output**

```bash
cargo run --bin gen-openapi
cat docs/openapi.json | grep -A5 '"paths"'
```

Expected: paths like `/api/user/signup`, `/api/user/login`, `/api/user/logout` present in JSON.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/main.rs src/bin/gen-openapi.rs docs/openapi.json
git commit -m "refactor: convert to lib+bin crate; gen-openapi imports from lib"
```

---

## Task 4: Unit tests — `src/models/http/user.rs`

**Files:**
- Modify: `src/models/http/user.rs`

**Interfaces:**
- Consumes: `SignupRequest::validate()`, `SignupRequest::validate_email()`, `SignupRequest::validate_password()` (all exist in current file)

- [ ] **Step 1: Add `#[cfg(test)]` module to `src/models/http/user.rs`**

Append to the end of `src/models/http/user.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn req(email: &str, password: &str) -> SignupRequest {
        SignupRequest {
            email: email.to_string(),
            password: password.to_string(),
        }
    }

    // --- validate_email ---

    #[test]
    fn valid_email() {
        assert!(SignupRequest::validate_email("alice@example.com"));
    }

    #[test]
    fn email_no_at_symbol() {
        assert!(!SignupRequest::validate_email("notanemail"));
    }

    #[test]
    fn email_no_domain() {
        assert!(!SignupRequest::validate_email("alice@"));
    }

    #[test]
    fn email_no_tld() {
        assert!(!SignupRequest::validate_email("alice@example"));
    }

    #[test]
    fn email_empty() {
        assert!(!SignupRequest::validate_email(""));
    }

    // --- validate_password ---

    #[test]
    fn valid_password() {
        assert!(SignupRequest::validate_password("Secret_123").is_ok());
    }

    #[test]
    fn password_too_short() {
        assert!(SignupRequest::validate_password("Ab1").is_err());
    }

    #[test]
    fn password_no_uppercase() {
        assert!(SignupRequest::validate_password("secret_123").is_err());
    }

    #[test]
    fn password_no_lowercase() {
        assert!(SignupRequest::validate_password("SECRET_123").is_err());
    }

    #[test]
    fn password_no_digit() {
        assert!(SignupRequest::validate_password("Secret_abc").is_err());
    }

    #[test]
    fn password_non_ascii() {
        assert!(SignupRequest::validate_password("Sécret_123").is_err());
    }

    #[test]
    fn password_too_long() {
        let long = "A1a".repeat(50); // 150 chars
        assert!(SignupRequest::validate_password(&long).is_err());
    }

    // --- validate (combined) ---

    #[test]
    fn validate_success() {
        assert!(req("alice@example.com", "Secret_123").validate().is_ok());
    }

    #[test]
    fn validate_empty_email() {
        assert!(req("", "Secret_123").validate().is_err());
    }

    #[test]
    fn validate_invalid_email() {
        assert!(req("notanemail", "Secret_123").validate().is_err());
    }

    #[test]
    fn validate_bad_password() {
        assert!(req("alice@example.com", "weak").validate().is_err());
    }
}
```

- [ ] **Step 2: Run unit tests**

```bash
cargo test models::http::user::tests
```

Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/models/http/user.rs
git commit -m "test(models): unit tests for SignupRequest validation"
```

---

## Task 5: Unit tests — `src/middleware.rs` (extract `parse_auth_token`)

**Files:**
- Modify: `src/middleware.rs`

**Interfaces:**
- Produces: `pub fn parse_auth_token(token: &str) -> Option<(i32, i64)>` — pure function, no DB, no cookies

- [ ] **Step 1: Extract `parse_auth_token` in `src/middleware.rs`**

Add this function before `middleware_auth`:

```rust
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
```

- [ ] **Step 2: Update `middleware_auth` to use `parse_auth_token`**

Replace the inline parsing block in `middleware_auth`. The existing code in `middleware_auth` from `let parts: Vec<&str>...` down to the `exp` parse:

```rust
    let token = decrypted.value().to_string();

    let (user_id, exp) = match parse_auth_token(&token) {
        Some(v) => v,
        None => return AppError::Unauthorized.into_response(),
    };
```

Remove the old manual `parts` parsing and the individual `user_id`/`exp` parsing blocks that followed.

- [ ] **Step 3: Add `#[cfg(test)]` module to `src/middleware.rs`**

Append to the end of `src/middleware.rs`:

```rust
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
```

- [ ] **Step 4: Run tests and check compilation**

```bash
cargo test middleware::tests
cargo clippy -- -D warnings
```

Expected: all 7 tests pass, 0 Clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add src/middleware.rs
git commit -m "test(middleware): extract parse_auth_token and add unit tests"
```

---

## Task 6: Integration tests — `src/tests.rs`

**Files:**
- Modify: `src/tests.rs`

**Interfaces:**
- Consumes: `smart_finance::controllers::user::user_routes()`, `smart_finance::db::create_pool()`, `smart_finance::controllers::AxumRouter`
- Consumes: `tower::ServiceExt::oneshot`, `http_body_util::BodyExt`
- Requires: `DATABASE_URL` env var pointing at a running Postgres instance with migrations applied

**Important:** Integration tests must be compiled as part of the binary (`src/tests.rs` is `mod tests` inside `src/lib.rs`). The `#[tokio::test]` attribute requires `tokio` as a dependency (already present as a non-dev dep).

- [ ] **Step 1: Write the `test_app` helper and signup tests**

Replace the contents of `src/tests.rs` with:

```rust
//! Integration tests for the Smart Finance API.
//!
//! Each test spins up the full axum router in-process against a real Postgres database.
//! Set `DATABASE_URL` in `.env` or the environment before running.
//!
//! Run with: `cargo test --test '*'`

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::PgPool;
use tower::ServiceExt;
use tower_cookies::{CookieManagerLayer, cookie::Key};

use crate::{controllers::AxumRouter, controllers::user::user_routes, db};

/// Builds the full application router for testing — identical to `main()` minus CORS and Swagger.
async fn test_app() -> (axum::Router, PgPool) {
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
fn unique_email(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("test+{prefix}{nanos}@example.com")
}

/// Deletes a test user by email to clean up after each test.
async fn cleanup(pool: &PgPool, email: &str) {
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await
        .ok();
}

fn signup_body(email: &str, password: &str) -> Body {
    Body::from(
        json!({"email": email, "password": password})
            .to_string(),
    )
}

// ─── Signup tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn signup_success() {
    let (app, pool) = test_app().await;
    let email = unique_email("signup_ok");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn signup_duplicate_email() {
    let (app, pool) = test_app().await;
    let email = unique_email("signup_dup");

    // First signup
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Second signup with same email
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn signup_invalid_payload() {
    let (app, _pool) = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"email":"missing-password@example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn signup_bad_password() {
    let (app, _pool) = test_app().await;
    let email = unique_email("signup_badpw");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "weak"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ─── Login tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn login_success() {
    let (app, pool) = test_app().await;
    let email = unique_email("login_ok");

    // Create user first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_wrong_password() {
    let (app, pool) = test_app().await;
    let email = unique_email("login_wrongpw");

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(signup_body(&email, "WrongPass_1"))
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_unknown_email() {
    let (app, _pool) = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/login")
                .header("content-type", "application/json")
                .body(signup_body("nobody@example.com", "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ─── Logout tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn logout_unauthenticated() {
    let (app, _pool) = test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_success() {
    let (app, pool) = test_app().await;
    let email = unique_email("logout_ok");

    // Signup to get a Set-Cookie header
    let signup_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/signup")
                .header("content-type", "application/json")
                .body(signup_body(&email, "Secret_123"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Extract the Set-Cookie value to forward on logout
    let cookie_header = signup_resp
        .headers()
        .get("set-cookie")
        .expect("signup must set a cookie")
        .to_str()
        .unwrap()
        .to_string();

    // The cookie value is encrypted so we can't forge it — forward the raw header
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/user/logout")
                .header("cookie", cookie_header)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    cleanup(&pool, &email).await;
    assert_eq!(response.status(), StatusCode::OK);
}
```

**Note on `logout_success`:** The cookie is private (encrypted with `Key`). The `test_app()` uses `Key::generate()` for each call, which means the cookie set by one `test_app()` call cannot be decrypted by a different `test_app()` instance. The `test_app()` must be called once and the same `app` (with the same `Key`) must handle both signup and logout. The test above does this correctly — `app.clone()` shares the same layers including the same `Key` extension.

- [ ] **Step 2: Run integration tests**

Ensure a Postgres instance is running and `DATABASE_URL` is set in `.env`. Run migrations first if not already done:

```bash
sqlx migrate run
cargo test
```

Expected: all tests pass. If `logout_success` fails with 401, the cookie forwarding approach may need adjustment — see the note about `Key` sharing above.

- [ ] **Step 3: Fix any Clippy warnings**

```bash
cargo clippy -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add src/tests.rs
git commit -m "test(integration): signup, login, logout integration tests"
```

---

## Task 7: Vendor Swagger UI assets

**Files:**
- Create: `docs/swagger/index.html`
- Create: `docs/swagger/swagger-ui-bundle.js`
- Create: `docs/swagger/swagger-ui-standalone-preset.js`
- Create: `docs/swagger/swagger-ui.css`
- Create: `docs/swagger/favicon-32x32.png`

- [ ] **Step 1: Download Swagger UI assets from npm**

```bash
cd /tmp
npm pack swagger-ui-dist 2>/dev/null || npx --yes extract-zip@latest swagger-ui-dist.tgz . 
# Simpler: use curl against the unpkg CDN to fetch the specific version
SWAGGER_VERSION=$(npm view swagger-ui-dist version)
echo "Using swagger-ui-dist $SWAGGER_VERSION"

curl -o /tmp/swagger-ui-bundle.js "https://unpkg.com/swagger-ui-dist@${SWAGGER_VERSION}/swagger-ui-bundle.js"
curl -o /tmp/swagger-ui-standalone-preset.js "https://unpkg.com/swagger-ui-dist@${SWAGGER_VERSION}/swagger-ui-standalone-preset.js"
curl -o /tmp/swagger-ui.css "https://unpkg.com/swagger-ui-dist@${SWAGGER_VERSION}/swagger-ui.css"
curl -o /tmp/favicon-32x32.png "https://unpkg.com/swagger-ui-dist@${SWAGGER_VERSION}/favicon-32x32.png"

mkdir -p /Users/cfarrell/Documents/smart-finance/docs/swagger
cp /tmp/swagger-ui-bundle.js /tmp/swagger-ui-standalone-preset.js /tmp/swagger-ui.css /tmp/favicon-32x32.png \
   /Users/cfarrell/Documents/smart-finance/docs/swagger/
```

- [ ] **Step 2: Create `docs/swagger/index.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Smart Finance API — Swagger UI</title>
  <link rel="stylesheet" href="./swagger-ui.css" />
  <link rel="icon" type="image/png" href="./favicon-32x32.png" sizes="32x32" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="./swagger-ui-bundle.js"></script>
  <script src="./swagger-ui-standalone-preset.js"></script>
  <script>
    window.onload = function () {
      SwaggerUIBundle({
        url: "./openapi.json",
        dom_id: "#swagger-ui",
        presets: [SwaggerUIBundle.presets.apis, SwaggerUIStandalonePreset],
        layout: "StandaloneLayout",
      });
    };
  </script>
</body>
</html>
```

- [ ] **Step 3: Add `docs/swagger/` to `.gitignore` exclusion**

Check `.gitignore` — ensure `docs/swagger/` and `docs/openapi.json` are NOT ignored (they should be committed). If `docs/` is in `.gitignore`, add exceptions:

```
!docs/swagger/
!docs/openapi.json
```

- [ ] **Step 4: Commit vendored assets**

```bash
git add docs/swagger/
git commit -m "chore(swagger): vendor swagger-ui-dist assets for GitHub Pages"
```

---

## Task 8: GitHub Actions workflow

**Files:**
- Replace: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `cargo run --bin gen-openapi` (Task 3)
- Consumes: `docs/swagger/` assets (Task 7)
- Produces: GitHub Pages deployment at `https://cfdefense.github.io/smart-finance/`

**Prerequisites:** In the GitHub repo settings, enable GitHub Pages with source set to **"GitHub Actions"** (not a branch/folder). Also ensure the repo's Actions have `pages: write` and `id-token: write` permissions (set at the job level in the workflow).

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  ci:
    name: Build, Lint, Test & Coverage
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_USER: postgres
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: smart_finance_test
        ports:
          - 5432:5432
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    env:
      DATABASE_URL: postgres://postgres:postgres@localhost:5432/smart_finance_test
      FRONTEND_URL: http://localhost:3000
      BIND_ADDRESS: 127.0.0.1:3001

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cache Rust build artefacts
        uses: Swatinem/rust-cache@v2

      - name: Install cargo-tarpaulin
        uses: taiki-e/install-action@v2
        with:
          tool: cargo-tarpaulin

      - name: Install sqlx-cli
        run: cargo install sqlx-cli --no-default-features --features postgres --locked

      - name: Run database migrations
        run: sqlx migrate run

      - name: Check formatting
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy -- -D warnings

      - name: Build
        run: cargo build --locked

      - name: Unit tests with coverage (90% gate)
        run: >
          cargo tarpaulin
          --run-types Tests
          --fail-under 90
          --out Xml
          --output-dir docs/coverage

      - name: Integration tests with coverage (75% gate)
        run: >
          cargo tarpaulin
          --run-types Bins
          --fail-under 75
          --out Xml Html
          --output-dir docs/coverage

      - name: Upload coverage report
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: coverage-report
          path: docs/coverage/

  deploy:
    name: Deploy Swagger UI to GitHub Pages
    runs-on: ubuntu-latest
    needs: ci
    if: github.ref == 'refs/heads/main' && github.event_name == 'push'

    permissions:
      pages: write
      id-token: write

    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust build artefacts
        uses: Swatinem/rust-cache@v2

      - name: Generate openapi.json
        run: cargo run --bin gen-openapi

      - name: Assemble Pages artifact
        run: |
          mkdir -p _site
          cp -r docs/swagger/. _site/
          cp docs/openapi.json _site/openapi.json

      - name: Upload Pages artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: _site

      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

- [ ] **Step 2: Verify workflow YAML is valid**

```bash
# Install actionlint locally if available, otherwise just check YAML syntax
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && echo "YAML valid"
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: full pipeline — lint, tests, tarpaulin coverage, Swagger Pages deploy"
```

---

## Task 9: End-to-end verification

- [ ] **Step 1: Run the full local test suite**

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Expected: 0 errors, all tests pass.

- [ ] **Step 2: Run tarpaulin locally for unit tests**

```bash
cargo tarpaulin --run-types Tests --fail-under 90 --out Stdout
```

Expected: ≥ 90% coverage, exit code 0.

- [ ] **Step 3: Run tarpaulin locally for integration/bin tests**

```bash
cargo tarpaulin --run-types Bins --fail-under 75 --out Stdout
```

Expected: ≥ 75% coverage, exit code 0.

- [ ] **Step 4: Test `gen-openapi` binary**

```bash
cargo run --bin gen-openapi
```

Expected: `docs/openapi.json` written, contains paths for `/api/user/signup`, `/api/user/login`, `/api/user/logout`.

- [ ] **Step 5: Push to a branch and verify CI passes**

```bash
git push origin <branch-name>
```

Open the Actions tab on `https://github.com/CFdefense/smart-finance` and verify the `ci` job passes. The `deploy` job only runs on `main` — merge to main to verify GitHub Pages deployment.

- [ ] **Step 6: Verify GitHub Pages**

After merging to `main`, visit `https://cfdefense.github.io/smart-finance/` and confirm the Swagger UI loads with all three auth endpoints visible.

---

## Self-Review Notes

**Spec coverage check:**

| Spec requirement | Task |
|---|---|
| Unit tests for `SignupRequest::validate()` | Task 4 |
| Unit tests for `parse_auth_token` | Task 5 |
| 90% unit test coverage gate | Task 8 (CI step) |
| Integration tests: signup/login/logout | Task 6 |
| 75% integration coverage gate | Task 8 (CI step) |
| Postgres service container in CI | Task 8 |
| `cargo fmt` check | Task 8 |
| `cargo clippy -D warnings` | Task 8 |
| `gen-openapi` binary | Task 2 + Task 3 |
| Swagger UI vendored assets | Task 7 |
| GitHub Pages deploy on `main` only | Task 8 |
| Self-contained (no CDN) | Task 7 |
| Dev-dependencies: `tower`, `http-body-util` | Task 1 |

**Known complexity:** `logout_success` integration test relies on cookie forwarding. Because cookies are encrypted with an ephemeral `Key`, the same `Key` must be used across the signup and logout requests. The test design (`app.clone()` sharing layers) handles this correctly — both calls go through the same `CookieManagerLayer` with the same `Key` extension. If this still fails due to how `tower-cookies` handles private cookies in the test harness, the test may need to be simplified to just assert that logout without a cookie returns 401 (which `logout_unauthenticated` already covers).
