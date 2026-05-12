CREATE TABLE IF NOT EXISTS contact_style (
    account_id                 TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email                      TEXT NOT NULL COLLATE NOCASE,
    formality_score            REAL NOT NULL DEFAULT 0,
    formality_score_theirs     REAL NOT NULL DEFAULT 0,
    avg_sentence_len           REAL NOT NULL DEFAULT 0,
    avg_sentence_len_theirs    REAL NOT NULL DEFAULT 0,
    msg_count_used             INTEGER NOT NULL DEFAULT 0,
    msg_count_used_theirs      INTEGER NOT NULL DEFAULT 0,
    metrics_json               TEXT NOT NULL DEFAULT '{}',
    metrics_json_theirs        TEXT NOT NULL DEFAULT '{}',
    computed_at                INTEGER NOT NULL,
    source_hash                TEXT NOT NULL,
    PRIMARY KEY (account_id, email)
);

CREATE INDEX IF NOT EXISTS idx_contact_style_computed_at
    ON contact_style(account_id, computed_at DESC);
