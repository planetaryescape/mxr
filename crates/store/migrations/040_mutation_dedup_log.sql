-- Migration 040: mutation_dedup_log
--
-- Idempotent-retry safety for provider-side mutations. Before
-- calling provider.apply_mutation, the daemon checks whether the
-- (mutation_id, provider_message_id) pair already has a row.
-- If so, skip the call. After a successful call, insert the row.
-- TTL = 24h; prune via the maintenance loop alongside the
-- existing mutation_undo_log prune.
--
-- ReadAndArchive fans out into SetRead + ModifyLabels for the same
-- envelope under one mutation_id. The daemon disambiguates the two
-- rows by suffixing provider_message_id with "#read" / "#labels"
-- when inserting; the rest of the code treats provider_message_id
-- as opaque dedup key, never re-derives the envelope id from it.

CREATE TABLE IF NOT EXISTS mutation_dedup_log (
    -- UUIDv7 string. One id per MutationCommand from the IPC layer;
    -- shared across all envelopes in a batch.
    mutation_id TEXT NOT NULL,

    -- Provider message id, optionally suffixed with "#read" or
    -- "#labels" for the two halves of ReadAndArchive. Opaque to
    -- the store.
    provider_message_id TEXT NOT NULL,

    account_id TEXT NOT NULL,

    -- Unix epoch seconds.
    applied_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,

    PRIMARY KEY (mutation_id, provider_message_id)
);

CREATE INDEX IF NOT EXISTS idx_mutation_dedup_log_expires_at
    ON mutation_dedup_log(expires_at);
