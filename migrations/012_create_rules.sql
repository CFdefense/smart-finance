-- User-defined natural-language rules.
-- Each rule belongs to a user and can target specific accounts or all accounts.
-- An empty account_ids array means the rule applies to all accounts.

CREATE TABLE IF NOT EXISTS rules (
    id          SERIAL PRIMARY KEY,
    user_id     INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    description TEXT NOT NULL,
    account_ids TEXT[] NOT NULL DEFAULT '{}',
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rules_user_id ON rules(user_id);
CREATE INDEX IF NOT EXISTS idx_rules_account_ids ON rules USING GIN(account_ids);