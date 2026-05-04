-- =========================================================================
-- Materialized contact-dimension table. Refreshed periodically by the
-- reply-pair reconciler loop AND on `mxr doctor --refresh-contacts`.
-- Stores per-account (account_id, email) tuples — the same email under two
-- accounts gets two rows.
--
-- `cadence_days_p50` is filled by the contacts refresher; null until first
-- refresh sees enough events to compute a median (>=2 messages).
-- =========================================================================
CREATE TABLE IF NOT EXISTS contacts (
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email               TEXT NOT NULL,
    display_name        TEXT,
    first_seen_at       INTEGER NOT NULL,
    last_seen_at        INTEGER NOT NULL,
    last_inbound_at     INTEGER,
    last_outbound_at    INTEGER,
    total_inbound       INTEGER NOT NULL DEFAULT 0,
    total_outbound      INTEGER NOT NULL DEFAULT 0,
    replied_count       INTEGER NOT NULL DEFAULT 0,
    cadence_days_p50    REAL,
    is_list_sender      INTEGER NOT NULL DEFAULT 0,
    list_id             TEXT,
    refreshed_at        INTEGER NOT NULL,
    PRIMARY KEY (account_id, email)
);

CREATE INDEX IF NOT EXISTS idx_contacts_last_seen
    ON contacts(account_id, last_seen_at DESC);

CREATE INDEX IF NOT EXISTS idx_contacts_inbound_count
    ON contacts(account_id, total_inbound DESC);

CREATE INDEX IF NOT EXISTS idx_contacts_outbound_count
    ON contacts(account_id, total_outbound DESC);
