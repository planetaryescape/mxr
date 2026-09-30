-- Covering index for `list_owed_replies`: each thread's latest inbound
-- message, a later outbound reply and the sender are all read from it.
CREATE INDEX IF NOT EXISTS idx_messages_owed
    ON messages(account_id, direction, thread_id, date, from_email);
