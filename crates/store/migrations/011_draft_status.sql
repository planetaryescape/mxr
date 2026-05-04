-- =========================================================================
-- Draft status state machine + idempotency anchor.
--
-- `status` records where the draft is in the send pipeline:
--   * 'draft'   — composable, default state.
--   * 'sending' — CAS'd by send_stored_draft just before invoking the
--                 provider. Refuses concurrent sends; on daemon crash, stays
--                 in this state and requires manual `mxr drafts resolve`.
--   * 'sent'    — terminal. Retry of `mxr send <id>` returns "already sent"
--                 instead of resending.
--
-- `message_id_header` is the RFC 5322 Message-ID assigned at first send. It
-- is persisted so that a recovered/retried send keeps the same header,
-- letting the next IMAP sync dedupe by Message-ID against the synthetic
-- envelope inserted on send-success.
-- =========================================================================
ALTER TABLE drafts ADD COLUMN status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'sending', 'sent'));
ALTER TABLE drafts ADD COLUMN status_updated_at INTEGER;
ALTER TABLE drafts ADD COLUMN message_id_header TEXT;

CREATE INDEX IF NOT EXISTS idx_drafts_status
    ON drafts(account_id, status);
