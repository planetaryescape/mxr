-- An evidenced later message is an offer, never an automatic task mutation.
-- Keep history so a newer message can supersede an offer without losing the
-- source or the snapshot needed to undo an accepted change.
CREATE TABLE IF NOT EXISTS todo_resolution_suggestions (
    id                  TEXT PRIMARY KEY,
    todo_id             TEXT NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source_message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_thread_id    TEXT NOT NULL,
    source_date         INTEGER NOT NULL,
    source_quote        TEXT NOT NULL,
    quote_location      TEXT NOT NULL CHECK (quote_location IN ('subject', 'body')),
    source_method       TEXT NOT NULL CHECK (source_method IN ('rule', 'model')),
    action              TEXT NOT NULL CHECK (action IN ('complete', 'revise_date')),
    proposed_due_at     INTEGER,
    proposed_due_words  TEXT,
    task_updated_at     INTEGER NOT NULL,
    task_fingerprint    TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'dismissed', 'undone', 'superseded')),
    snapshot_json       TEXT,
    applied_updated_at  INTEGER,
    applied_fingerprint TEXT,
    created_at          INTEGER NOT NULL,
    CHECK ((action = 'revise_date' AND proposed_due_at IS NOT NULL AND proposed_due_words IS NOT NULL)
        OR (action = 'complete' AND proposed_due_at IS NULL AND proposed_due_words IS NULL))
);

-- Only determinate, relevant later evidence advances this ordered watermark.
-- It survives a dismissal or undo so an older replay cannot revive an offer.
CREATE TABLE IF NOT EXISTS todo_resolution_evidence (
    todo_id           TEXT NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_date       INTEGER NOT NULL,
    outcome           TEXT NOT NULL CHECK (outcome IN ('offer', 'contradiction')),
    PRIMARY KEY (todo_id, source_message_id)
);

CREATE INDEX IF NOT EXISTS idx_todo_resolution_evidence_order
    ON todo_resolution_evidence (todo_id, source_date DESC, source_message_id DESC);

CREATE INDEX IF NOT EXISTS idx_todo_resolution_todo_order
    ON todo_resolution_suggestions (todo_id, source_date DESC, source_message_id DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_todo_resolution_source
    ON todo_resolution_suggestions (todo_id, source_message_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_todo_resolution_pending
    ON todo_resolution_suggestions (todo_id) WHERE status = 'pending';
