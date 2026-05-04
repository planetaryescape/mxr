-- =========================================================================
-- Per-message state-transition log. Powers analytics queries that need
-- history rather than the snapshot in `messages.flags`.
--
-- Writes are emit-only on real transitions: callers in store/src/message.rs
-- detect (was X) -> (is Y) and only insert a row when Y differs from X.
--
-- `event_type` and `source` columns store the snake_case values from
-- `MessageEventType::as_db_str()` and `EventSource::as_db_str()` in
-- crates/core/src/types.rs. Keep both lists in sync with the CHECK
-- constraints below.
-- =========================================================================
CREATE TABLE IF NOT EXISTS message_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id      TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    event_type      TEXT NOT NULL CHECK (event_type IN (
        'read', 'unread',
        'starred', 'unstarred',
        'archived', 'unarchived',
        'trashed', 'untrashed',
        'labeled', 'unlabeled',
        'moved',
        'received', 'sent',
        'replied', 'forwarded',
        'snoozed', 'unsnoozed',
        'unsubscribed'
    )),
    source          TEXT NOT NULL CHECK (source IN (
        'user', 'rule_engine', 'sync', 'reconciler', 'doctor', 'external'
    )),
    label_id        TEXT REFERENCES labels(id) ON DELETE SET NULL,
    occurred_at     INTEGER NOT NULL,
    metadata_json   TEXT
);

CREATE INDEX IF NOT EXISTS idx_message_events_message
    ON message_events(message_id, occurred_at);

CREATE INDEX IF NOT EXISTS idx_message_events_account
    ON message_events(account_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_message_events_type
    ON message_events(event_type, occurred_at DESC);
