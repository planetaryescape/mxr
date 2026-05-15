-- Decision log stable-id semantics.
--
-- 030 originally added UNIQUE(account_id, thread_id, source_hash), which
-- collapsed multiple decisions from the same thread extraction and made a
-- changed source hash insert a duplicate primary decision. The stable id is
-- already the primary key, so keep id as the conflict target and preserve
-- source_hash as refresh metadata.

PRAGMA foreign_keys = OFF;

CREATE TABLE decision_log_v2 (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id           TEXT NOT NULL,
    topic               TEXT,
    decision            TEXT NOT NULL,
    rationale           TEXT,
    evidence_msg_ids    TEXT NOT NULL,
    decided_at          INTEGER,
    extracted_at        INTEGER NOT NULL,
    source_hash         TEXT NOT NULL
);

INSERT OR REPLACE INTO decision_log_v2
    (id, account_id, thread_id, topic, decision, rationale,
     evidence_msg_ids, decided_at, extracted_at, source_hash)
SELECT
    id, account_id, thread_id, topic, decision, rationale,
    evidence_msg_ids, decided_at, extracted_at, source_hash
FROM decision_log;

DROP TABLE decision_log;
ALTER TABLE decision_log_v2 RENAME TO decision_log;

CREATE INDEX IF NOT EXISTS idx_decision_log_account_topic
    ON decision_log(account_id, topic);

CREATE INDEX IF NOT EXISTS idx_decision_log_account_decided
    ON decision_log(account_id, decided_at DESC);

PRAGMA foreign_keys = ON;
