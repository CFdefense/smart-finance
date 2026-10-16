-- migrations/004_create_plaid_transactions.sql
-- Stores transactions synced from Plaid for each account.

CREATE TABLE IF NOT EXISTS plaid_transactions (
    id             SERIAL PRIMARY KEY,
    transaction_id TEXT NOT NULL UNIQUE,
    account_id     TEXT NOT NULL REFERENCES plaid_accounts(account_id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    amount         DOUBLE PRECISION NOT NULL,
    date           DATE NOT NULL,
    pending        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_plaid_transactions_account_id ON plaid_transactions(account_id);
CREATE INDEX IF NOT EXISTS idx_plaid_transactions_date ON plaid_transactions(date);
