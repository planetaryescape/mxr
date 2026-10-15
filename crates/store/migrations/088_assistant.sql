-- =========================================================================
-- The in-app assistant's conversations.
--
-- assistant_messages is the transcript exactly as the model saw it, in
-- order, append-only: providers reject edited history, so rows are never
-- rewritten. message_json is mxr-llm's ToolChatMessage; meta_json holds the
-- model, token usage and per-call reasons. Tool results can contain mail
-- text, so these rows are as private as the mail itself and are deleted
-- with their conversation.
--
-- pending_json on the conversation lists the calls waiting for the user
-- (approvals and questions), so a waiting turn survives a daemon restart.
-- auto_approve is the user's choice to let reversible changes run without
-- asking. summary replaces every message up to summary_through_seq in what
-- the model reads, once the history outgrows its context window; the
-- messages themselves stay.
-- =========================================================================
CREATE TABLE IF NOT EXISTS assistant_conversations (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('idle', 'running', 'awaiting_user')),
    -- How the last turn ended: finished, cancelled, step_limit, failed or
    -- interrupted (the daemon stopped mid-turn). NULL before the first.
    last_outcome  TEXT,
    last_error    TEXT,
    pending_json  TEXT,
    auto_approve  INTEGER NOT NULL DEFAULT 0 CHECK (auto_approve IN (0, 1)),
    summary       TEXT,
    summary_through_seq INTEGER,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_assistant_conversations_updated
    ON assistant_conversations(updated_at DESC);

CREATE TABLE IF NOT EXISTS assistant_messages (
    conversation_id TEXT NOT NULL REFERENCES assistant_conversations(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    message_json    TEXT NOT NULL,
    meta_json       TEXT,
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, seq)
);
