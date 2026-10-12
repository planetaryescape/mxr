CREATE TABLE IF NOT EXISTS draft_attachment_resources (
 id TEXT PRIMARY KEY,
 draft_id TEXT NOT NULL,
 account_id TEXT NOT NULL,
 metadata_json TEXT NOT NULL,
 ready INTEGER NOT NULL DEFAULT 0 CHECK (ready IN (0, 1)),
 updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS draft_attachment_resources_owner ON draft_attachment_resources(draft_id);
