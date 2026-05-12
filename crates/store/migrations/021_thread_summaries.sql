CREATE TABLE IF NOT EXISTS thread_summaries (
    thread_id     TEXT PRIMARY KEY,
    account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    content_hash  TEXT NOT NULL,
    text          TEXT NOT NULL,
    model         TEXT NOT NULL,
    generated_at  INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_thread_summaries_account
    ON thread_summaries(account_id, updated_at DESC);
