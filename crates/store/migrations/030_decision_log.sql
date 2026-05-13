-- Slice 3.2 of docs/ai-email/03-archive-intelligence.md.
-- Decision log entries extracted from threads. Stable id is a hash of
-- (account_id, thread_id, normalized decision text, evidence msg ids)
-- so a re-extraction with unchanged content is idempotent.

CREATE TABLE IF NOT EXISTS decision_log (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id           TEXT NOT NULL,
    topic               TEXT,
    decision            TEXT NOT NULL,
    rationale           TEXT,
    evidence_msg_ids    TEXT NOT NULL,         -- JSON array of msg ids
    decided_at          INTEGER,
    extracted_at        INTEGER NOT NULL,
    source_hash         TEXT NOT NULL,
    UNIQUE (account_id, thread_id, source_hash)
);

CREATE INDEX IF NOT EXISTS idx_decision_log_account_topic
    ON decision_log(account_id, topic);

CREATE INDEX IF NOT EXISTS idx_decision_log_account_decided
    ON decision_log(account_id, decided_at DESC);
