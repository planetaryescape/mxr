CREATE TABLE IF NOT EXISTS personal_categories (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    normalized_name TEXT NOT NULL UNIQUE,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS message_category_memberships (
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    category_id TEXT NOT NULL REFERENCES personal_categories(id) ON DELETE CASCADE,
    assigned    INTEGER NOT NULL CHECK (assigned IN (0, 1)),
    revision    INTEGER NOT NULL CHECK (revision > 0),
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (message_id, category_id)
);

CREATE INDEX IF NOT EXISTS idx_category_memberships_category_message
    ON message_category_memberships(category_id, message_id) WHERE assigned = 1;

CREATE TABLE IF NOT EXISTS category_membership_undo (
    id          TEXT PRIMARY KEY,
    snapshot    TEXT NOT NULL,
    expires_at  INTEGER NOT NULL
);
