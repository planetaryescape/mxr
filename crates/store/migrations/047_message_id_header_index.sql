-- Composite index for the per-message dedup / existence lookup
-- (`list_envelopes_by_message_id_header`): `WHERE account_id = ? AND message_id_header = ?`.
--
-- Without it, a single-account mailbox falls back to `idx_messages_account`
-- (account_id only) and scans every row of that account per lookup. On a large
-- mailbox (~108k rows) this pins a CPU core continuously and lets the WAL grow
-- unbounded as the loop dribbles writes (issue #107). The composite index turns
-- the lookup into an index seek (~200x faster).
CREATE INDEX IF NOT EXISTS idx_messages_account_msgidhdr
    ON messages(account_id, message_id_header);
