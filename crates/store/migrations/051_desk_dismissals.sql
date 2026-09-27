-- =========================================================================
-- Desk dismissals: "done waiting" on a conversation you wrote last.
--
-- Archiving cannot take a thread you started off the desk's Waiting lane
-- (a sent-only thread has nothing in the inbox to archive), so the desk
-- keeps its own marker. It records how far the thread had arrived when it
-- was dismissed, by storage order rather than Date headers: the highest
-- message rowid and the message count. Any message stored afterwards, even
-- one with an older or bogus Date, brings the thread back.
-- =========================================================================
CREATE TABLE IF NOT EXISTS desk_dismissals (
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id      TEXT NOT NULL,
    through_rowid  INTEGER NOT NULL,
    through_count  INTEGER NOT NULL,
    dismissed_at   INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);
