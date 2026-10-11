-- Keep a cycle's identity independently of mutable subscription grouping
-- and user-editable task fields. Aliases survive source deletion/backfill.
CREATE TABLE IF NOT EXISTS record_renewal_cycles (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    record_id TEXT NOT NULL,
    dedup_key TEXT NOT NULL,
    PRIMARY KEY (account_id, record_id)
);
