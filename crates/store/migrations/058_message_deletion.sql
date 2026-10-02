-- Support for deleting a message together with everything derived from it.

-- The message delete removes event log rows by message id inside its write
-- transaction. Without this index every delete batch scans the whole event
-- log while holding the writer.
CREATE INDEX IF NOT EXISTS idx_event_log_message
    ON event_log(message_id)
    WHERE message_id IS NOT NULL;

-- Deleted messages whose files and in-memory index entries still have to be
-- cleared. Written in the delete's own transaction and drained by the
-- daemon after it clears them, so a failed sync pass or a crash between the
-- two leaves the work recorded instead of lost. No foreign key: the
-- messages are gone by the time a row is read.
-- A cleanup that fails (a permission error on a file) stays recorded and is
-- retried after `retry_after`, with a backoff that grows per attempt, and
-- is left alone once `attempts` reaches the cap so it cannot spin forever.
CREATE TABLE IF NOT EXISTS pending_message_forgets (
    message_id  TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL,
    deleted_at  INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    retry_after INTEGER NOT NULL DEFAULT 0
);

-- Which messages a decision cites, as rows the delete can find through an
-- index instead of parsing every decision's evidence JSON on the writer.
-- Both sides cascade: a deleted message drops its rows, and a decision
-- left with none is then deleted by the message delete.
CREATE TABLE IF NOT EXISTS decision_evidence (
    decision_id TEXT NOT NULL REFERENCES decision_log(id) ON DELETE CASCADE,
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    PRIMARY KEY (decision_id, message_id)
);

CREATE INDEX IF NOT EXISTS idx_decision_evidence_message
    ON decision_evidence(message_id);

INSERT OR IGNORE INTO decision_evidence (decision_id, message_id)
SELECT decision_log.id, evidence.value
FROM decision_log, json_each(decision_log.evidence_msg_ids) AS evidence
WHERE evidence.value IN (SELECT id FROM messages);
