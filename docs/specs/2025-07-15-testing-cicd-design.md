# Testing, CI/CD & Swagger Pages Design — smart-finance

**Date:** 2025-07-15  
**Status:** Approved  
**Scope:** Unit tests, integration tests, CI/CD pipeline (lint + coverage gates), and GitHub Pages publication of the OpenAPI spec as a fully self-contained Swagger UI.

---

## Overview

Add a complete test harness and CI/CD pipeline to smart-finance. The CI runs on all pushes and pull requests: format check, Clippy, build, DB migrations, unit tests (90% coverage gate), and integration tests (75% coverage gate). On merge to `main`, a second job deploys the generated OpenAPI spec as a self-contained Swagger UI to GitHub Pages at `https://cfdefense.github.io/smart-finance/`.

---

## Architecture

```
src/
├── bin/
│   └── gen-openapi.rs          ← standalone binary: writes docs/openapi.json
├── controllers/
│   └── user.rs                 ← unit tests added in #[cfg(test)] module
├── models/
│   └── http/
│       └── user.rs             ← unit tests for SignupRequest::validate()
├── middleware.rs                ← pure token-parsing logic extracted + unit tested
└── tests.rs                    ← integration tests (already gated via #[cfg(test)])

docs/
└── swagger/
    ├── index.html              ← vendored Swagger UI entry point
    ├── swagger-ui-bundle.js    ← vendored
    ├── swagger-ui-standalone-preset.js  ← vendored
    ├── swagger-ui.css          ← vendored
    └── favicon-32x32.png       ← vendored

.github/
└── workflows/
    └── ci.yml                  ← replaces stub; two jobs: ci + deploy
```

---

## Section 1: Unit Tests

Unit tests live in `#[cfg(test)]` modules within the same file as the code under test. No DB, no HTTP stack.

### `src/models/http/user.rs`

Test `SignupRequest::validate()` exhaustively:

| Test | Input | Expected |
|---|---|---|
| `valid_signup` | `alice@example.com` / `Secret_123` | `Ok(())` |
| `empty_email` | `""` / valid password | `Err(...)` |
| `invalid_email_no_at` | `notanemail` / valid password | `Err(...)` |
| `password_too_short` | valid email / `Ab1` | `Err(...)` |
| `password_no_uppercase` | valid email / `secret_123` | `Err(...)` |
| `password_no_digit` | valid email / `Secret_abc` | `Err(...)` |
| `password_no_special` | valid email / `Secret123` | `Err(...)` |

### `src/middleware.rs`

Extract token parsing into a pure function `parse_auth_token(token: &str) -> Option<(i32, i64)>` that returns `(user_id, expiry_unix)` or `None` on malformed input. Unit test this function:

| Test | Input | Expected |
|---|---|---|
| `valid_token` | `"user-42.9999999999.sign"` | `Some((42, 9999999999))` |
| `missing_sign_suffix` | `"user-42.9999.nope"` | `None` |
| `missing_user_prefix` | `"42.9999.sign"` | `None` |
| `non_numeric_id` | `"user-abc.9999.sign"` | `None` |
| `non_numeric_exp` | `"user-42.abc.sign"` | `None` |
| `wrong_part_count` | `"user-42.sign"` | `None` |

`middleware_auth` calls `parse_auth_token` internally. The DB-dependent and cookie-dependent logic stays untested at the unit level (covered by integration tests).

### Coverage target

`cargo tarpaulin --run-types Tests --fail-under 90`

---

## Section 2: Integration Tests

### Test infrastructure

**`src/tests.rs`** contains all integration tests under a `#[cfg(test)]` guard.

**`test_app()` helper** — builds the full `axum::Router` identical to `main()` minus the Swagger UI merge:
- Reads `DATABASE_URL` from env (set by CI; locally from `.env`)
- Creates a `PgPool` via `db::create_pool()`
- Generates an ephemeral `Key::generate()` for cookies
- Layers: `Extension(pool)`, `Extension(cookie_key)`, `CookieManagerLayer::new()`
- No CORS layer needed for in-process tests

**Test isolation** — each test uses a unique email (`format!("test+{}@example.com", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos())`) to avoid conflicts. Cleanup: each test deletes the created user by email in a `defer`-style pattern using a drop guard or explicit `sqlx::query` at the end.

**Request pattern** — `app.oneshot(Request::builder()...build().unwrap()).await.unwrap()` using `tower::ServiceExt`.

### Test cases

#### Signup (`POST /api/user/signup`)

| Test | Scenario | Expected status |
|---|---|---|
| `signup_success` | valid unique email + password | 200 |
| `signup_duplicate_email` | same email twice | 409 on second |
| `signup_invalid_payload` | missing password field | 400 |
| `signup_bad_password` | password fails validation | 400 |

#### Login (`POST /api/user/login`)

| Test | Scenario | Expected status |
|---|---|---|
| `login_success` | existing user, correct password | 200 |
| `login_wrong_password` | existing user, wrong password | 400 |
| `login_unknown_email` | no user with that email | 400 |

#### Logout (`POST /api/user/logout`)

| Test | Scenario | Expected status |
|---|---|---|
| `logout_success` | signup → extract cookie → logout with cookie | 200 |
| `logout_unauthenticated` | no cookie | 401 |

### Dev-dependencies to add

```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

`tower` provides `ServiceExt::oneshot`. `http-body-util` is needed to collect response body bytes in assertions. `tokio` is already a non-dev dependency so it does not need repeating here.

### Coverage target

`cargo tarpaulin --run-types Bins --fail-under 75`

---

## Section 3: CI/CD Pipeline

Replaces `.github/workflows/ci.yml` entirely.

### Job 1: `ci`

Trigger: all pushes, all pull requests.

```yaml
services:
  postgres:
    image: postgres:16
    env:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: smart_finance_test
    ports: ["5432:5432"]
    options: >-
      --health-cmd pg_isready
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5
```

Environment variables for all steps:
```
DATABASE_URL=postgres://postgres:postgres@localhost:5432/smart_finance_test
FRONTEND_URL=http://localhost:3000
BIND_ADDRESS=127.0.0.1:3001
```

Steps (sequential):

| # | Step | Command |
|---|---|---|
| 1 | Checkout | `actions/checkout@v4` |
| 2 | Rust toolchain | `dtolnay/rust-toolchain@stable` with `components: clippy, rustfmt` |
| 3 | Cache | `Swatinem/rust-cache@v2` |
| 4 | Install tarpaulin | `cargo install cargo-tarpaulin --locked` (cached) |
| 5 | Install sqlx-cli | `cargo install sqlx-cli --no-default-features --features postgres --locked` (cached) |
| 6 | Run migrations | `sqlx migrate run` |
| 7 | Format check | `cargo fmt --check` |
| 8 | Clippy | `cargo clippy -- -D warnings` |
| 9 | Build | `cargo build --locked` |
| 10 | Unit tests + coverage | `cargo tarpaulin --run-types Tests --fail-under 90 --out Xml` |
| 11 | Integration tests + coverage | `cargo tarpaulin --run-types Bins --fail-under 75 --out Xml Html` |
| 12 | Upload coverage report | `actions/upload-artifact@v4` (HTML report from step 11) |

### Job 2: `deploy`

Trigger: push to `main` only. Needs: `ci`.

Steps:

| # | Step | Detail |
|---|---|---|
| 1 | Checkout | `actions/checkout@v4` |
| 2 | Rust toolchain | `dtolnay/rust-toolchain@stable` |
| 3 | Cache | `Swatinem/rust-cache@v2` |
| 4 | Generate openapi.json | `cargo run --bin gen-openapi` → writes `docs/openapi.json` |
| 5 | Assemble Pages artifact | Copy `docs/swagger/` + `docs/openapi.json` into `_site/` |
| 6 | Upload Pages artifact | `actions/upload-pages-artifact@v3` with `path: _site` |
| 7 | Deploy to Pages | `actions/deploy-pages@v4` |

Requires GitHub Pages enabled on the repo with source set to **"GitHub Actions"**.

---

## Section 4: `gen-openapi` Binary

**File:** `src/bin/gen-openapi.rs`

```
fn main()
  → calls swagger::ApiDoc::openapi()
  → serialises to pretty JSON
  → writes to docs/openapi.json (creates docs/ if absent)
  → exits 0
```

`ApiDoc` and `SecurityAddon` must be `pub` (currently `ApiDoc` is private). Move them out from behind the `#[cfg(all(not(test), debug_assertions))]` gate — only `merge_swagger()` needs that gate. The structs themselves are harmless to compile in all configurations.

No DB, no tokio runtime, no env vars required.

---

## Section 5: Swagger UI Vendor Assets

**Location:** `docs/swagger/`

Assets sourced from `swagger-ui-dist` npm package (latest stable at time of implementation). Committed to the repo — zero external dependencies at deploy time.

Files:
- `index.html` — custom entry point; loads bundled JS/CSS with relative paths; sets `url: "./openapi.json"`
- `swagger-ui-bundle.js`
- `swagger-ui-standalone-preset.js`
- `swagger-ui.css`
- `favicon-32x32.png`

The `_site/` directory assembled in the deploy job contains exactly these files plus `openapi.json` written by `gen-openapi`.

GitHub Pages URL: `https://cfdefense.github.io/smart-finance/`

---

## What This Does Not Include

- Test coverage for `src/error.rs` (intentionally excluded via `#[cfg(not(tarpaulin_include))]`)
- Coverage for `src/log.rs`, `src/db.rs`, `src/main.rs` (infrastructure/wiring, not business logic)
- Mutation testing
- Load or performance testing
- Branch preview deployments on PRs
- Automatic ratcheting of coverage thresholds
