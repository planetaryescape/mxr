-- =========================================================================
-- user_activity accepts every client kind.
--
-- 035 allowed only 'tui', 'cli', 'web' and 'daemon', so rows from every
-- other client (human, script, agent, mcp, and the in-app assistant) failed
-- the CHECK and the fire-and-forget recorder dropped them with a warning:
-- nothing an agent did through MCP was ever in the activity log. SQLite
-- can't alter a CHECK, so the table is rebuilt with the same columns, ids
-- and rows, and its indexes and FTS triggers are put back.
-- =========================================================================
CREATE TABLE user_activity_next (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           INTEGER NOT NULL,
    account_id   TEXT,
    source       TEXT NOT NULL,
    action       TEXT NOT NULL,
    target_kind  TEXT,
    target_id    TEXT,
    tier         TEXT NOT NULL DEFAULT 'standard',
    context_json TEXT,
    redacted     INTEGER NOT NULL DEFAULT 0,
    CHECK (source IN ('human', 'tui', 'cli', 'script', 'web', 'daemon', 'agent', 'mcp', 'assistant')),
    CHECK (tier   IN ('ephemeral', 'standard', 'important')),
    CHECK (redacted IN (0, 1))
);

INSERT INTO user_activity_next
    (id, ts, account_id, source, action, target_kind, target_id, tier, context_json, redacted)
SELECT id, ts, account_id, source, action, target_kind, target_id, tier, context_json, redacted
FROM user_activity;

-- Takes the old indexes and triggers with it. user_activity_fts keeps its
-- rows: they are keyed by id, and the ids are unchanged.
DROP TABLE user_activity;
ALTER TABLE user_activity_next RENAME TO user_activity;

CREATE INDEX idx_user_activity_ts ON user_activity (ts DESC);
CREATE INDEX idx_user_activity_action_ts ON user_activity (action, ts DESC);
CREATE INDEX idx_user_activity_target ON user_activity (target_kind, target_id);
CREATE INDEX idx_user_activity_account_ts ON user_activity (account_id, ts DESC);
CREATE INDEX idx_user_activity_tier_ts ON user_activity (tier, ts);

CREATE TRIGGER user_activity_ai AFTER INSERT ON user_activity
BEGIN
    INSERT INTO user_activity_fts (rowid, context_json)
    VALUES (new.id, new.context_json);
END;

CREATE TRIGGER user_activity_ad AFTER DELETE ON user_activity
BEGIN
    INSERT INTO user_activity_fts (user_activity_fts, rowid, context_json)
    VALUES ('delete', old.id, old.context_json);
END;

CREATE TRIGGER user_activity_au AFTER UPDATE ON user_activity
BEGIN
    INSERT INTO user_activity_fts (user_activity_fts, rowid, context_json)
    VALUES ('delete', old.id, old.context_json);
    INSERT INTO user_activity_fts (rowid, context_json)
    VALUES (new.id, new.context_json);
END;
