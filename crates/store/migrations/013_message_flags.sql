-- =========================================================================
-- Per-message user-intent flags. Distinct from MessageFlags (which mirror
-- provider-side flags like SEEN, FLAGGED, ANSWERED) — these are local user
-- intents that don't roundtrip to the provider.
--
-- `reply_later`: the user marked the message for follow-up. Cleared
-- automatically when a reply is sent (any path) or explicitly dismissed.
--
-- A row exists only when at least one local flag is set; absence equals
-- "no local flags." `set_at` and `dismissed_at` are bookkeeping for the
-- queue UI ordering and analytics.
-- =========================================================================
CREATE TABLE IF NOT EXISTS message_flags (
    message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    reply_later INTEGER NOT NULL DEFAULT 0,
    reply_later_set_at INTEGER,
    reply_later_dismissed_at INTEGER
);

-- Partial index for the reply-later queue view: only the flagged set is
-- ever queried, and we sort by set_at desc for "most recent first".
CREATE INDEX IF NOT EXISTS idx_message_flags_reply_later
    ON message_flags (reply_later_set_at DESC)
    WHERE reply_later = 1;
