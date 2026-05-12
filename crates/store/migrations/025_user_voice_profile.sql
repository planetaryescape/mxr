CREATE TABLE IF NOT EXISTS user_voice_profile (
    account_id              TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    formality_score         REAL NOT NULL DEFAULT 0,
    avg_sentence_len        REAL NOT NULL DEFAULT 0,
    msg_count_used          INTEGER NOT NULL DEFAULT 0,
    metrics_json            TEXT NOT NULL DEFAULT '{}',
    register_modes_json     TEXT NOT NULL DEFAULT '[]',
    computed_at             INTEGER NOT NULL,
    source_hash             TEXT NOT NULL
);
