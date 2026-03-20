-- =========================================================================
-- Accounts
-- =========================================================================
CREATE TABLE IF NOT EXISTS accounts (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    email           TEXT NOT NULL,
    sync_provider   TEXT,
    send_provider   TEXT,
    sync_config     TEXT,
    send_config     TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1,
    sync_cursor     TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- =========================================================================
-- Labels
-- =========================================================================
CREATE TABLE IF NOT EXISTS labels (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('system', 'folder', 'user')),
    color           TEXT,
    provider_id     TEXT NOT NULL,
    unread_count    INTEGER NOT NULL DEFAULT 0,
    total_count     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, provider_id)
);

CREATE INDEX IF NOT EXISTS idx_labels_account ON labels(account_id);

-- =========================================================================
-- Messages
-- =========================================================================
CREATE TABLE IF NOT EXISTS messages (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    provider_id         TEXT NOT NULL,
    thread_id           TEXT NOT NULL,
    message_id_header   TEXT,
    in_reply_to         TEXT,
    reference_headers   TEXT,
    from_name           TEXT,
    from_email          TEXT NOT NULL,
    to_addrs            TEXT NOT NULL,
    cc_addrs            TEXT NOT NULL DEFAULT '[]',
    bcc_addrs           TEXT NOT NULL DEFAULT '[]',
    subject             TEXT NOT NULL DEFAULT '',
    date                INTEGER NOT NULL,
    flags               INTEGER NOT NULL DEFAULT 0,
    snippet             TEXT NOT NULL DEFAULT '',
    has_attachments     INTEGER NOT NULL DEFAULT 0,
    size_bytes          INTEGER NOT NULL DEFAULT 0,
    unsubscribe_method  TEXT,
    UNIQUE (account_id, provider_id)
);

CREATE INDEX IF NOT EXISTS idx_messages_account ON messages(account_id);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(date DESC);
CREATE INDEX IF NOT EXISTS idx_messages_from ON messages(from_email);
CREATE INDEX IF NOT EXISTS idx_messages_flags ON messages(flags);

-- =========================================================================
-- Message-label junction
-- =========================================================================
CREATE TABLE IF NOT EXISTS message_labels (
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    label_id    TEXT NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, label_id)
);

CREATE INDEX IF NOT EXISTS idx_message_labels_label ON message_labels(label_id);

-- =========================================================================
-- Bodies
-- =========================================================================
CREATE TABLE IF NOT EXISTS bodies (
    message_id  TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    text_plain  TEXT,
    text_html   TEXT,
    fetched_at  INTEGER NOT NULL
);

-- =========================================================================
-- Attachments
-- =========================================================================
CREATE TABLE IF NOT EXISTS attachments (
    id          TEXT PRIMARY KEY,
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename    TEXT NOT NULL,
    mime_type   TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    local_path  TEXT,
    provider_id TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attachments_message ON attachments(message_id);

-- =========================================================================
-- Provider metadata
-- =========================================================================
CREATE TABLE IF NOT EXISTS provider_meta (
    message_id      TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,
    remote_id       TEXT NOT NULL,
    thread_remote_id TEXT,
    sync_token      TEXT,
    raw_labels      TEXT,
    mailbox_id      TEXT,
    uid_validity    INTEGER,
    raw_json        TEXT,
    PRIMARY KEY (message_id, provider)
);

-- =========================================================================
-- Drafts
-- =========================================================================
CREATE TABLE IF NOT EXISTS drafts (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    in_reply_to     TEXT,
    to_addrs        TEXT NOT NULL DEFAULT '[]',
    cc_addrs        TEXT NOT NULL DEFAULT '[]',
    bcc_addrs       TEXT NOT NULL DEFAULT '[]',
    subject         TEXT NOT NULL DEFAULT '',
    body_markdown   TEXT NOT NULL DEFAULT '',
    attachments     TEXT NOT NULL DEFAULT '[]',
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- =========================================================================
-- Snoozed
-- =========================================================================
CREATE TABLE IF NOT EXISTS snoozed (
    message_id      TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    snoozed_at      INTEGER NOT NULL,
    wake_at         INTEGER NOT NULL,
    original_labels TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_snoozed_wake ON snoozed(wake_at);

-- =========================================================================
-- Saved searches
-- =========================================================================
CREATE TABLE IF NOT EXISTS saved_searches (
    id          TEXT PRIMARY KEY,
    account_id  TEXT,
    name        TEXT NOT NULL,
    query       TEXT NOT NULL,
    sort_order  TEXT NOT NULL DEFAULT 'date_desc',
    icon        TEXT,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

-- =========================================================================
-- Rules
-- =========================================================================
CREATE TABLE IF NOT EXISTS rules (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    priority    INTEGER NOT NULL DEFAULT 0,
    conditions  TEXT NOT NULL,
    actions     TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS rule_execution_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    message_id TEXT NOT NULL,
    actions_applied TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 1,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_rule_log_rule_id ON rule_execution_log(rule_id);
CREATE INDEX IF NOT EXISTS idx_rule_log_message_id ON rule_execution_log(message_id);
CREATE INDEX IF NOT EXISTS idx_rule_log_timestamp ON rule_execution_log(timestamp);

-- =========================================================================
-- FTS5
-- =========================================================================
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    subject, from_email, from_name, snippet,
    content='messages',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, subject, from_email, from_name, snippet)
    VALUES (new.rowid, new.subject, new.from_email, new.from_name, new.snippet);
END;

CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, from_email, from_name, snippet)
    VALUES ('delete', old.rowid, old.subject, old.from_email, old.from_name, old.snippet);
END;

CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, from_email, from_name, snippet)
    VALUES ('delete', old.rowid, old.subject, old.from_email, old.from_name, old.snippet);
    INSERT INTO messages_fts(rowid, subject, from_email, from_name, snippet)
    VALUES (new.rowid, new.subject, new.from_email, new.from_name, new.snippet);
END;

-- =========================================================================
-- Sync log
-- =========================================================================
CREATE TABLE IF NOT EXISTS sync_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    status          TEXT NOT NULL CHECK (status IN ('running', 'success', 'error')),
    messages_synced INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_log_account ON sync_log(account_id, started_at DESC);

-- =========================================================================
-- Event log (A006)
-- =========================================================================
CREATE TABLE IF NOT EXISTS event_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   INTEGER NOT NULL,
    level       TEXT NOT NULL CHECK (level IN ('error', 'warn', 'info')),
    category    TEXT NOT NULL,
    account_id  TEXT,
    message_id  TEXT,
    rule_id     TEXT,
    summary     TEXT NOT NULL,
    details     TEXT,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_log_time ON event_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_category ON event_log(category, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_event_log_level ON event_log(level, timestamp DESC);
