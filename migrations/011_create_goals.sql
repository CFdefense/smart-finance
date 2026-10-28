-- migrations/011_create_goals.sql
-- Savings / financial goals. Progress is derived on read:
--   current = SUM(goal_contributions.amount) + (linked account balance - baseline_balance)
-- so nothing derived is stored. Linking an account is optional; if the account is later
-- disconnected the goal simply keeps its manual contributions.

CREATE TABLE IF NOT EXISTS goals (
    id               SERIAL PRIMARY KEY,
    user_id          INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    description      TEXT,
    target_amount    DOUBLE PRECISION NOT NULL CHECK (target_amount > 0),
    target_date      DATE,
    account_id       TEXT REFERENCES plaid_accounts(account_id) ON DELETE SET NULL,
    baseline_balance DOUBLE PRECISION,
    archived         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_goals_user_id ON goals(user_id);
CREATE INDEX IF NOT EXISTS idx_goals_account_id ON goals(account_id);

-- Ledger of manual deposits (positive) and withdrawals (negative).
CREATE TABLE IF NOT EXISTS goal_contributions (
    id                SERIAL PRIMARY KEY,
    goal_id           INT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    amount            DOUBLE PRECISION NOT NULL CHECK (amount <> 0),
    contribution_date DATE NOT NULL DEFAULT CURRENT_DATE,
    note              TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_goal_contributions_goal_id ON goal_contributions(goal_id);
