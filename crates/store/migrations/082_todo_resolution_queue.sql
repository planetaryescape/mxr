-- Processing intent stays local.
-- Completed and exhausted revisions remain to prevent repeated sync replay.
CREATE TABLE IF NOT EXISTS todo_resolution_queue (
    todo_id TEXT NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    input_fingerprint TEXT NOT NULL,
    source_date INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    next_attempt_at INTEGER NOT NULL,
    completed INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    PRIMARY KEY (account_id, todo_id, message_id)
);
CREATE INDEX IF NOT EXISTS idx_todo_resolution_queue_due
    ON todo_resolution_queue (next_attempt_at, source_date DESC, message_id DESC)
    WHERE completed = 0 AND attempts < 5;
