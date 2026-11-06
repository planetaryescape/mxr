-- What the receiving provider said about a message when it arrived: its own
-- receipt time and whether it vouched for the From host (blueprint 22, D193).
-- Written once by the sync that stores the message and never recomputed, so
-- the sender's Date header and later reclassification cannot change it.
CREATE TABLE IF NOT EXISTS message_arrivals (
    message_id         TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id         TEXT NOT NULL,
    from_host          TEXT NOT NULL,
    -- Unix seconds of the provider's receipt; NULL when it gave none.
    received_at        INTEGER,
    -- Set only when the provider's own verdict passed DMARC for from_host.
    verified_host      TEXT,
    unverified_reason  TEXT,
    is_live            INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_message_arrivals_host
    ON message_arrivals(account_id, from_host, received_at);

-- Existing message dates cannot establish receiver receipt: Gmail historically
-- falls back to Date when internalDate is absent, and imports may choose Date.
-- History begins with provenance recorded by a supported receiver adapter.

-- A claim survives source deletion, re-sync and rules-version changes. Only
-- account deletion clears it; it contains identifiers, never message content.
CREATE TABLE IF NOT EXISTS alert_todo_claims (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    dedup_key TEXT NOT NULL,
    message_id TEXT NOT NULL,
    alert_kind TEXT NOT NULL,
    PRIMARY KEY (account_id, dedup_key),
    UNIQUE (account_id, message_id, alert_kind)
);
