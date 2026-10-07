-- Messages' Got it (blueprint 22, phase 3): one acknowledgement per message,
-- ever. A row is written before the reply is handed to the provider and
-- kept whatever happens after, so a timeout or a failed local ingest can
-- never lead to a second send. `sent_message_id` is set once the send is
-- confirmed. Deleting the acknowledged message deletes its row.
CREATE TABLE IF NOT EXISTS got_it_sends (
    target_message_id  TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id         TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id          TEXT NOT NULL,
    draft_id           TEXT NOT NULL,
    sent_message_id    TEXT,
    claimed_at         INTEGER NOT NULL
);
