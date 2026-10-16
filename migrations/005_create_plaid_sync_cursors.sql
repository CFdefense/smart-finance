-- migrations/005_create_plaid_sync_cursors.sql
-- Tracks the Plaid Transactions Sync cursor per Item for incremental syncs.

CREATE TABLE IF NOT EXISTS plaid_sync_cursors (
    id         SERIAL PRIMARY KEY,
    item_id    TEXT NOT NULL UNIQUE REFERENCES plaid_links(item_id) ON DELETE CASCADE,
    cursor     TEXT,
    has_more   BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
