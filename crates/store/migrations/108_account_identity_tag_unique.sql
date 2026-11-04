-- A stored tag must be held by one account. Accounts created before this
-- index can share one (the tag was unique by convention only). The oldest
-- account keeps it; the others lose it here, and the identity backfill derives
-- a new tag for them on this same open, against the tags still held.
UPDATE accounts
SET identity_tag = NULL
WHERE id IN (
    SELECT id FROM (
        SELECT id,
               ROW_NUMBER() OVER (PARTITION BY identity_tag ORDER BY created_at, id) AS holder_rank
        FROM accounts
        WHERE identity_tag IS NOT NULL
    )
    WHERE holder_rank > 1
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_accounts_identity_tag
    ON accounts(identity_tag)
    WHERE identity_tag IS NOT NULL;
