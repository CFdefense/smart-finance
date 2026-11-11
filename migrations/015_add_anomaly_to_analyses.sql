-- Store the anomaly detector's result alongside each transaction analysis.
-- anomaly_flagged: NULL = detection didn't run or failed, FALSE = checked and nothing unusual,
-- TRUE = flagged. anomaly_reasons holds the reasons, joined with "; ". 

ALTER TABLE transaction_analyses
    ADD COLUMN IF NOT EXISTS anomaly_flagged BOOLEAN,
    ADD COLUMN IF NOT EXISTS anomaly_reason TEXT;