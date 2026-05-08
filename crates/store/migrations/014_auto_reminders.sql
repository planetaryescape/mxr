-- =========================================================================
-- Auto-reminders: "nudge me if no reply in N days."
--
-- Set when a user sends a reply with a reminder window. The daemon's
-- auto-reminders loop scans this table on a 60-second cadence and:
--
--   * If `now >= remind_at` and there's no reply tracked for the sent
--     message yet, marks `triggered_at` and emits a ReminderTriggered
--     event so the UI can surface it as a follow-up.
--
--   * If a reply has been ingested in the meantime, marks `cancelled_at`
--     so the reminder doesn't fire — the user already got their answer.
--
-- A row exists per outbound message that has a reminder configured.
-- Cancellation is non-destructive (we keep the row) so analytics can
-- distinguish "user got reply naturally" from "user dismissed reminder."
-- =========================================================================
CREATE TABLE IF NOT EXISTS auto_reminders (
    sent_message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    remind_at INTEGER NOT NULL,
    set_at INTEGER NOT NULL,
    triggered_at INTEGER,
    cancelled_at INTEGER
);

-- Hot-path index: the loop only ever queries the active set (neither
-- triggered nor cancelled), ordered by remind_at ascending.
CREATE INDEX IF NOT EXISTS idx_auto_reminders_pending
    ON auto_reminders (remind_at)
    WHERE triggered_at IS NULL AND cancelled_at IS NULL;
