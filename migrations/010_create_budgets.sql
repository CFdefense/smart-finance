-- migrations/010_create_budgets.sql
-- Recurring monthly budgets: one per user per spending category.
-- Spent amounts are computed from plaid_transactions on read, never stored.

CREATE TABLE IF NOT EXISTS budgets (
    id            SERIAL PRIMARY KEY,
    user_id       INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    category      TEXT NOT NULL,
    monthly_limit DOUBLE PRECISION NOT NULL CHECK (monthly_limit > 0),
    active        BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT budgets_category_valid CHECK (category IN (
        'SHOPPING', 'ENTERTAINMENT', 'UTILITIES', 'HEALTHCARE',
        'FOOD_DINING', 'GROCERIES', 'TRANSPORTATION', 'OTHER'
    )),
    CONSTRAINT budgets_user_category_unique UNIQUE (user_id, category)
);

CREATE INDEX IF NOT EXISTS idx_budgets_user_id ON budgets(user_id);
