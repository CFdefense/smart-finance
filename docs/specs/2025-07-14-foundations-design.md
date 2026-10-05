# Foundations Design — smart-finance Web API

**Date:** 2025-07-14  
**Status:** Approved  
**Scope:** Five foundation modules required to stand up a compilable, runnable Axum web API with logging, database, error handling, and Swagger documentation.

---

## Overview

Set up the five foundation modules for the smart-finance web API. The approach is minimal-from-scratch — each module is as simple as possible while still being fully functional. Patterns are informed by the Journey reference project but stripped of anything not needed at this stage.

---

## Architecture

```
src/
├── main.rs          — async entry point: wires everything together
├── global.rs        — compile-time constants (log paths)
├── log.rs           — file logging + stdout + panic handler
├── error.rs         — AppError enum, ApiResult type alias
├── db.rs            — PgPool creation from DATABASE_URL
├── swagger.rs       — OpenApi root doc, merge_swagger(), openapi.json write
├── middleware.rs     — middleware_auth (cookie-based auth guard)
└── controllers/
    └── mod.rs       — AxumRouter type alias, future controller re-exports
```

A new `src/global.rs` module holds shared compile-time string constants so that `log.rs` and future modules don't hard-code paths inline.

---

## Dependencies to Add (`Cargo.toml`)

| Crate | Version | Features |
|---|---|---|
| `axum` | 0.8 | `macros` |
| `tokio` | 1 | `full` |
| `tower-http` | 0.6 | `cors` |
| `tower-cookies` | 0.11 | `private`, `signed` |
| `sqlx` | 0.8 | `runtime-tokio`, `postgres`, `macros`, `chrono` |
| `tracing` | 0.1 | — |
| `tracing-appender` | 0.2 | — |
| `tracing-subscriber` | 0.3 | `env-filter`, `registry` |
| `dotenvy` | 0.15 | — |
| `serde` | 1 | `derive` |
| `serde_json` | 1 | — |
| `chrono` | 0.4 | `serde` |
| `http` | 1 | — |
| `utoipa` | 5 | `axum_extras`, `chrono`, `openapi_extensions` |
| `utoipa-swagger-ui` | 9 | `axum`, `cache` |
| `utoipa-axum` | 0.2 | — |

---

## Module Designs

### `src/global.rs`

Compile-time constants only. No logic.

```
LOG_DIR      — path to logs/ directory (CARGO_MANIFEST_DIR + /logs)
CRASH_LOG    — "crash.log"
LATEST_LOG   — "latest.log"
```

---

### `src/log.rs`

Two public functions:

**`init_panic_handler()`**
- Sets `RUST_BACKTRACE=full` via `env::set_var`
- Installs a custom panic hook that:
  - Logs the panic via `tracing::error!`
  - Creates `logs/crash.log` with timestamp, panic info, and full backtrace

**`init_logger()`**
- Called once via `Once` static guard
- Removes any existing `logs/latest.log` before starting
- Creates a non-blocking `tracing_appender::non_blocking` writer to `logs/latest.log` (rolling::never)
- Installs a `tracing_subscriber::registry()` with two layers:
  - **File layer** — pretty format, timestamps, file+line, thread info, filtered by `RUST_LOG` env var
  - **Stdout layer** — compact format, ANSI colours, filtered by `RUST_LOG` env var
- Leaks the `WorkerGuard` to give it a `'static` lifetime (same as Journey)

---

### `src/error.rs`

All items are `#[cfg(not(tarpaulin_include))]` to exclude from coverage.

**Type alias:** `pub type ApiResult<T> = Result<T, AppError>`

**`AppError` enum variants:**
- `Validation(String)` → 400
- `BadRequest(String)` → 400
- `Unauthorized` → 401
- `NotFound` → 404
- `Conflict(String)` → 409
- `Internal(String)` → 500

**Trait implementations:**
- `status_code() -> StatusCode` — match on variant, return HTTP status
- `log()` — `tracing::error!` with structured fields (kind, message)
- `Display` — human-readable string per variant
- `std::error::Error`
- `IntoResponse` — calls `self.log()`, returns `self.status_code().into_response()` (no body, no information leak)

**`From` conversions into `AppError::Internal`:**
- `sqlx::Error`
- `serde_json::Error`
- `std::env::VarError`

---

### `src/db.rs`

Single public async function:

```rust
pub async fn create_pool() -> PgPool
```

- Reads `DATABASE_URL` from env; panics with a clear message if missing
- Creates `PgPoolOptions::new().max_connections(5).connect(url).await`
- Panics with a clear message if connection fails

---

### `src/swagger.rs`

**`ApiDoc`** — `#[derive(OpenApi)]` struct with:
- `info.title` = "Smart Finance API"
- `info.description` = description string
- One local dev server: `http://localhost:3001`
- Empty `nest()` list for now — controllers will add their own `ApiDoc` structs later

**`merge_swagger(router: OpenApiRouter) -> Router`**
- Writes `docs/openapi.json` to disk (creates dirs if needed)
- Wraps router with `OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(router)`
- Calls `.split_for_parts()`, then merges `SwaggerUi::new("/swagger").url("/docs/openapi.json", api)`
- Returns the plain `axum::Router`

---

### `src/middleware.rs`

**`AuthUser`** — `#[derive(Clone, Copy, Debug)]` struct with `pub id: i32`, inserted into request extensions on authenticated requests.

**`middleware_auth`** — async axum middleware function:
1. Extracts `Key` and `PgPool` from request extensions; returns 401 if missing
2. Decrypts `auth-token` private cookie using `Key`; returns 401 if absent or invalid
3. Parses token format `user-<id>.<exp>.sign`; returns 401 if malformed
4. Checks `Utc::now().timestamp() <= exp`; returns 401 if expired
5. If expiry is within 1 hour, issues a refreshed cookie with a new 1-hour expiry
6. Queries DB: `SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)`; returns 401 if not found
7. Inserts `AuthUser { id }` into request extensions and calls `next.run(req)`

> Note: The DB query uses `users` table — adjust table name when the users migration is written.

---

### `src/controllers/mod.rs`

**`AxumRouter` type alias:**
- `axum::Router` when `#[cfg(any(test, not(debug_assertions)))]`
- `utoipa_axum::router::OpenApiRouter` when `#[cfg(all(not(test), debug_assertions))]`

No controller submodules wired up yet.

---

### `src/main.rs`

```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>
```

Startup sequence:
1. `dotenvy::dotenv().ok()`
2. `log::init_panic_handler()`
3. `log::init_logger()`
4. `db::create_pool().await`
5. Build `AxumRouter::new()` with no routes yet — just the skeleton
6. In `#[cfg(all(not(test), debug_assertions))]`: `swagger::merge_swagger(router)`
7. Layer stack: `Extension(pool)`, `Extension(cookie_key)`, `CookieManagerLayer::new()`, `CorsLayer`
8. Read `BIND_ADDRESS` from env; bind `TcpListener`; `axum::serve(...).await`

CORS: allows `FRONTEND_URL` origin, credentials, GET/POST/DELETE methods, standard headers.  
Cookie key: `Key::generate()` — ephemeral for now (sessions invalidated on restart).

---

## What This Does Not Include

- Any actual route handlers or controllers
- User/account models or migrations
- JWT or any auth mechanism beyond the cookie middleware stub
- Frontend static file serving
- Production deployment config
- Test harness beyond what compiles under `#[cfg(test)]`
