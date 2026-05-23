-- =========================================================================
-- Deliveries: physical-package tracking distilled from inbound mail.
--
-- One row per parcel (per tracking number), or per order while no tracking
-- number exists yet. Detection is local-first: a deterministic heuristic
-- (+ optional local-LLM enrichment) writes these rows during the daemon's
-- post-sync fan-out, the same place semantic/rules/relationship run.
--
-- A single order spawns many emails (ordered -> shipped -> out_for_delivery
-- -> delivered). They are collapsed into ONE delivery row keyed by
-- `dedup_key`, and `status` advances monotonically. `resolved_at` marks a
-- closed delivery (set automatically when a "delivered" signal arrives, or
-- manually by the user); `dismissed_at` hides a false positive. Both are
-- non-destructive so the row (and its provenance) is retained.
-- =========================================================================
CREATE TABLE IF NOT EXISTS deliveries (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Correlation key: tracking number when known, else "merchant|order".
    dedup_key TEXT NOT NULL,
    merchant TEXT,
    carrier TEXT,
    tracking_number TEXT,
    tracking_url TEXT,
    order_number TEXT,
    -- Normalized lifecycle status (text); typed enum lives in the
    -- `deliveries` crate to keep core/protocol free of feature creep.
    status TEXT NOT NULL,
    eta_from INTEGER,
    eta_until INTEGER,
    delivered_at INTEGER,
    -- JSON array of {name, quantity?}; schema.org/LLM-derived.
    items_json TEXT NOT NULL DEFAULT '[]',
    confidence REAL NOT NULL DEFAULT 0,
    -- How the row was detected: 'schema' | 'llm' | 'heuristic'.
    source TEXT NOT NULL,
    -- Latest contributing thread (for "open in mailbox").
    thread_id TEXT,
    last_event_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    resolved_at INTEGER,
    dismissed_at INTEGER
);

-- Dedup/correlation: at most one delivery per (account, dedup_key). Upserts
-- use `ON CONFLICT(account_id, dedup_key)` — never INSERT OR REPLACE, which
-- would cascade-delete the provenance rows.
CREATE UNIQUE INDEX IF NOT EXISTS idx_deliveries_dedup
    ON deliveries(account_id, dedup_key);

-- Hot path: the active list (not delivered, not dismissed), newest first.
CREATE INDEX IF NOT EXISTS idx_deliveries_active
    ON deliveries(account_id, last_event_at)
    WHERE dismissed_at IS NULL AND delivered_at IS NULL;

-- Provenance: which emails contributed to a delivery and what stage each
-- signalled. Lets the UI open source threads and supports correlation/debug.
CREATE TABLE IF NOT EXISTS delivery_messages (
    delivery_id TEXT NOT NULL REFERENCES deliveries(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    thread_id TEXT,
    email_kind TEXT,
    detected_at INTEGER NOT NULL,
    PRIMARY KEY (delivery_id, message_id)
);

-- Reverse lookup: "is this message already linked to a delivery?"
CREATE INDEX IF NOT EXISTS idx_delivery_messages_message
    ON delivery_messages(message_id);
