CREATE TABLE IF NOT EXISTS calendar_invites (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    message_id          TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    method              TEXT,
    uid                 TEXT,
    recurrence_id       TEXT,
    sequence            INTEGER,
    summary             TEXT,
    starts_at           TEXT,
    ends_at             TEXT,
    organizer_email     TEXT,
    current_partstat    TEXT,
    rsvp_requested      INTEGER NOT NULL DEFAULT 0,
    metadata_json       TEXT NOT NULL,
    raw_ics             TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    UNIQUE(message_id, uid, recurrence_id, method, sequence)
);

CREATE INDEX IF NOT EXISTS idx_calendar_invites_message
    ON calendar_invites(message_id);

CREATE INDEX IF NOT EXISTS idx_calendar_invites_account_uid
    ON calendar_invites(account_id, uid);

CREATE INDEX IF NOT EXISTS idx_calendar_invites_starts_at
    ON calendar_invites(starts_at);

