-- migrations/011_create_goals.sql
-- Account-balance goals: "get this account's balance to this amount".
-- Progress is derived on read from the linked account's current balance:
--   percent = (current_balance - baseline_balance) / (target_amount - baseline_balance)
-- so nothing derived is stored. baseline_balance is the balance when the goal was created.
-- A goal exists only for its account, so removing the account removes its goals.

CREATE TABLE IF NOT EXISTS goals (
    id               SERIAL PRIMARY KEY,
    user_id          INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    account_id       TEXT NOT NULL REFERENCES plaid_accounts(account_id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    description      TEXT,
    target_amount    DOUBLE PRECISION NOT NULL CHECK (target_amount > 0),
    baseline_balance DOUBLE PRECISION NOT NULL,
    target_date      DATE,
    archived         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_goals_user_id ON goals(user_id);
CREATE INDEX IF NOT EXISTS idx_goals_account_id ON goals(account_id);