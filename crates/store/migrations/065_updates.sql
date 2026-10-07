-- =========================================================================
-- Updates as a briefing by source (blueprint 22, phase 4).
--
-- update_facts caches what the rules derive from one automated message:
-- its source, template, fact line, quoted numbers, rule signal, window
-- and tracked state, as JSON the daemon decodes. It is recomputable: a
-- row whose rules_version is behind is derived again. The cascade drops a
-- fact with its message, so deleting an email deletes its fact.
--
-- update_sources holds how you tuned a source (every digest, changes
-- only, muted, breakthrough) and how many digests in a row you let it go
-- without opening it, for the one-time mute suggestion. Keys are sender
-- domains and repository names, never message text.
-- =========================================================================
CREATE TABLE IF NOT EXISTS update_facts (
    message_id     TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source_key     TEXT NOT NULL,
    template_key   TEXT NOT NULL,
    message_date   INTEGER NOT NULL,
    relevant_until INTEGER,
    fact_json      TEXT NOT NULL,
    rules_version  INTEGER NOT NULL,
    computed_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_update_facts_source
    ON update_facts(account_id, source_key, message_date);

CREATE TABLE IF NOT EXISTS update_sources (
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    source_key     TEXT NOT NULL,
    setting        TEXT NOT NULL DEFAULT 'every_digest'
                   CHECK (setting IN ('every_digest', 'changes_only', 'muted', 'breakthrough')),
    decided_at     INTEGER,
    let_go_streak  INTEGER NOT NULL DEFAULT 0,
    last_let_go_at INTEGER,
    suggested_at   INTEGER,
    PRIMARY KEY (account_id, source_key)
);
