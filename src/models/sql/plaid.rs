//! SQL row models for Plaid tables.

use chrono::{DateTime, Utc};

/// Row model for the `plaid_links` table.
///
/// Represents a linked bank connection (Plaid Item) for a user.
pub struct PlaidLinkRow {
    /// Primary key.
    pub id: i32,
    /// FK to `users.id`.
    pub user_id: i32,
    /// Plaid's unique identifier for this Item.
    pub item_id: String,
    /// Plaid access token for this Item. Never exposed in API responses.
    pub access_token: String,
    /// When the Item was linked.
    pub created_at: DateTime<Utc>,
}

/// Row model for the `plaid_sync_cursors` table.
///
/// Tracks the Transactions Sync cursor for each Plaid Item.
pub struct PlaidSyncCursorRow {
    /// Primary key.
    pub id: i32,
    /// FK to `plaid_links.item_id`.
    pub item_id: String,
    /// Opaque cursor string returned by Plaid. `None` before the first sync.
    pub cursor: Option<String>,
    /// Whether Plaid has more pages to return for the current sync window.
    pub has_more: bool,
    /// When the cursor was last updated.
    pub updated_at: DateTime<Utc>,
}
