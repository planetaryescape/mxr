-- Slice 1.3: persistent audit + override tokens for the pre-send
-- safety pipeline. Per docs/reference/ai-email.md:
-- - draft_safety_runs:    one row per check, redacted previews only
-- - draft_safety_overrides: single-use tokens for blocker bypass
--
-- PII previews are stored only inside `issues_json`'s `detail` field,
-- which the safety crate redacts before persistence.

CREATE TABLE IF NOT EXISTS draft_safety_runs (
    id TEXT PRIMARY KEY,
    draft_id TEXT,
    account_id TEXT NOT NULL,
    verdict TEXT NOT NULL,
    issues_json TEXT NOT NULL,
    checked_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_draft_safety_runs_draft
    ON draft_safety_runs(draft_id, checked_at DESC);

CREATE TABLE IF NOT EXISTS draft_safety_overrides (
    token TEXT PRIMARY KEY,
    draft_id TEXT,
    issue_kinds_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    used_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_draft_safety_overrides_draft
    ON draft_safety_overrides(draft_id);
