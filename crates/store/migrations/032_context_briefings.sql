-- Slice 5.1 / 5.2 of docs/reference/ai-email.md
-- Cached briefings for threads and recipients. The content_hash is
-- a hash of the input prompt context: identical inputs reuse the
-- cached row, content drift invalidates it.

CREATE TABLE IF NOT EXISTS context_briefings (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('thread', 'recipient')),
    subject_key     TEXT NOT NULL,                 -- thread_id or lower(email)
    content_hash    TEXT NOT NULL,
    body_markdown   TEXT NOT NULL,
    citations_json  TEXT NOT NULL,                 -- JSON array of CitationRef
    generated_at    INTEGER NOT NULL,
    UNIQUE (account_id, kind, subject_key)
);

CREATE INDEX IF NOT EXISTS idx_context_briefings_lookup
    ON context_briefings(account_id, kind, subject_key);
