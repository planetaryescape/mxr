-- Durable record of each scheduled-send firing, so a lost send (daemon
-- crashed in the window between clearing `send_at` and the provider
-- accepting the message) can be surfaced on the next startup instead of
-- vanishing silently.
--
-- Scheduled sends stay AT-MOST-ONCE: `send_at` is cleared before the
-- provider call so a crash can't re-fire forever. The cost of that is a
-- possible silent loss; this table makes the loss visible. An attempt row
-- is written in the SAME transaction that clears `send_at`; `outcome` is
-- NULL until the send resolves (sent / blocked / failed). A NULL outcome
-- found at startup means the daemon died mid-send — a candidate lost send.
CREATE TABLE IF NOT EXISTS scheduled_send_attempts (
    draft_id     TEXT NOT NULL,
    attempted_at INTEGER NOT NULL,
    outcome      TEXT,
    PRIMARY KEY (draft_id, attempted_at)
);

CREATE INDEX IF NOT EXISTS idx_scheduled_send_attempts_unresolved
    ON scheduled_send_attempts (attempted_at)
    WHERE outcome IS NULL;
