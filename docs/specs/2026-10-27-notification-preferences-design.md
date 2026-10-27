# Notification Preferences — Design Spec

**Date:** 2026-10-27  
**Scope:** Rust API only (Python notification-sending is a separate story)  
**SRS references:** FR-5.4 (email opt-in), FR-1.4 (profile edit)

---

## Overview

Users can configure notification delivery preferences on their profile. Every account has an email address; users may opt in to email notifications, and may optionally add a phone number and opt in to SMS notifications. This story adds the data model, migration, and two new profile routes to the existing user controller.

---

## Database

**Migration:** `007_add_notification_preferences.sql`

Three columns added to the `users` table:

| Column | Type | Nullable | Default |
|---|---|---|---|
| `email_notifications` | `BOOLEAN NOT NULL` | no | `false` |
| `phone_number` | `TEXT` | yes | `NULL` |
| `sms_notifications` | `BOOLEAN NOT NULL` | no | `false` |

**Constraints:**

```sql
ALTER TABLE users
  ADD COLUMN email_notifications BOOLEAN NOT NULL DEFAULT false,
  ADD COLUMN phone_number        TEXT,
  ADD COLUMN sms_notifications   BOOLEAN NOT NULL DEFAULT false,
  ADD CONSTRAINT phone_number_e164
    CHECK (phone_number IS NULL OR phone_number ~ '^\+[1-9]\d{7,14}$'),
  ADD CONSTRAINT sms_requires_phone
    CHECK (sms_notifications = false OR phone_number IS NOT NULL);
```

- `phone_number` must be E.164 format (`+` followed by 8–15 digits) or `NULL`.
- `sms_notifications` cannot be `true` when `phone_number` is `NULL`.

---

## API Routes

Both routes are added to `src/controllers/user.rs` and sit behind `middleware_auth`.

### `GET /api/user/profile`

Returns the authenticated user's current notification preferences.

**Response `200`:**
```json
{
  "email": "alice@example.com",
  "email_notifications": false,
  "phone_number": null,
  "sms_notifications": false
}
```

**Errors:** `401` if not authenticated.

---

### `PUT /api/user/profile`

Updates any combination of notification preference fields. All fields are optional; omitted fields are left unchanged.

**Request body:**
```json
{
  "email_notifications": true,
  "phone_number": "+15551234567",
  "sms_notifications": true
}
```

**Validation (server-side, before DB):**
1. If `phone_number` is provided, it must match E.164 regex `^\+[1-9]\d{7,14}$`.
2. If `sms_notifications: true` is submitted and no `phone_number` is stored (and none provided in this request), return `400`.

**Responses:**
- `200` — update applied
- `400` — validation failure (bad phone format, or SMS enabled without phone)
- `401` — not authenticated

---

## Models

### `src/models/sql/user.rs`

New struct `UserProfileRow`:
```rust
pub struct UserProfileRow {
    pub email: String,
    pub email_notifications: bool,
    pub phone_number: Option<String>,
    pub sms_notifications: bool,
}
```

### `src/models/http/user.rs`

New structs:

```rust
/// Response for GET /api/user/profile
pub struct ProfileResponse {
    pub email: String,
    pub email_notifications: bool,
    pub phone_number: Option<String>,
    pub sms_notifications: bool,
}

/// Request body for PUT /api/user/profile — all fields optional
pub struct UpdateProfileRequest {
    pub email_notifications: Option<bool>,
    pub phone_number: Option<String>,   // None = leave unchanged; Some("") = clear
    pub sms_notifications: Option<bool>,
}
```

`phone_number: Some("")` (empty string) clears the stored phone number and implicitly sets `sms_notifications = false`.

---

## Validation Logic

Implemented as a method on `UpdateProfileRequest`:

1. If `phone_number` is `Some(s)` and `s` is non-empty, validate against E.164 regex.
2. Determine the effective phone number after the update (new value, existing value, or cleared).
3. If effective `sms_notifications` is `true` and effective phone number is `None`/empty → `Err("sms_notifications requires a phone number")`.

---

## Files Changed

| File | Change |
|---|---|
| `migrations/007_add_notification_preferences.sql` | New migration |
| `src/models/sql/user.rs` | Add `UserProfileRow` |
| `src/models/http/user.rs` | Add `ProfileResponse`, `UpdateProfileRequest` |
| `src/controllers/user.rs` | Add `api_get_profile`, `api_update_profile`, register routes |

---

## Out of Scope

- Sending email or SMS (separate Python story)
- Exposing notification preference fields on signup or login responses
- Allowing users to change their email address or password via profile (separate story, FR-1.4)
