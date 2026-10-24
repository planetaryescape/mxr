-- Receiver and global classifier authority survive process restarts.
CREATE TABLE IF NOT EXISTS assessment_authority (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
