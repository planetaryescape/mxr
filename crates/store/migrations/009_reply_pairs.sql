-- =========================================================================
-- Reply pairs: each row links a reply (`reply_message_id`) to the message it
-- responds to (`parent_message_id`). Direction encodes who did the replying:
-- 'i_replied' when the reply is outbound (I responded to an inbound parent),
-- 'they_replied' when the reply is inbound (they responded to an outbound).
--
-- Pair creation is forward-attempted at sync time. When the parent isn't
-- locally known yet (out-of-order delivery on IMAP, especially), the reply
-- waits in `reply_pair_pending` and the reconciler resolves it later.
--
-- `business_hours_latency_seconds` is filled later by the reconciler/Slice 14.
-- =========================================================================
CREATE TABLE IF NOT EXISTS reply_pairs (
    reply_message_id              TEXT NOT NULL PRIMARY KEY
                                   REFERENCES messages(id) ON DELETE CASCADE,
    parent_message_id             TEXT NOT NULL
                                   REFERENCES messages(id) ON DELETE CASCADE,
    account_id                    TEXT NOT NULL
                                   REFERENCES accounts(id) ON DELETE CASCADE,
    counterparty_email            TEXT NOT NULL,
    direction                     TEXT NOT NULL CHECK (direction IN (
                                       'i_replied', 'they_replied'
                                   )),
    parent_received_at            INTEGER NOT NULL,
    replied_at                    INTEGER NOT NULL,
    latency_seconds               INTEGER NOT NULL,
    business_hours_latency_seconds INTEGER,
    created_at                    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reply_pairs_party
    ON reply_pairs(counterparty_email, replied_at DESC);

CREATE INDEX IF NOT EXISTS idx_reply_pairs_account
    ON reply_pairs(account_id, replied_at DESC);

CREATE INDEX IF NOT EXISTS idx_reply_pairs_direction
    ON reply_pairs(direction, replied_at DESC);

-- Replies whose parent isn't in `messages` yet. Resolved by the reconciler
-- (Slice 10) when the parent eventually arrives. Single-row per reply.
CREATE TABLE IF NOT EXISTS reply_pair_pending (
    reply_message_id    TEXT NOT NULL PRIMARY KEY
                         REFERENCES messages(id) ON DELETE CASCADE,
    in_reply_to_header  TEXT NOT NULL,
    account_id          TEXT NOT NULL
                         REFERENCES accounts(id) ON DELETE CASCADE,
    created_at          INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reply_pair_pending_header
    ON reply_pair_pending(in_reply_to_header);
