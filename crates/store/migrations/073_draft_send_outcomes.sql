-- No draft FK: deleting a draft must not remove send evidence or replay protection.
CREATE TABLE IF NOT EXISTS draft_send_outcomes (
 operation_id TEXT PRIMARY KEY,
 draft_id TEXT NOT NULL,
 account_id TEXT NOT NULL,
 reviewed_revision INTEGER NOT NULL,
 command_hash TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('unknown','accepted','not_accepted')),
 local_message_id TEXT,
 provider_message_id TEXT,
 rfc2822_message_id TEXT,
 updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS draft_send_outcomes_draft ON draft_send_outcomes(draft_id);

-- Preserve historical acceptance even when a legacy draft row has gone.
INSERT OR IGNORE INTO draft_send_outcomes
(operation_id,draft_id,account_id,reviewed_revision,command_hash,state,local_message_id,provider_message_id,rfc2822_message_id,updated_at)
SELECT 'legacy:' || draft_id,draft_id,account_id,0,'legacy','accepted',local_message_id,provider_message_id,rfc2822_message_id,sent_at
FROM sent_draft_receipts;

-- Legacy in-flight submissions have no evidence that the provider rejected them.
INSERT OR IGNORE INTO draft_send_outcomes
(operation_id,draft_id,account_id,reviewed_revision,command_hash,state,updated_at)
SELECT 'legacy-sending:' || id,id,account_id,revision,'legacy','unknown',COALESCE(status_updated_at, updated_at)
FROM drafts WHERE status = 'sending';
