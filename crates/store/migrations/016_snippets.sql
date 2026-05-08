-- =========================================================================
-- Snippets: user-defined short-text expansions for compose.
--
-- Identified by a short keyword (`name`, the prefix the user types after
-- `;` in compose, e.g. `;sig`). Body is plain text/markdown. `vars` is a
-- JSON array of declared `{var}` placeholders so missing-var detection
-- can surface a warning before send without parsing the body twice.
--
-- Snippets are local-only. They never roundtrip to the provider.
-- =========================================================================
CREATE TABLE IF NOT EXISTS snippets (
    name TEXT PRIMARY KEY,
    body TEXT NOT NULL,
    vars TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
