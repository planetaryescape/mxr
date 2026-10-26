-- A new-sender check must look up the requested addresses, not parse every
-- sent message's To/Cc/Bcc on each visit to Now or the rail. Keep one row per
-- supporting message so edits, direction changes and deletion remain exact.
CREATE TABLE IF NOT EXISTS outbound_recipients (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE ON UPDATE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    PRIMARY KEY (message_id, email)
);
CREATE INDEX IF NOT EXISTS idx_outbound_recipients_address ON outbound_recipients(account_id, email);

CREATE VIEW IF NOT EXISTS outbound_recipient_projection AS
SELECT DISTINCT m.id AS message_id, m.account_id, lower(json_extract(a.value, '$.email')) AS email
FROM messages m,
    json_each(json_array(json(m.to_addrs), json(m.cc_addrs), json(m.bcc_addrs))) lists,
    json_each(lists.value) a
WHERE m.direction = 'outbound' AND json_extract(a.value, '$.email') IS NOT NULL;

INSERT OR IGNORE INTO outbound_recipients SELECT * FROM outbound_recipient_projection;

CREATE TRIGGER IF NOT EXISTS outbound_recipients_insert AFTER INSERT ON messages
WHEN NEW.direction = 'outbound'
BEGIN
    INSERT INTO outbound_recipients
    SELECT * FROM outbound_recipient_projection WHERE message_id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS outbound_recipients_update
AFTER UPDATE OF id, account_id, direction, to_addrs, cc_addrs, bcc_addrs ON messages
WHEN OLD.id IS NOT NEW.id OR OLD.account_id IS NOT NEW.account_id
    OR OLD.direction IS NOT NEW.direction OR OLD.to_addrs IS NOT NEW.to_addrs
    OR OLD.cc_addrs IS NOT NEW.cc_addrs OR OLD.bcc_addrs IS NOT NEW.bcc_addrs
BEGIN
    DELETE FROM outbound_recipients WHERE message_id IN (OLD.id, NEW.id);
    INSERT INTO outbound_recipients
    SELECT * FROM outbound_recipient_projection WHERE message_id = NEW.id;
END;

-- Updates takes a bounded history per sender. This index lets LIMIT stop
-- before loading the sender's older mail or another account's messages.
CREATE INDEX IF NOT EXISTS idx_messages_sender_history
ON messages(account_id, lower(from_email), date DESC, id DESC);
