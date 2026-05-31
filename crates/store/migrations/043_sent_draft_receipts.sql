-- Persist successful draft-send receipts so request retries can be
-- idempotent after the live draft row has been removed.

CREATE TABLE IF NOT EXISTS sent_draft_receipts (
    draft_id             TEXT PRIMARY KEY,
    account_id           TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    local_message_id     TEXT NOT NULL,
    provider_message_id  TEXT,
    rfc2822_message_id   TEXT NOT NULL,
    sent_at              INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sent_draft_receipts_account_sent_at
    ON sent_draft_receipts(account_id, sent_at DESC);
