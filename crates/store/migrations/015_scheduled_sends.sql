-- =========================================================================
-- Send-later: schedule a draft to be sent at a future time.
--
-- Drafts already have a status state machine ('draft' -> 'sending' ->
-- 'sent') from migration 011. Rather than introducing a fourth status
-- and reworking the CHECK constraint, "scheduled" is the orthogonal
-- combination of `status = 'draft' AND send_at IS NOT NULL`.
--
-- The flusher loop scans `pending_scheduled` (the partial index below)
-- and CAS-promotes due drafts from `draft` -> `sending` via the same
-- existing send pipeline. Cancellation is just `UPDATE drafts SET
-- send_at = NULL` while the row is still in `draft` status.
-- =========================================================================
ALTER TABLE drafts ADD COLUMN send_at INTEGER;

-- Hot-path index: only rows currently scheduled and not-yet-sending.
-- Once the flusher CAS-promotes to 'sending', the row drops out of
-- this index and the existing send pipeline takes over.
CREATE INDEX IF NOT EXISTS idx_drafts_pending_scheduled
    ON drafts(send_at)
    WHERE send_at IS NOT NULL AND status = 'draft';
