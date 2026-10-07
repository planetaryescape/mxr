-- =========================================================================
-- Archive: records built from mail (receipts, orders, bookings, invoices,
-- statements, tickets, contracts, warranties, accounts).
--
-- A record is one thing, not one email: an order's confirmation, dispatch
-- and delivery are one row; a booking and its changes are one row. Trips
-- and series group records in `record_groups`.
--
-- Values live in `record_fields`, one row per field per source: each email
-- that says something about the record adds its own candidate, and the user
-- adds `source = 'user'` rows. The `records` columns are the winning value
-- of each field (user, then schema.org, then the to-do or delivery it came
-- from, then a rule; newest first within a rank), recomputed whenever a
-- field row changes. Deleting an email cascades its field rows away, and
-- the delete path recomputes the columns from what is left, so a record's
-- fields always come from mail that still exists or from the user.
--
-- A record is deleted when no source email is left (the delete path, like
-- deliveries). `dismissed_at` is "not a record": the row and its correction
-- stay so a re-run never files it again.
--
-- Upserts use `ON CONFLICT(account_id, dedup_key)`, never INSERT OR
-- REPLACE, which would cascade away the field and message rows.
-- =========================================================================
CREATE TABLE IF NOT EXISTS record_groups (
    id          TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- trip | series
    kind        TEXT NOT NULL CHECK (kind IN ('trip', 'series')),
    -- trip|<first record id> or series|<issuer key>|<account ref>
    group_key   TEXT NOT NULL,
    title       TEXT NOT NULL,
    span_start  INTEGER,
    span_end    INTEGER,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (account_id, group_key)
);

CREATE TABLE IF NOT EXISTS records (
    id             TEXT PRIMARY KEY,
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- kind + issuer + reference, or the source message when there is no
    -- reference. A later email about the same order lands on the same row.
    dedup_key      TEXT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN (
                       'receipt', 'order', 'booking', 'invoice', 'statement',
                       'ticket', 'contract', 'warranty', 'account')),
    -- Winning field values, recomputed from record_fields.
    issuer         TEXT,
    -- Lowercased issuer for the issuer page and series grouping.
    issuer_key     TEXT,
    title          TEXT,
    reference      TEXT,
    amount_minor   INTEGER,
    currency       TEXT,
    -- The transaction's date, not the email's.
    issued_at      INTEGER,
    span_start     INTEGER,
    span_end       INTEGER,
    -- Where a booking takes you, for trip grouping and the answer box.
    place          TEXT,
    delivered_at   INTEGER,
    return_by      INTEGER,
    warranty_until INTEGER,
    valid_until    INTEGER,
    -- 1 when every money and date field came from schema.org or the user.
    checked        INTEGER NOT NULL DEFAULT 0,
    group_id       TEXT REFERENCES record_groups(id) ON DELETE SET NULL,
    -- How it was filed: schema | rule | delivery | todo | manual | sender
    origin         TEXT NOT NULL CHECK (origin IN (
                       'schema', 'rule', 'delivery', 'todo', 'manual', 'sender')),
    -- The why line's evidence: "order confirmation with schema.org markup".
    reason         TEXT NOT NULL,
    -- The newest source email's thread, for "open the email".
    thread_id      TEXT,
    -- The newest source email's date: the ledger's fallback date.
    last_message_at INTEGER,
    rules_version  INTEGER NOT NULL,
    dismissed_at   INTEGER,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    UNIQUE (account_id, dedup_key)
);

-- The ledger: an account's records by transaction date, newest first.
CREATE INDEX IF NOT EXISTS idx_records_ledger
    ON records (account_id, issued_at DESC)
    WHERE dismissed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_records_issuer
    ON records (account_id, issuer_key);

CREATE INDEX IF NOT EXISTS idx_records_reference
    ON records (account_id, reference)
    WHERE reference IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_records_group
    ON records (group_id)
    WHERE group_id IS NOT NULL;

-- Which emails a record was built from and what each one said.
CREATE TABLE IF NOT EXISTS record_messages (
    record_id   TEXT NOT NULL REFERENCES records(id) ON DELETE CASCADE,
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    thread_id   TEXT,
    -- confirmation | shipped | delivered | return | refund | invoice |
    -- statement | booking | change | cancellation | receipt | other
    stage       TEXT NOT NULL,
    message_at  INTEGER NOT NULL,
    -- detector | delivery | todo | manual | sender
    filed_by    TEXT NOT NULL,
    filed_at    INTEGER NOT NULL,
    PRIMARY KEY (record_id, message_id)
);

CREATE INDEX IF NOT EXISTS idx_record_messages_message
    ON record_messages (message_id);

-- One candidate value per field per source. `source_key` is the message id
-- for a value read from an email, `user` for a correction, `todo:<id>` for
-- what a ticked-off to-do carried.
CREATE TABLE IF NOT EXISTS record_fields (
    record_id   TEXT NOT NULL REFERENCES records(id) ON DELETE CASCADE,
    -- kind | issuer | title | reference | amount | issued_at | span_start |
    -- span_end | place | delivered_at | return_by | warranty_until |
    -- valid_until
    field       TEXT NOT NULL,
    source_key  TEXT NOT NULL,
    message_id  TEXT REFERENCES messages(id) ON DELETE CASCADE,
    -- schema | rule | delivery | todo | user
    source      TEXT NOT NULL CHECK (source IN ('schema', 'rule', 'delivery', 'todo', 'user')),
    -- Higher wins: user 4, schema 3, delivery and todo 2, rule 1.
    rank        INTEGER NOT NULL,
    value_text  TEXT,
    -- Minor units for amount (currency in value_text), unix seconds for
    -- dates.
    value_int   INTEGER,
    checked     INTEGER NOT NULL,
    -- The words the value was read from, verbatim.
    evidence    TEXT,
    observed_at INTEGER NOT NULL,
    PRIMARY KEY (record_id, field, source_key)
);

CREATE INDEX IF NOT EXISTS idx_record_fields_message
    ON record_fields (message_id)
    WHERE message_id IS NOT NULL;

-- Per-sender corrections: always file this sender's mail, never file it,
-- or call the issuer something else. Made by the user, so they outlive
-- the mail.
CREATE TABLE IF NOT EXISTS record_senders (
    account_id   TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    sender_email TEXT NOT NULL,
    -- always | never
    verdict      TEXT CHECK (verdict IN ('always', 'never')),
    -- The kind "always" files as.
    kind         TEXT,
    issuer_name  TEXT,
    decided_at   INTEGER NOT NULL,
    PRIMARY KEY (account_id, sender_email)
);

-- Where an account's first pass over history has got to, newest first.
CREATE TABLE IF NOT EXISTS record_runs (
    account_id        TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    rules_version     INTEGER NOT NULL,
    cursor_date       INTEGER,
    cursor_message_id TEXT,
    scanned           INTEGER NOT NULL DEFAULT 0,
    started_at        INTEGER NOT NULL,
    completed_at      INTEGER
);
