CREATE TABLE IF NOT EXISTS triage_cache (
    message_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    verdict TEXT NOT NULL,
    verdict_line TEXT NOT NULL,
    reason TEXT NOT NULL,
    model TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (message_id, prompt_version),
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_triage_cache_account_verdict
    ON triage_cache(account_id, verdict, generated_at DESC);
