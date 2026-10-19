-- migrations/003_create_plaid_accounts.sql
-- Stores financial accounts fetched from Plaid for each Item.

CREATE TABLE IF NOT EXISTS plaid_accounts (
    id                SERIAL PRIMARY KEY,
    account_id        TEXT NOT NULL UNIQUE,
    item_id           TEXT NOT NULL REFERENCES plaid_links(item_id) ON DELETE CASCADE,
    name              TEXT NOT NULL,
    official_name     TEXT,
    account_type      TEXT NOT NULL,
    account_subtype   TEXT,
    mask              TEXT,
    current_balance   DOUBLE PRECISION,
    available_balance DOUBLE PRECISION,
    currency_code     TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_plaid_accounts_item_id ON plaid_accounts(item_id);
