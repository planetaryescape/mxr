-- =========================================================================
-- Timed reply later: "reply to this tue 9".
--
-- `reply_later_due_at` is the UTC instant the user chose (the previewed
-- one, never the words). While it is in the future the message is out of
-- the reply queue and its conversation is off the desk; from then on it is
-- back in both. NULL is the untimed flag, as before.
--
-- `reply_later_returned_at` records that the daemon announced the return.
-- The wake loop claims a row by setting it in the same UPDATE that finds
-- it, so the announcement happens once, even across restarts. Visibility
-- never depends on it: a conversation is back as soon as its time passes.
--
-- Applied in code via `MigrationKind::Composite` (see `pool.rs`); this file
-- is the canonical record of the schema change.
-- =========================================================================
ALTER TABLE message_flags ADD COLUMN reply_later_due_at INTEGER;
ALTER TABLE message_flags ADD COLUMN reply_later_returned_at INTEGER;

-- The wake loop's scan: flagged, timed, not yet announced.
CREATE INDEX IF NOT EXISTS idx_message_flags_reply_later_due
    ON message_flags (reply_later_due_at)
    WHERE reply_later = 1
      AND reply_later_due_at IS NOT NULL
      AND reply_later_returned_at IS NULL;
