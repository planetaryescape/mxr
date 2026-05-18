-- Migration 041: message_keywords
--
-- Per-message custom IMAP-style keywords (Dovecot $Forwarded,
-- $NotJunk, $MDNSent, user-defined $Work etc.). Separate from
-- the messages.flags bitfield which carries the fixed system
-- flag set (READ/STARRED/DRAFT/SENT/TRASH/SPAM/ARCHIVED/ANSWERED).
--
-- Foreign-keyed to messages so deletes cascade. Keywords are stored
-- case-preserved as received from the provider.

CREATE TABLE IF NOT EXISTS message_keywords (
    message_id TEXT NOT NULL,
    keyword    TEXT NOT NULL,
    PRIMARY KEY (message_id, keyword),
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- For `is:$forwarded`-style search filters and reverse lookups.
CREATE INDEX IF NOT EXISTS idx_message_keywords_keyword
    ON message_keywords(keyword);
