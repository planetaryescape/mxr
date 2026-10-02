-- The message delete removes event log rows by message id inside its write
-- transaction. Without this index every delete batch scans the whole event
-- log while holding the writer.
CREATE INDEX IF NOT EXISTS idx_event_log_message
    ON event_log(message_id)
    WHERE message_id IS NOT NULL;
