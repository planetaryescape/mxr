ALTER TABLE contact_style ADD COLUMN drift_detected INTEGER NOT NULL DEFAULT 0;
ALTER TABLE contact_style ADD COLUMN drift_reason TEXT;
ALTER TABLE contact_style ADD COLUMN drift_detected_at INTEGER;
