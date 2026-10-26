-- Rebuildable thread facts for Messages history, not cached lane decisions.
CREATE TABLE IF NOT EXISTS conversation_summaries (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL,
    input_revision INTEGER NOT NULL,
    view_revision INTEGER NOT NULL,
    context TEXT NOT NULL,
    valid_from INTEGER NOT NULL,
    valid_until INTEGER NOT NULL,
    summary_json TEXT NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);

-- Assessment generations deliberately ignore read/inbox changes and assessment
-- writes at the same input generation. A separate token covers those inputs.
CREATE TABLE IF NOT EXISTS conversation_view_inputs (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);

-- Candidate membership is an input projection too. Its generation is global
-- because a sender/rule change can make an old, previously excluded thread a
-- candidate. No display rows, limits or lane state are stored in this set.
CREATE TABLE IF NOT EXISTS conversation_facet_clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL
);
INSERT OR IGNORE INTO conversation_facet_clock VALUES (1, 0);
CREATE TABLE IF NOT EXISTS conversation_candidate_sets (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    context TEXT NOT NULL,
    input_revision INTEGER NOT NULL,
    facet_revision INTEGER NOT NULL,
    valid_from INTEGER NOT NULL,
    valid_until INTEGER NOT NULL,
    thread_ids_json TEXT NOT NULL
);

CREATE TRIGGER IF NOT EXISTS conversation_flags_update AFTER UPDATE OF flags ON messages
WHEN OLD.flags IS NOT NEW.flags
BEGIN
    INSERT INTO conversation_view_inputs VALUES (NEW.account_id, NEW.thread_id, 1)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;

CREATE TRIGGER IF NOT EXISTS conversation_facet_insert AFTER INSERT ON mail_assessments
BEGIN
    UPDATE conversation_facet_clock SET revision = revision + 1 WHERE id = 1;
    INSERT INTO conversation_view_inputs
    SELECT account_id, thread_id, 1 FROM messages WHERE id = NEW.message_id
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = messages.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;
CREATE TRIGGER IF NOT EXISTS conversation_facet_update AFTER UPDATE ON mail_assessments
BEGIN
    UPDATE conversation_facet_clock SET revision = revision + 1 WHERE id = 1;
    INSERT INTO conversation_view_inputs
    SELECT DISTINCT account_id, thread_id, 1 FROM messages WHERE id IN (OLD.message_id, NEW.message_id)
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = messages.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;
CREATE TRIGGER IF NOT EXISTS conversation_facet_delete AFTER DELETE ON mail_assessments
BEGIN
    UPDATE conversation_facet_clock SET revision = revision + 1 WHERE id = 1;
    INSERT INTO conversation_view_inputs
    SELECT account_id, thread_id, 1 FROM messages WHERE id = OLD.message_id
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = messages.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;

CREATE TRIGGER IF NOT EXISTS conversation_label_insert AFTER INSERT ON message_labels
BEGIN
    INSERT INTO conversation_view_inputs
    SELECT account_id, thread_id, 1 FROM messages WHERE id = NEW.message_id
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = messages.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;
CREATE TRIGGER IF NOT EXISTS conversation_label_update AFTER UPDATE ON message_labels
BEGIN
    INSERT INTO conversation_view_inputs
    SELECT DISTINCT account_id, thread_id, 1 FROM messages WHERE id IN (OLD.message_id, NEW.message_id)
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = messages.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;
CREATE TRIGGER IF NOT EXISTS conversation_label_delete AFTER DELETE ON message_labels
BEGIN
    INSERT INTO conversation_view_inputs
    SELECT account_id, thread_id, 1 FROM messages WHERE id = OLD.message_id
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = messages.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;
CREATE TRIGGER IF NOT EXISTS conversation_label_role_update AFTER UPDATE OF provider_id ON labels
WHEN OLD.provider_id IS NOT NEW.provider_id
BEGIN
    INSERT INTO conversation_view_inputs
    SELECT DISTINCT m.account_id, m.thread_id, 1 FROM message_labels ml
    JOIN messages m ON m.id = ml.message_id WHERE ml.label_id = NEW.id
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = m.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;
CREATE TRIGGER IF NOT EXISTS conversation_label_role_delete BEFORE DELETE ON labels
BEGIN
    INSERT INTO conversation_view_inputs
    SELECT DISTINCT m.account_id, m.thread_id, 1 FROM message_labels ml
    JOIN messages m ON m.id = ml.message_id WHERE ml.label_id = OLD.id
        AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = m.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = revision + 1;
END;

-- A deleted thread must not leave a large, unreachable cached synopsis.
CREATE TRIGGER IF NOT EXISTS conversation_message_delete AFTER DELETE ON messages
BEGIN
    DELETE FROM conversation_candidate_sets WHERE account_id = OLD.account_id;
    DELETE FROM conversation_summaries WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id;
    DELETE FROM conversation_view_inputs WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id
        AND NOT EXISTS (SELECT 1 FROM messages WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id);
END;
