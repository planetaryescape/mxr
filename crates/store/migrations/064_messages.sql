-- Messages (blueprint 22, phase 3): one person can write from several
-- addresses. A link joins an address to the person it belongs to, named by
-- that person's primary address. Links are only ever made by the user:
-- mxr suggests a merge (same name, you've written to both) and never
-- merges on its own (D117). Unlinked addresses are each their own person.
CREATE TABLE IF NOT EXISTS person_links (
    account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email         TEXT NOT NULL COLLATE NOCASE,
    person_email  TEXT NOT NULL COLLATE NOCASE,
    linked_at     INTEGER NOT NULL,
    PRIMARY KEY (account_id, email),
    CHECK (email <> person_email)
);

CREATE INDEX IF NOT EXISTS idx_person_links_person
    ON person_links(account_id, person_email);

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
