-- Accounts a rule applies to.
-- A rule with no rows here applies to all of the user's accounts.

CREATE TABLE IF NOT EXISTS rule_accounts (
    rule_id    INT  NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL,
    PRIMARY KEY (rule_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_rule_accounts_account_id ON rule_accounts(account_id);