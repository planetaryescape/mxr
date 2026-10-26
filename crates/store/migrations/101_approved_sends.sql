-- Keep the old outcome as unknown until acceptance or definite refusal. Older
-- binaries therefore fail closed instead of treating a waiting send as new.
CREATE TABLE IF NOT EXISTS approved_sends (
    operation_id TEXT PRIMARY KEY REFERENCES draft_send_outcomes(operation_id),
    approval_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('submitting','waiting','unknown','accepted','failed')),
    phase TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 1,
    retry_at INTEGER,
    accepted_at INTEGER,
    reason TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS approved_sends_due ON approved_sends(status, retry_at);
CREATE TABLE IF NOT EXISTS approved_send_attempts (
    operation_id TEXT NOT NULL REFERENCES approved_sends(operation_id),
    attempt INTEGER NOT NULL,
    phase TEXT NOT NULL,
    outcome TEXT NOT NULL,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY(operation_id, attempt)
);

-- A successful canonical edit revokes only approvals with positive proof of
-- non-submission. Never release a submitting/unknown attempt this way.
CREATE TRIGGER IF NOT EXISTS approved_send_draft_edited AFTER UPDATE OF revision ON drafts
WHEN NEW.revision != OLD.revision
BEGIN
    UPDATE draft_send_outcomes SET state='not_accepted',updated_at=unixepoch()
    WHERE draft_id=NEW.id AND state='unknown'
      AND operation_id IN (SELECT operation_id FROM approved_sends WHERE status='waiting');
    UPDATE approved_sends SET status='failed',reason='approval_invalidated',retry_at=NULL,updated_at=unixepoch()
    WHERE status='waiting' AND operation_id IN
      (SELECT operation_id FROM draft_send_outcomes WHERE draft_id=NEW.id AND state='not_accepted');
    UPDATE scheduled_send_attempts SET outcome='failed' WHERE draft_id=NEW.id AND outcome='waiting';
END;
