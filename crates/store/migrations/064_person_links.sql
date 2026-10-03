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
