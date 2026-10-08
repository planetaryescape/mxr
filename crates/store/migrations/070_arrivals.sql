-- Where each email went when it arrived, and what the user corrected
-- (D119). Membership stays computed (D097); an arrivals row records a past
-- fact, so mail that later leaves the inbox still counts where it went.

-- One row per inbound message, written by a trigger the moment the message
-- is stored, so every insert path records it. The daemon fills `mode`,
-- `rule` and `reason` after sync; until then the row is "still sorting".
CREATE TABLE IF NOT EXISTS arrivals (
    message_id    TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Lowercased, so a sender's rows are found without a scan.
    sender_email  TEXT NOT NULL,
    -- When mxr first stored it. Never the Date header of mail that arrives
    -- after the account's first sync, which the sender sets and may back-
    -- date. Only an account's first sync (no sync cursor yet) is backfill:
    -- there, history older than two days keeps its date so it never reads
    -- as "arrived just now".
    first_seen_at INTEGER NOT NULL,
    -- messages | updates | reading | screened_out | spam; NULL while sorting.
    mode          TEXT,
    -- The rule that placed it, as `KindRuleData` names it, or `copied`,
    -- `spam`.
    rule          TEXT,
    -- The reason shown beside the mode: "has List-Unsubscribe".
    reason        TEXT,
    -- A rule conflict to ask about on Now ("copied_known"), else NULL.
    not_sure      TEXT,
    placed_at     INTEGER,
    -- Where it is now, re-placed after every correction; NULL means where
    -- it arrived.
    now_mode      TEXT,
    -- The user's move of this one email (X): where to, and when. A sender
    -- move made later wins over it.
    moved_to      TEXT,
    moved_at      INTEGER,
    -- The sender move (a `mode_corrections` id) that overrode this email's
    -- move; undoing that sender move clears it and the email move stands
    -- again.
    superseded_by INTEGER
);

CREATE INDEX IF NOT EXISTS idx_arrivals_account_seen
    ON arrivals(account_id, first_seen_at);
CREATE INDEX IF NOT EXISTS idx_arrivals_sender
    ON arrivals(account_id, sender_email);
CREATE INDEX IF NOT EXISTS idx_arrivals_sorting
    ON arrivals(account_id) WHERE mode IS NULL;
CREATE INDEX IF NOT EXISTS idx_arrivals_moved
    ON arrivals(account_id) WHERE moved_at IS NOT NULL;

-- After an account's first sync every inbound message is an arrival, first
-- seen when it is stored whatever its Date header says. During the first
-- sync (the account has no sync cursor yet) only the last 30 days count:
-- older messages are history, not news.
CREATE TRIGGER IF NOT EXISTS arrivals_on_message_insert AFTER INSERT ON messages
WHEN NEW.direction != 'outbound'
    AND (
        NOT COALESCE((SELECT sync_cursor IS NULL FROM accounts WHERE id = NEW.account_id), 0)
        OR NEW.date >= CAST(strftime('%s', 'now') AS INTEGER) - 30 * 86400
    )
BEGIN
    INSERT INTO arrivals (message_id, account_id, sender_email, first_seen_at)
    VALUES (
        NEW.id,
        NEW.account_id,
        lower(NEW.from_email),
        CASE
            WHEN COALESCE((SELECT sync_cursor IS NULL FROM accounts WHERE id = NEW.account_id), 0)
                AND NEW.date < CAST(strftime('%s', 'now') AS INTEGER) - 2 * 86400
            THEN NEW.date
            ELSE CAST(strftime('%s', 'now') AS INTEGER)
        END
    )
    ON CONFLICT(message_id) DO NOTHING;
END;

-- Backfill: the last 30 days, first seen at their date (never in the
-- future). The daemon places them on its next pass.
INSERT INTO arrivals (message_id, account_id, sender_email, first_seen_at)
SELECT id, account_id, lower(from_email),
       MIN(date, CAST(strftime('%s', 'now') AS INTEGER))
FROM messages
WHERE direction != 'outbound'
  AND date >= CAST(strftime('%s', 'now') AS INTEGER) - 30 * 86400
ON CONFLICT(message_id) DO NOTHING;

-- Every correction the user made: a per-email move (X), a sender's mode
-- (K) and each "Not sure" answer. A log: undo stamps `undone_at` rather
-- than deleting, and the `prior_*` columns put the arrival back exactly.
CREATE TABLE IF NOT EXISTS mode_corrections (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id         TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- email | sender
    scope              TEXT NOT NULL CHECK (scope IN ('email', 'sender')),
    -- The email moved, or the one a sender's mode was set from. No cascade:
    -- the log outlives the email, and holds ids only.
    message_id         TEXT,
    sender_email       TEXT NOT NULL,
    from_mode          TEXT NOT NULL,
    to_mode            TEXT NOT NULL,
    -- The rule that had placed it.
    rule               TEXT,
    -- move | sender | not_sure
    source             TEXT NOT NULL,
    created_at         INTEGER NOT NULL,
    undone_at          INTEGER,
    -- For undo: the email's move before this one, if any.
    prior_moved_to     TEXT,
    prior_moved_at     INTEGER,
    -- For undo of a sender move: the decision before it ('' for none).
    prior_disposition  TEXT,
    -- ... and when it was made, so undo restores it without a new date
    -- that would override email moves made after the original decision.
    prior_decided_at   INTEGER,
    -- To do and Archive add an aspect: the to-do or record it made.
    aspect_id          TEXT
);

CREATE INDEX IF NOT EXISTS idx_mode_corrections_account_created
    ON mode_corrections(account_id, created_at);
CREATE INDEX IF NOT EXISTS idx_mode_corrections_message
    ON mode_corrections(message_id);
