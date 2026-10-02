-- =========================================================================
-- Messages whose lexical search entry may be stale.
--
-- A mutation changes a message in the store, then reindexes it in search.
-- When reading the message back for the index fails, its id lands here so
-- the next reindex, or startup maintenance, retries it. The search index's
-- document count does not change in that case, so the count-based startup
-- repair would never notice.
-- =========================================================================
CREATE TABLE IF NOT EXISTS search_reindex_pending (
    message_id TEXT PRIMARY KEY,
    marked_at  INTEGER NOT NULL
);
