# Task 6 Report: Integration tests in `src/tests.rs`

## Status: DONE_WITH_CONCERNS

## Changes Made

### File: `src/tests.rs`

Replaced placeholder file with full integration test suite containing 9 tests across auth endpoints:

- **Helper functions**:
  - `test_app()`: builds full router in-process with `PgPool`, generated `Key`, cookie layer, and route nesting `/api/user`.
  - `unique_email()`: generates collision-free test emails with nanosecond timestamps.
  - `cleanup()`: deletes test user by email after test execution.
  - `auth_body()`: helper for building JSON request payloads.

- **Signup tests**:
  - `signup_success`: verifies `POST /api/user/signup` returns `200 OK`.
  - `signup_duplicate_email`: verifies second signup with same email returns `409 CONFLICT`.
  - `signup_invalid_payload`: verifies missing password field returns `422 UNPROCESSABLE_ENTITY`.
  - `signup_bad_password`: verifies weak password returns `400 BAD_REQUEST`.

- **Login tests**:
  - `login_success`: verifies valid login returns `200 OK`.
  - `login_wrong_password`: verifies wrong password returns `400 BAD_REQUEST`.
  - `login_unknown_email`: verifies non-existent user returns `400 BAD_REQUEST`.

- **Logout tests**:
  - `logout_unauthenticated`: verifies unauthenticated logout returns `401 UNAUTHORIZED`.
  - `logout_success`: signs up user, extracts `Set-Cookie` header, passes it to `POST /api/user/logout`, and verifies `200 OK`.

## Verification

- `cargo check`: Passed with zero new errors.
- `cargo test --no-run`: Test binaries compiled successfully.
- Unit tests (`cargo test middleware::`): 7 passed, 0 failed.
- Integration tests: Require live PostgreSQL instance with `DATABASE_URL` configured. No local Postgres database was available; tests are structured to run in CI against the service container.

## Commit

`cbdb873` — `test(integration): signup, login, logout integration tests`

## Concerns

Local Postgres database is not running / `DATABASE_URL` is not set locally, so the 9 integration tests could not be executed to completion on the local machine. They compile cleanly and will run against the Postgres container in CI.

---

## Post-Review Fix (commit after `cbdb873`)

### Issues fixed

1. **Cookie header forwarding in `logout_success`** (Critical): The raw `Set-Cookie` directive (e.g. `auth-token=<val>; Domain=localhost; Path=/; …`) was forwarded wholesale as the `cookie` request header. The RFC requires the `cookie` request header to contain only `name=value` pairs. Fixed by splitting on `;` and taking the first segment before trimming:
   ```rust
   let cookie_header = set_cookie.split(';').next().unwrap().trim().to_string();
   ```
   The `cookie` header is now `auth-token=<encrypted-value>` with no attributes.

2. **Unused import `http_body_util::BodyExt`** (Important): Removed `use http_body_util::BodyExt;` from the top of `src/tests.rs`. It was imported but never used.

### Verification

- `cargo check`: Passed — no errors, zero new warnings introduced.
- `cargo test --no-run`: Test binaries compiled cleanly — no warnings from `src/tests.rs`.
