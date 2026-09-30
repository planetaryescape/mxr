-- =========================================================================
-- Fingerprints of sentences an AI draft wrote from the user's history
-- (past emails, habits, relationship summaries), so a later refine or
-- humanize never hands that text to a cloud model without the user's
-- opt-in, even after a daemon restart.
--
-- Only 64-bit hashes of normalised sentences are stored, never text. Rows
-- older than 7 days are pruned on insert, with a per-account cap.
-- Fingerprint first so a lookup by fingerprint alone (a request that names
-- no account) uses the key too.
-- =========================================================================
CREATE TABLE IF NOT EXISTS history_text_fingerprints (
    fingerprint INTEGER NOT NULL,
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (fingerprint, account_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_history_text_fingerprints_age
    ON history_text_fingerprints(account_id, created_at);
