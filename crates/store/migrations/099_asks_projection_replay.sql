-- One replaceable outbox entry per source: no account-wide replay scan.
CREATE TABLE IF NOT EXISTS asks_projection_outbox (
    message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    input_revision INTEGER NOT NULL,
    input_fingerprint TEXT NOT NULL,
    context_token TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_asks_projection_account ON asks_projection_outbox(account_id, message_id);
-- Ambiguous protected history suppresses each possible successor without
-- assigning the user's decision to an arbitrary one of them.
CREATE TABLE IF NOT EXISTS asks_legacy_links (
    todo_id TEXT NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
    dedup_key TEXT NOT NULL,
    PRIMARY KEY(todo_id, dedup_key)
);
