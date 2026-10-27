ALTER TABLE users
ADD COLUMN first_name TEXT NOT NULL DEFAULT '',
ADD COLUMN last_name TEXT NOT NULL DEFAULT '',
ADD COLUMN account_status TEXT NOT NULL DEFAULT 'active'
    CHECK (account_status IN ('active', 'suspended', 'disabled'));
