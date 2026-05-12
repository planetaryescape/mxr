CREATE TABLE IF NOT EXISTS contact_commitments (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email               TEXT NOT NULL COLLATE NOCASE,
    thread_id           TEXT NOT NULL,
    direction           TEXT NOT NULL CHECK (direction IN ('yours', 'theirs')),
    status              TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'expired')),
    who_owes            TEXT NOT NULL,
    what                TEXT NOT NULL,
    by_when             INTEGER,
    evidence_msg_id     TEXT NOT NULL,
    extracted_at        INTEGER NOT NULL,
    resolved_at         INTEGER,
    UNIQUE (account_id, email, thread_id, direction, what, evidence_msg_id)
);

CREATE INDEX IF NOT EXISTS idx_contact_commitments_lookup
    ON contact_commitments(account_id, email, status);
