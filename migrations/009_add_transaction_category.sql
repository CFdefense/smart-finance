-- migrations/009_add_transaction_category.sql
-- Adds a spending category to each synced transaction so budgets can be tracked.
-- `category_overridden` protects user-chosen categories from being reset by a re-sync.

ALTER TABLE plaid_transactions
    ADD COLUMN category TEXT NOT NULL DEFAULT 'OTHER',
    ADD COLUMN category_overridden BOOLEAN NOT NULL DEFAULT FALSE,
    ADD CONSTRAINT plaid_transactions_category_valid CHECK (category IN (
        'SHOPPING', 'ENTERTAINMENT', 'UTILITIES', 'HEALTHCARE',
        'FOOD_DINING', 'GROCERIES', 'TRANSPORTATION', 'OTHER'
    ));

CREATE INDEX IF NOT EXISTS idx_plaid_transactions_category ON plaid_transactions(category);
