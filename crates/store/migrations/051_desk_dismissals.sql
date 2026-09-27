-- =========================================================================
-- Desk dismissals: "done waiting" on a conversation you wrote last.
--
-- Archiving cannot take a thread you started off the desk's Waiting lane
-- (a sent-only thread has nothing in the inbox to archive), so the desk
-- keeps its own marker. `through_date` is the date of the thread's newest
-- message when it was dismissed: any later message brings the thread back.
-- =========================================================================
CREATE TABLE IF NOT EXISTS desk_dismissals (
    account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id     TEXT NOT NULL,
    through_date  INTEGER NOT NULL,
    dismissed_at  INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);
