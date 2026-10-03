-- =========================================================================
-- Per-mode done (blueprint 22, "Handoff names where the item went").
--
-- Each mode keeps its own done state, so done in Messages never clears To
-- do and a new message brings the thread back to that mode only. Same
-- watermark as desk_dismissals: the highest message rowid and the message
-- count when it was done, by storage order, not Date headers. To do keeps
-- its done state on the todos rows themselves, so it has no row here.
-- =========================================================================
CREATE TABLE IF NOT EXISTS mode_done (
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id      TEXT NOT NULL,
    mode           TEXT NOT NULL CHECK (mode IN ('messages', 'updates', 'reading')),
    through_rowid  INTEGER NOT NULL,
    through_count  INTEGER NOT NULL,
    done_at        INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id, mode)
);

CREATE INDEX IF NOT EXISTS idx_mode_done_mode ON mode_done(account_id, mode);
