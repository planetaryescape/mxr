CREATE TABLE IF NOT EXISTS restore_recovery (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    pending INTEGER NOT NULL,
    source_owner_stopped INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS restored_draft_holds (
    draft_id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    subject TEXT NOT NULL,
    prepared_draft_id TEXT
);
CREATE TABLE IF NOT EXISTS restored_intentions (
    original_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    account_id TEXT,
    original_state TEXT NOT NULL,
    outcome_reference TEXT,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (kind, original_id)
);
