-- =========================================================================
-- Reading items sent to Marginalia, and the highlights read back from it.
--
-- reading_marginalia records which Marginalia article an item became, so
-- opening it again goes straight there and its highlights can be read back.
-- Highlights read back live in reading_highlights with origin 'marginalia'
-- and Marginalia's own id in origin_id; a re-read replaces them, so one
-- deleted in Marginalia goes here too. Both cascade with the message.
-- =========================================================================
CREATE TABLE IF NOT EXISTS reading_marginalia (
    message_id            TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    idx                   INTEGER NOT NULL,
    account_id            TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    article_id            TEXT NOT NULL,
    read_url              TEXT NOT NULL,
    -- Which text was sent: 'issue' or 'article'.
    view                  TEXT NOT NULL CHECK (view IN ('issue', 'article')),
    sent_at               INTEGER NOT NULL,
    highlights_synced_at  INTEGER,
    PRIMARY KEY (message_id, idx)
);

CREATE INDEX IF NOT EXISTS idx_reading_marginalia_account ON reading_marginalia(account_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_reading_highlights_origin
    ON reading_highlights(origin, origin_id) WHERE origin_id IS NOT NULL;
