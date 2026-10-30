-- Accounts a rule applies to.
-- A rule with no rows here applies to all of the user's accounts.

CREATE TABLE IF NOT EXISTS rule_accounts (
    rule_id    INT  NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES plaid_accounts(account_id) ON DELETE CASCADE,
    PRIMARY KEY (rule_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_rule_accounts_account_id ON rule_accounts(account_id);