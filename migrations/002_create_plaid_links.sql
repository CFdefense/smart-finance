-- migrations/002_create_plaid_links.sql
-- Stores Plaid linked bank connections for each user.

CREATE TABLE IF NOT EXISTS plaid_links (
    id            SERIAL PRIMARY KEY,
    user_id       INT  NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id       TEXT NOT NULL UNIQUE,
    access_token  TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_plaid_links_user_id ON plaid_links(user_id);
