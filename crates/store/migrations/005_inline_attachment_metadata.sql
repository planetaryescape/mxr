ALTER TABLE attachments
ADD COLUMN disposition TEXT NOT NULL DEFAULT 'unspecified';

ALTER TABLE attachments
ADD COLUMN content_id TEXT;

ALTER TABLE attachments
ADD COLUMN content_location TEXT;

CREATE INDEX IF NOT EXISTS idx_attachments_content_id
ON attachments(content_id);
