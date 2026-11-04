-- Tracks which transactions the analyzer agent has claimed, so each is analyzed once.
-- 'started' rows older than 10 minutes and 'failed' rows can be claimed again.

CREATE TABLE IF NOT EXISTS transaction_analyses (
    transaction_id TEXT PRIMARY KEY,
    user_id        INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status         TEXT NOT NULL CHECK (status IN ('started', 'done', 'failed')),
    verdict        TEXT,
    notified       BOOLEAN NOT NULL DEFAULT FALSE,
    error          TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transaction_analyses_user_id ON transaction_analyses(user_id);
