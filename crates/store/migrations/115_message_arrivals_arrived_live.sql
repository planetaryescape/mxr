-- arrived_live: a sync stored the message for the first time after the
-- account's initial backfill. Background model work runs only on such mail;
-- backfilled mail reaches the model only when the user asks
-- (docs/issues/backfill-mail-triggers-background-inference.md).
-- It is broader than is_live, which alert to-dos use and which also requires
-- unread inbound Inbox mail, so every is_live row arrived live.
UPDATE message_arrivals SET arrived_live = 1 WHERE is_live = 1 AND arrived_live = 0;

CREATE INDEX IF NOT EXISTS idx_message_arrivals_arrived_live
    ON message_arrivals(account_id, message_id) WHERE arrived_live = 1;
