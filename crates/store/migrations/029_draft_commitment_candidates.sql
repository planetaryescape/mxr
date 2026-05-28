-- Slice 2.1 of docs/reference/ai-email.md
-- Per-draft commitment candidates extracted before send. Promoted to
-- contact_commitments after a successful send. Deleted when the draft
-- is sent or discarded.

CREATE TABLE IF NOT EXISTS draft_commitment_candidates (
    id                  TEXT PRIMARY KEY,
    draft_id            TEXT NOT NULL,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email               TEXT NOT NULL COLLATE NOCASE,
    direction           TEXT NOT NULL CHECK (direction IN ('yours', 'theirs')),
    who_owes            TEXT NOT NULL,
    what                TEXT NOT NULL,
    by_when             INTEGER,
    extracted_at        INTEGER NOT NULL,
    UNIQUE (draft_id, email, direction, what)
);

CREATE INDEX IF NOT EXISTS idx_draft_commitment_candidates_draft
    ON draft_commitment_candidates(draft_id);
