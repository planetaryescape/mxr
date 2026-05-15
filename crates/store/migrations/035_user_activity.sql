-- =========================================================================
-- User-activity log. Append-only, queryable, retention-bounded record of
-- *user-initiated* actions across TUI / CLI / web. The git-reflog for the
-- user's inbox. See docs/activity-log/ for the full design.
--
-- NOT to be confused with:
--   * `event_log` — system/diagnostic events (sync, errors, rule fires).
--   * `message_events` — per-message state transitions (read/archive/etc.)
--     keyed by message_id.
--
-- This table is strictly local. Never synced, never transmitted. Codified
-- in AGENTS.md and enforced at the recorder boundary.
-- =========================================================================
CREATE TABLE IF NOT EXISTS user_activity (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    ts           INTEGER NOT NULL,                     -- unix epoch ms (i64)
    account_id   TEXT,                                 -- nullable; app-level actions are accountless
    source       TEXT NOT NULL,                        -- 'tui' | 'cli' | 'web' | 'daemon'
    action       TEXT NOT NULL,                        -- canonical action token (see docs/activity-log/APPENDIX-action-catalog.md)
    target_kind  TEXT,                                 -- 'thread' | 'message' | 'draft' | 'search' | 'label' | ...
    target_id    TEXT,                                 -- foreign-ish id; not enforced (targets may be deleted)
    tier         TEXT NOT NULL DEFAULT 'standard',     -- 'ephemeral' | 'standard' | 'important'
    context_json TEXT,                                 -- JSON, schema per action; NULL when redacted
    redacted     INTEGER NOT NULL DEFAULT 0,           -- 0 | 1; tombstone marker
    CHECK (source IN ('tui', 'cli', 'web', 'daemon')),
    CHECK (tier   IN ('ephemeral', 'standard', 'important')),
    CHECK (redacted IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_user_activity_ts
    ON user_activity (ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_activity_action_ts
    ON user_activity (action, ts DESC);

CREATE INDEX IF NOT EXISTS idx_user_activity_target
    ON user_activity (target_kind, target_id);

CREATE INDEX IF NOT EXISTS idx_user_activity_account_ts
    ON user_activity (account_id, ts DESC);

-- ASC on (tier, ts) is intentional: the retention prune scans the leading
-- range of old rows per tier (ts < cutoff) so ascending order matches the
-- access pattern.
CREATE INDEX IF NOT EXISTS idx_user_activity_tier_ts
    ON user_activity (tier, ts);
