CREATE TABLE IF NOT EXISTS contact_relationship_summary (
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email               TEXT NOT NULL COLLATE NOCASE,
    text                TEXT NOT NULL,
    model               TEXT NOT NULL,
    known_topics_json   TEXT NOT NULL DEFAULT '[]',
    computed_at         INTEGER NOT NULL,
    source_hash         TEXT NOT NULL,
    last_error          TEXT,
    PRIMARY KEY (account_id, email)
);

CREATE INDEX IF NOT EXISTS idx_contact_relationship_summary_computed_at
    ON contact_relationship_summary(account_id, computed_at DESC);
