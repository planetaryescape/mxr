-- =========================================================================
-- Done watermarks by (date, message id), not rowid.
--
-- desk_dismissals and mode_done recorded how far a thread had arrived by
-- the highest message rowid. SQLite reuses the highest rowid after that
-- row is deleted, so a message stored after a delete could take the old
-- number and stay hidden behind a done mark. The watermark is now the
-- newest message's date and id (message ids are stable, derived from the
-- provider's id), still with the message count, so a new message, even
-- one with an older Date, brings the thread back unless one was deleted
-- in between. through_rowid stays for older binaries and is no longer
-- read.
--
-- The columns are added by the migration's AddColumn steps in pool.rs, so
-- re-running it on a database that has them is safe. Existing marks are
-- backfilled here from the newest message the old watermark covered; only
-- rows not yet backfilled (through_date 0, a real rowid), since new marks
-- write through_rowid 0. A mark whose messages are all gone keeps the
-- epoch and the nil id, so it covers nothing.
-- =========================================================================
UPDATE desk_dismissals SET
    through_date = COALESCE((
        SELECT m.date FROM messages m
        WHERE m.account_id = desk_dismissals.account_id
          AND m.thread_id = desk_dismissals.thread_id
          AND m.rowid <= desk_dismissals.through_rowid
        ORDER BY m.date DESC, m.id DESC LIMIT 1), 0),
    through_message_id = COALESCE((
        SELECT m.id FROM messages m
        WHERE m.account_id = desk_dismissals.account_id
          AND m.thread_id = desk_dismissals.thread_id
          AND m.rowid <= desk_dismissals.through_rowid
        ORDER BY m.date DESC, m.id DESC LIMIT 1),
        '00000000-0000-0000-0000-000000000000')
WHERE through_date = 0 AND through_rowid > 0;

UPDATE mode_done SET
    through_date = COALESCE((
        SELECT m.date FROM messages m
        WHERE m.account_id = mode_done.account_id
          AND m.thread_id = mode_done.thread_id
          AND m.rowid <= mode_done.through_rowid
        ORDER BY m.date DESC, m.id DESC LIMIT 1), 0),
    through_message_id = COALESCE((
        SELECT m.id FROM messages m
        WHERE m.account_id = mode_done.account_id
          AND m.thread_id = mode_done.thread_id
          AND m.rowid <= mode_done.through_rowid
        ORDER BY m.date DESC, m.id DESC LIMIT 1),
        '00000000-0000-0000-0000-000000000000')
WHERE through_date = 0 AND through_rowid > 0;
