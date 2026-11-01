-- Row attention is local and independent of provider read flags. Keep exact
-- revisions: a late acknowledgement of an old peek cannot calm newer content.
CREATE TABLE IF NOT EXISTS mode_row_seen (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    mode TEXT NOT NULL CHECK (mode IN ('todo', 'updates')),
    row_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (account_id, mode, row_id, revision)
);
