-- migrations/012_create_rules.sql
-- User-defined spending rules. `description` keeps the user's own wording (free text);
-- the structured columns are what is evaluated. Natural-language parsing can populate the
-- structured columns later without a schema change.

CREATE TABLE IF NOT EXISTS rules (
    id          SERIAL PRIMARY KEY,
    user_id     INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rule_type   TEXT NOT NULL,
    account_id  TEXT REFERENCES plaid_accounts(account_id) ON DELETE CASCADE,
    category    TEXT,
    amount      DOUBLE PRECISION,
    description TEXT NOT NULL,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT rules_type_valid CHECK (rule_type IN (
        'MIN_BALANCE', 'MAX_TRANSACTION', 'SPEND_ONLY_ON_ACCOUNT', 'NO_SPEND_ON_ACCOUNT'
    )),
    CONSTRAINT rules_category_valid CHECK (category IS NULL OR category IN (
        'SHOPPING', 'ENTERTAINMENT', 'UTILITIES', 'HEALTHCARE',
        'FOOD_DINING', 'GROCERIES', 'TRANSPORTATION', 'OTHER'
    ))
);

CREATE INDEX IF NOT EXISTS idx_rules_user_id ON rules(user_id);
CREATE INDEX IF NOT EXISTS idx_rules_account_id ON rules(account_id);
