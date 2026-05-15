-- =========================================================================
-- FTS5 mirror of user_activity.context_json. Lets the user search free-text
-- over subjects, recipient handles, search queries, draft prefixes, URLs.
--
-- Triggers keep the mirror in sync with the base table. On redaction we
-- clear `context_json` to NULL — the update trigger removes the FTS row
-- and does not re-insert (NULL is skipped at the SQLite level).
-- =========================================================================
CREATE VIRTUAL TABLE IF NOT EXISTS user_activity_fts USING fts5(
    context_json,
    content='user_activity',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS user_activity_ai AFTER INSERT ON user_activity
BEGIN
    INSERT INTO user_activity_fts (rowid, context_json)
    VALUES (new.id, new.context_json);
END;

CREATE TRIGGER IF NOT EXISTS user_activity_ad AFTER DELETE ON user_activity
BEGIN
    INSERT INTO user_activity_fts (user_activity_fts, rowid, context_json)
    VALUES ('delete', old.id, old.context_json);
END;

CREATE TRIGGER IF NOT EXISTS user_activity_au AFTER UPDATE ON user_activity
BEGIN
    INSERT INTO user_activity_fts (user_activity_fts, rowid, context_json)
    VALUES ('delete', old.id, old.context_json);
    INSERT INTO user_activity_fts (rowid, context_json)
    VALUES (new.id, new.context_json);
END;
