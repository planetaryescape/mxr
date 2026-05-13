-- Slice 4.2 of docs/ai-email/04-timing-cadence.md.
-- Manually-curated watchlist of contacts whose cadence the user
-- wants flagged when it drifts. (List senders are excluded by
-- default; the user can override.)

CREATE TABLE IF NOT EXISTS relationship_watchlist (
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email               TEXT NOT NULL COLLATE NOCASE,
    expected_days       REAL,                       -- override; NULL = use contact cadence
    note                TEXT,
    added_at            INTEGER NOT NULL,
    PRIMARY KEY (account_id, email)
);

CREATE INDEX IF NOT EXISTS idx_relationship_watchlist_account
    ON relationship_watchlist(account_id);
