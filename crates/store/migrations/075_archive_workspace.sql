CREATE TABLE IF NOT EXISTS archive_workspace (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    workspace_id TEXT NOT NULL,
    recovery_generation TEXT NOT NULL
);
