
-- Historical net-worth snapshots.
--
-- Each row represents one user's linked-account net worth
-- on a particular calendar date.
--
-- The UNIQUE constraint guarantees that repeated account
-- refreshes on the same day update rather than duplicate
-- the snapshot.

CREATE TABLE IF NOT EXISTS net_worth_snapshots (
    id SERIAL PRIMARY KEY,

    -- Every snapshot belongs to an application user.
    -- Deleting that user removes their financial history.
    user_id INT NOT NULL
        REFERENCES users(id) ON DELETE CASCADE,

    snapshot_date DATE NOT NULL DEFAULT CURRENT_DATE,

    -- Current linked-account net worth in USD.
    -- This is assets minus liabilities.
    amount DOUBLE PRECISION NOT NULL,

    -- Useful metadata when explaining partial snapshots.
    included_accounts INT NOT NULL,
    excluded_accounts INT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- At most one snapshot per user per calendar day.
    CONSTRAINT unique_user_net_worth_date
        UNIQUE (user_id, snapshot_date),

    CONSTRAINT valid_net_worth_account_counts
        CHECK (
            included_accounts > 0
            AND excluded_accounts >= 0
        ),

    CONSTRAINT finite_net_worth_amount
        CHECK (amount > '-Infinity'::float8
               AND amount < 'Infinity'::float8)
);

-- Optimize historical queries for one user in date order.
CREATE INDEX IF NOT EXISTS idx_net_worth_snapshots_user_date
    ON net_worth_snapshots(user_id, snapshot_date);
