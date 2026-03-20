CREATE TABLE IF NOT EXISTS sync_runtime_status (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    last_attempt_at INTEGER,
    last_success_at INTEGER,
    last_error TEXT,
    failure_class TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    backoff_until INTEGER,
    sync_in_progress INTEGER NOT NULL DEFAULT 0,
    current_cursor_summary TEXT,
    last_synced_count INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sync_runtime_status_updated_at
    ON sync_runtime_status(updated_at DESC);
