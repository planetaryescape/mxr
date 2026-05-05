-- Migration 012: mutation_undo_log
--
-- Tracks the prior state of envelopes affected by recent destructive
-- mutations (Archive, Trash, Spam, SetRead, ReadAndArchive). When the
-- user requests undo within a short window the daemon reads the entry,
-- issues a reverse mutation against the provider, and restores the
-- envelope's flags + label memberships in the local store.
--
-- Bounded by `expires_at` — the daemon refuses undo requests past it
-- and (eventually) prunes stale rows. Keeps memory and disk usage
-- bounded under heavy mutation traffic.

CREATE TABLE IF NOT EXISTS mutation_undo_log (
    -- UUIDv7 string. One row per Mutation request (a batch of envelopes
    -- shares a single id so a single Undo restores the whole batch).
    mutation_id TEXT PRIMARY KEY NOT NULL,

    -- Coarse mutation kind ("archive", "trash", "spam", "set_read",
    -- "read_and_archive"). Tells the daemon what reverse op to issue
    -- against the provider.
    mutation_kind TEXT NOT NULL,

    -- JSON array of {message_id, account_id, provider_id, prior_flags,
    -- prior_label_provider_ids} entries — one per envelope in the batch.
    prior_state_json TEXT NOT NULL,

    -- Unix epoch seconds.
    applied_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mutation_undo_log_expires_at
    ON mutation_undo_log(expires_at);
