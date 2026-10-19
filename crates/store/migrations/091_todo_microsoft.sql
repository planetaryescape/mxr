-- A handoff is claimed before launching the external CLI. An interrupted
-- claim stays uncertain: never create a duplicate after its idempotency TTL.
CREATE TABLE IF NOT EXISTS todo_microsoft_exports (
    todo_id TEXT PRIMARY KEY REFERENCES todos(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('sending', 'accepted', 'unknown')),
    task_id TEXT,
    operation_id TEXT,
    payload_json TEXT NOT NULL,
    baseline_json TEXT,
    last_attempt_at INTEGER,
    last_synced_at INTEGER,
    last_error TEXT,
    uncertain_json TEXT,
    started_at INTEGER NOT NULL
);
