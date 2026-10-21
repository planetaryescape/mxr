-- Per-message evidence reads look up the row's own thread by account and
-- thread, newest first. Without this index the planner searched by account
-- alone and walked the whole mailbox for every row, which held reader
-- connections for minutes and starved the pool.
CREATE INDEX IF NOT EXISTS idx_messages_account_thread_date
    ON messages(account_id, thread_id, date DESC, id DESC);
