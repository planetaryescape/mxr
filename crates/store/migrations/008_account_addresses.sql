-- =========================================================================
-- Per-account email addresses (aliases). One account can own many addresses;
-- exactly one is `is_primary = 1`.
--
-- Direction inference (Slice 8) compares `messages.from_email` against this
-- table to decide inbound vs outbound — the reason aliases need to be
-- first-class. `MessageFlags::SENT` is unreliable across providers.
--
-- Backfill: every existing account gets its `accounts.email` seeded as the
-- primary, idempotent via INSERT OR IGNORE. Accounts inserted after the
-- migration get the same seed via `Store::insert_account`.
-- =========================================================================
CREATE TABLE IF NOT EXISTS account_addresses (
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    is_primary  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, email)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_account_addresses_one_primary
    ON account_addresses(account_id) WHERE is_primary = 1;

CREATE INDEX IF NOT EXISTS idx_account_addresses_email
    ON account_addresses(email);

INSERT OR IGNORE INTO account_addresses (account_id, email, is_primary)
SELECT id, email, 1 FROM accounts WHERE email != '';
