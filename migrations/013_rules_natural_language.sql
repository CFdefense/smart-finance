-- migrations/013_rules_natural_language.sql
-- Rules are plain sentences written by the user and consumed by the AI agent; the backend
-- does not interpret them. A rule targets one or more accounts, or every account.
--   * `description` is the user's sentence.
--   * `rule_accounts` lists the accounts a rule targets.
--   * `all_accounts` = TRUE means "every account I own, now and later". When it is FALSE and
--     all targeted accounts are deleted, the rule targets nothing (it never silently widens).
-- The structured columns from 012 (type / account / category / amount) are removed.

ALTER TABLE rules ADD COLUMN IF NOT EXISTS all_accounts BOOLEAN NOT NULL DEFAULT TRUE;

CREATE TABLE IF NOT EXISTS rule_accounts (
    rule_id    INT  NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES plaid_accounts(account_id) ON DELETE CASCADE,
    PRIMARY KEY (rule_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_rule_accounts_account_id ON rule_accounts(account_id);

-- Carry over the old single-account scope.
INSERT INTO rule_accounts (rule_id, account_id)
SELECT id, account_id FROM rules WHERE account_id IS NOT NULL
ON CONFLICT DO NOTHING;

UPDATE rules SET all_accounts = FALSE WHERE account_id IS NOT NULL;

DROP INDEX IF EXISTS idx_rules_account_id;
ALTER TABLE rules DROP COLUMN IF EXISTS account_id;
ALTER TABLE rules DROP COLUMN IF EXISTS category;
ALTER TABLE rules DROP COLUMN IF EXISTS amount;
ALTER TABLE rules DROP COLUMN IF EXISTS rule_type;
