-- =========================================================================
-- To do: one row per thing you have to act on, with a date.
--
-- A to-do outlives its email: it can be made by hand, scheduled, ticked
-- off and corrected, so it has its own table rather than a flag on a
-- message. `deliveries` and `contact_commitments` set the pattern: a
-- detected row with provenance back to the message.
--
-- Three dates stay apart: `due_at` (the outside deadline), `act_by_at`
-- (due minus processing time) and `surface_at` (act-by minus the lead time
-- for its kind, at the start of working hours). `scheduled_for` is the
-- user's own date and wins over `surface_at`.
--
-- `relevant_until` ends the row's window. A detected row past it is
-- expired: at birth when the window had already closed when the row was
-- found (`expired_at_birth = 1`, never counted as "expired since you last
-- looked"), or by the sweep later. Rows the user made or touched never
-- expire (`origin` manual or handoff, `user_edited`, `scheduled_for`).
--
-- `surfaced_at` is the claim: the wake loop sets it in the same UPDATE
-- that finds a row whose surface time has come, so a to-do is announced
-- once across restarts. Re-runs never clear it, `expired_at` or a user
-- decision.
--
-- Deleting the source email: detected rows are deleted with it; a row the
-- user made or edited stays, with the text copied from the email dropped
-- (`delete_messages_and_derived`). The foreign keys clear the pointers.
-- =========================================================================
CREATE TABLE IF NOT EXISTS todos (
    id                    TEXT PRIMARY KEY,
    account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id             TEXT,
    source_message_id     TEXT REFERENCES messages(id) ON DELETE SET NULL,
    -- The source message's date: the catch-up rank and the age a window
    -- is measured from.
    source_date           INTEGER,
    -- bill | payment_failed | renewal | document | lease | return | rsvp
    -- | verify | sign | promise | other. Picks the lead time and window.
    kind                  TEXT NOT NULL,
    -- pay | fix | renew | verify | confirm | sign | rsvp | send | do
    verb                  TEXT NOT NULL,
    -- bill_link | bill_bank | passport | visa | licence | ...
    doc_type              TEXT,
    -- Verb plus object: "Pay council tax", never the subject line.
    title                 TEXT NOT NULL,
    counterparty          TEXT,
    -- Registrable domain of the sender, for confirmation matching.
    sender_domain         TEXT,
    amount_minor          INTEGER,
    currency              TEXT,
    due_at                INTEGER,
    -- The phrase the due date was read from, verbatim.
    due_words             TEXT,
    act_by_at             INTEGER,
    surface_at            INTEGER,
    scheduled_for         INTEGER,
    action_url            TEXT,
    -- The link the to-do is about and its registrable domain. Never opened
    -- from the row: the action opens the email with the link highlighted.
    action_domain         TEXT,
    relevant_until        INTEGER,
    -- schema | ics | rule | table | user
    window_source         TEXT,
    state                 TEXT NOT NULL DEFAULT 'open'
                          CHECK (state IN ('open', 'done', 'dismissed', 'expired')),
    expired_at            INTEGER,
    expired_at_birth      INTEGER NOT NULL DEFAULT 0,
    -- The first run's one-time batch of undated recent items: pending
    -- until kept or let go; overflow is past the cap of the batch.
    catchup               TEXT CHECK (catchup IN ('pending', 'kept', 'let_go', 'overflow')),
    -- A later message that looks like the confirmation. Only an offer:
    -- nothing closes a to-do on a guess.
    looks_done_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    looks_done_reason     TEXT,
    origin                TEXT NOT NULL
                          CHECK (origin IN ('rule', 'schema', 'ics', 'model', 'handoff', 'manual')),
    reason                TEXT NOT NULL,
    -- JSON: field -> {source, checked, evidence}.
    field_sources         TEXT NOT NULL DEFAULT '{}',
    user_edited           INTEGER NOT NULL DEFAULT 0,
    -- Promise rows: the contact_commitments row they mirror.
    commitment_id         TEXT,
    rules_version         INTEGER NOT NULL,
    -- account + kind + normalised object (+ due day): a reminder for the
    -- same bill updates the row instead of adding one.
    dedup_key             TEXT NOT NULL,
    surfaced_at           INTEGER,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    done_at               INTEGER,
    dismissed_at          INTEGER,
    UNIQUE (account_id, dedup_key)
);

-- The runway and the sweep read open rows by account.
CREATE INDEX IF NOT EXISTS idx_todos_account_state
    ON todos (account_id, state, surface_at);

-- The message delete finds a message's to-dos.
CREATE INDEX IF NOT EXISTS idx_todos_source_message
    ON todos (source_message_id);

CREATE INDEX IF NOT EXISTS idx_todos_looks_done_message
    ON todos (looks_done_message_id)
    WHERE looks_done_message_id IS NOT NULL;

-- Confirmation matching: open rows from one sender domain.
CREATE INDEX IF NOT EXISTS idx_todos_sender_domain
    ON todos (account_id, sender_domain)
    WHERE state = 'open';

-- The wake loop's claim scan: open, not yet announced.
CREATE INDEX IF NOT EXISTS idx_todos_unsurfaced
    ON todos (surface_at)
    WHERE state = 'open' AND surfaced_at IS NULL;

-- One row per account and rules version: where the newest-first first run
-- has got to, so it resumes after a restart. A new rules version starts a
-- new run, which never clears a claim, an expiry or a decision.
CREATE TABLE IF NOT EXISTS todo_runs (
    account_id        TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    rules_version     INTEGER NOT NULL,
    cursor_date       INTEGER,
    cursor_message_id TEXT,
    scanned           INTEGER NOT NULL DEFAULT 0,
    started_at        INTEGER NOT NULL,
    completed_at      INTEGER
);

-- When each mode was last opened, for "N expired since you last looked".
CREATE TABLE IF NOT EXISTS mode_views (
    mode           TEXT PRIMARY KEY,
    last_viewed_at INTEGER NOT NULL
);
