ALTER TABLE saved_searches
    ADD COLUMN search_mode TEXT NOT NULL DEFAULT '\"lexical\"';

CREATE TABLE IF NOT EXISTS semantic_profiles (
    id                 TEXT PRIMARY KEY,
    profile_name       TEXT NOT NULL UNIQUE,
    backend            TEXT NOT NULL,
    model_revision     TEXT NOT NULL,
    dimensions         INTEGER NOT NULL,
    status             TEXT NOT NULL,
    installed_at       INTEGER,
    activated_at       INTEGER,
    last_indexed_at    INTEGER,
    progress_completed INTEGER NOT NULL DEFAULT 0,
    progress_total     INTEGER NOT NULL DEFAULT 0,
    last_error         TEXT
);

CREATE TABLE IF NOT EXISTS semantic_chunks (
    id            TEXT PRIMARY KEY,
    message_id    TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_kind   TEXT NOT NULL,
    ordinal       INTEGER NOT NULL,
    normalized    TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE(message_id, source_kind, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_semantic_chunks_message_id
    ON semantic_chunks(message_id);

CREATE TABLE IF NOT EXISTS semantic_embeddings (
    chunk_id      TEXT NOT NULL REFERENCES semantic_chunks(id) ON DELETE CASCADE,
    profile_id    TEXT NOT NULL REFERENCES semantic_profiles(id) ON DELETE CASCADE,
    dimensions    INTEGER NOT NULL,
    vector_blob   BLOB NOT NULL,
    status        TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    PRIMARY KEY (chunk_id, profile_id)
);

CREATE INDEX IF NOT EXISTS idx_semantic_embeddings_profile_id
    ON semantic_embeddings(profile_id);
