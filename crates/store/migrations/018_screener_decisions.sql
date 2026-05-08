-- =========================================================================
-- Screener: consent-based first-touch quarantine.
--
-- Stores the user's per-(account, sender_email) classification:
--   * 'allow'        — normal inbox treatment
--   * 'deny'         — auto-trash + mark-read on ingest
--   * 'feed'         — newsletters / non-urgent: skip inbox, go to feed
--   * 'paper_trail'  — receipts / records: skip inbox, archive
--   * 'unknown'      — explicit "I haven't decided yet" (rare; usually
--                      absence of a row implies unknown)
--
-- The screener queue is computed dynamically: inbound messages whose
-- sender doesn't have an `allow`/`deny`/`feed`/`paper_trail` decision
-- and is not the user themselves.
--
-- Local-only by default. `route_label` is an optional opt-in: when
-- non-null, the screener's classification also writes to the named
-- provider label so mobile/web sees the categorisation.
-- =========================================================================
CREATE TABLE IF NOT EXISTS screener_decisions (
    account_id   TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    sender_email TEXT NOT NULL COLLATE NOCASE,
    disposition  TEXT NOT NULL CHECK (
        disposition IN ('allow', 'deny', 'feed', 'paper_trail', 'unknown')
    ),
    route_label  TEXT,
    decided_at   INTEGER NOT NULL,
    PRIMARY KEY (account_id, sender_email)
);

CREATE INDEX IF NOT EXISTS idx_screener_decisions_disposition
    ON screener_decisions (account_id, disposition);
