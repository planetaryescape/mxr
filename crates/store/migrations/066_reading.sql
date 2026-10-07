-- =========================================================================
-- Reading (blueprint 22, phase 5): newsletters as an edition, a reader and
-- a Later shelf.
--
-- reading_items is the extraction cache: one row per readable item of a
-- newsletter message. Index 0 is the issue; a digest's links are 1..n.
-- Rebuilt when extractor_version moves on. No model, no network.
--
-- reading_state is what you did with an item: Later (yours, never expires)
-- and local engagement (opened, seconds read, how far, finished), which
-- ranks sources and backs the unsubscribe evidence. Engagement is never
-- written with MXR_ACTIVITY=off.
--
-- reading_articles caches an article fetched on request, so Later reads
-- offline. reading_highlights holds passages you saved.
--
-- Everything keyed by a message goes with it (ON DELETE CASCADE): deleting
-- an email deletes what was built from it, the fetched article and the
-- highlights quoted from it included.
--
-- reading_sources holds per-source choices (the sender's own layout, a
-- dismissed unsubscribe offer) and reading_visit when Reading was last
-- opened, for "Since you were last here".
-- =========================================================================
CREATE TABLE IF NOT EXISTS reading_items (
    message_id        TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    idx               INTEGER NOT NULL,
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind              TEXT NOT NULL CHECK (kind IN ('issue', 'link')),
    shape             TEXT NOT NULL CHECK (shape IN ('single', 'digest', 'teaser', 'notice')),
    title             TEXT NOT NULL,
    standfirst        TEXT,
    url               TEXT,
    domain            TEXT,
    tracked           INTEGER NOT NULL DEFAULT 0,
    words             INTEGER NOT NULL DEFAULT 0,
    extractor_version INTEGER NOT NULL,
    PRIMARY KEY (message_id, idx)
);

CREATE INDEX IF NOT EXISTS idx_reading_items_account ON reading_items(account_id);

CREATE TABLE IF NOT EXISTS reading_state (
    message_id     TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    idx            INTEGER NOT NULL,
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    later_at       INTEGER,
    -- "Still want it?" answered once, by keeping it.
    kept_at        INTEGER,
    opened_at      INTEGER,
    dwell_ms       INTEGER NOT NULL DEFAULT 0,
    progress       REAL NOT NULL DEFAULT 0,
    finished_at    INTEGER,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (message_id, idx)
);

CREATE INDEX IF NOT EXISTS idx_reading_state_later
    ON reading_state(account_id, later_at) WHERE later_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS reading_articles (
    message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    idx          INTEGER NOT NULL,
    url          TEXT NOT NULL,
    final_url    TEXT,
    status       TEXT NOT NULL CHECK (status IN ('ok', 'failed')),
    title        TEXT,
    byline       TEXT,
    site_name    TEXT,
    html         TEXT,
    paragraphs   TEXT,
    words        INTEGER NOT NULL DEFAULT 0,
    contacted    TEXT NOT NULL DEFAULT '[]',
    error        TEXT,
    fetched_at   INTEGER NOT NULL,
    PRIMARY KEY (message_id, idx)
);

CREATE TABLE IF NOT EXISTS reading_highlights (
    id          TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    view        TEXT NOT NULL CHECK (view IN ('issue', 'article')),
    quote       TEXT NOT NULL,
    note        TEXT,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reading_highlights_message ON reading_highlights(message_id);

CREATE TABLE IF NOT EXISTS reading_sources (
    account_id                 TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    sender_email               TEXT NOT NULL,
    original_layout            INTEGER NOT NULL DEFAULT 0,
    unsubscribe_offer_dismissed_at INTEGER,
    PRIMARY KEY (account_id, sender_email)
);

CREATE TABLE IF NOT EXISTS reading_visit (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    first_seen INTEGER,
    boundary   INTEGER,
    last_seen  INTEGER
);
