-- Receipts for unsubscribes mxr sent, one per message. Kept apart from
-- event_log so purging history never lets a second unsubscribe go out. No
-- subject, sender or URL: a receipt says only that an unsubscribe happened,
-- and how.
--
-- An attempt claims its message before it sends (method 'pending'), so two
-- requests cannot both send. A confirmed send is recorded with its method. Any
-- failed attempt releases its claim, so a retry may send again; a pending row
-- older than ten minutes is abandoned and taken over. Once confirmed, a
-- message is never sent again. Rows go with their message.
CREATE TABLE IF NOT EXISTS unsubscribe_receipts (
    message_id  TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id  TEXT NOT NULL,
    method      TEXT NOT NULL,
    sent_at     INTEGER NOT NULL
);

-- Backfill what the old event_log guard already treated as sent. Matches the
-- category, the summary substring and LIKE's case folding that
-- `has_event_for_message_with_summary` used. Only messages that still exist
-- qualify; the earliest row per message wins.
INSERT OR IGNORE INTO unsubscribe_receipts (message_id, account_id, method, sent_at)
SELECT message_id, account_id, 'unknown', timestamp
FROM (
    SELECT e.message_id,
           COALESCE(e.account_id, m.account_id) AS account_id,
           e.timestamp,
           ROW_NUMBER() OVER (
               PARTITION BY e.message_id
               ORDER BY e.timestamp ASC, e.id ASC
           ) AS row_rank
    FROM event_log e
    INNER JOIN messages m ON m.id = e.message_id
    WHERE e.category = 'mutation'
      AND (e.summary LIKE 'Sent unsubscribe request for %'
           OR e.summary LIKE 'Unsubscribed from %')
)
WHERE row_rank = 1;
