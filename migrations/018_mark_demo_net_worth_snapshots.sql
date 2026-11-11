
-- Migration 016
--
-- Distinguish synthetic net-worth snapshots from
-- snapshots recorded from connected financial accounts.
--
-- All existing records default to FALSE, meaning the
-- migration preserves their real recorded status.

ALTER TABLE net_worth_snapshots
ADD COLUMN IF NOT EXISTS is_demo BOOLEAN NOT NULL DEFAULT FALSE;
