-- =========================================================================
-- Named, reusable filter presets for `mxr activity` queries. Mirrors the
-- saved_searches pattern: slug-keyed, JSON-serialized filter payload, with
-- creation/update/last-used timestamps for sorting and pruning.
--
-- Strictly local: like the activity rows themselves, these never leave
-- the device.
-- =========================================================================
CREATE TABLE IF NOT EXISTS saved_activity_filters (
    slug         TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    filter_json  TEXT NOT NULL,           -- serialized `ActivityFilter`
    created_at   INTEGER NOT NULL,        -- unix ms
    updated_at   INTEGER NOT NULL,        -- unix ms
    last_used_at INTEGER                  -- unix ms; NULL until first use
);

CREATE INDEX IF NOT EXISTS idx_saved_activity_filters_last_used
    ON saved_activity_filters (last_used_at DESC);
