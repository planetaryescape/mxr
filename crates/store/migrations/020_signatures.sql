-- =========================================================================
-- Signatures: local-only outgoing compose signatures.
--
-- Signatures are reusable markdown/plain-text blocks. Defaults are scoped by
-- a deterministic key so NULL-heavy global/account/address defaults can use a
-- normal primary key instead of partial unique indexes.
-- =========================================================================
CREATE TABLE IF NOT EXISTS signatures (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS signature_defaults (
    scope_key    TEXT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('new', 'reply')),
    signature_id TEXT NOT NULL REFERENCES signatures(id) ON DELETE CASCADE,
    account_id   TEXT,
    from_email   TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (scope_key, kind),
    CHECK (scope_key != ''),
    CHECK (from_email IS NULL OR account_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_signature_defaults_signature_id
    ON signature_defaults(signature_id);

CREATE INDEX IF NOT EXISTS idx_signature_defaults_account_id
    ON signature_defaults(account_id);
