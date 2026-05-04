-- =========================================================================
-- Analytics columns on `messages`. All additive; no row-level data movement.
--
-- direction: 'inbound' | 'outbound' | 'unknown' — populated from
--   `from_email` against `account_addresses` at sync time (Slice 8). Default
--   'unknown' until then; doctor --rebuild-analytics reclassifies retroactively.
--
-- list_id: RFC 2919 List-Id header. Already parsed into bodies.metadata_json;
--   promoted here for indexed grouping (Slice 6, mxr unsub --rank).
--
-- body_word_count / body_quoted_lines: cheap heuristics computed at sync
--   time, used by future text-density analytics. Nullable until populated.
--
-- The pool.rs runtime applies these via `add_column_if_missing`, so a
-- partial run is safe — re-running the migration is a no-op for already-
-- present columns.
-- =========================================================================
ALTER TABLE messages ADD COLUMN direction TEXT NOT NULL DEFAULT 'unknown'
    CHECK (direction IN ('inbound', 'outbound', 'unknown'));

ALTER TABLE messages ADD COLUMN list_id TEXT;

ALTER TABLE messages ADD COLUMN body_word_count INTEGER;

ALTER TABLE messages ADD COLUMN body_quoted_lines INTEGER;

CREATE INDEX IF NOT EXISTS idx_messages_account_direction_date
    ON messages(account_id, direction, date DESC);

CREATE INDEX IF NOT EXISTS idx_messages_list_id
    ON messages(list_id) WHERE list_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_messages_from_date
    ON messages(from_email, date DESC);

CREATE INDEX IF NOT EXISTS idx_attachments_mime
    ON attachments(mime_type);
