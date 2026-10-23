-- Derived answers never replace immutable arrivals or correction history.
CREATE TABLE IF NOT EXISTS mail_assessments (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    policy_version TEXT NOT NULL,
    input_fingerprint TEXT NOT NULL,
    input_revision INTEGER NOT NULL,
    assessment_json TEXT NOT NULL,
    PRIMARY KEY (message_id, policy_version)
);
CREATE INDEX IF NOT EXISTS idx_mail_assessments_account ON mail_assessments(account_id, policy_version);
CREATE TABLE IF NOT EXISTS assessment_inputs (id INTEGER PRIMARY KEY CHECK(id = 1), revision INTEGER NOT NULL);
INSERT OR IGNORE INTO assessment_inputs VALUES (1, 0);

-- Thread generations retain local mutations and the last reconciled sender generation.
-- Keep generations after reconciliation so a concurrent reader can CAS them.
CREATE TABLE IF NOT EXISTS assessment_thread_inputs (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    dirty INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (account_id, thread_id)
);
CREATE INDEX IF NOT EXISTS idx_assessment_dirty ON assessment_thread_inputs(account_id, thread_id) WHERE dirty = 1;
CREATE INDEX IF NOT EXISTS idx_assessment_sender ON messages(account_id, lower(from_email), thread_id);
-- One token per sender avoids rewriting its entire history on every insert.
-- Readers include it immediately; reconciliation acknowledges it per thread.
CREATE TABLE IF NOT EXISTS assessment_sender_inputs (
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (account_id, email)
);
CREATE INDEX IF NOT EXISTS idx_assessment_thread_senders ON messages(account_id, thread_id, lower(from_email));
CREATE VIEW IF NOT EXISTS assessment_effective_thread_inputs AS
SELECT i.*, max(i.revision, COALESCE((
    SELECT max(s.revision) FROM messages m JOIN assessment_sender_inputs s
      ON s.account_id = m.account_id AND s.email = lower(m.from_email)
    WHERE m.account_id = i.account_id AND m.thread_id = i.thread_id
), -1)) AS effective_revision
FROM assessment_thread_inputs i;
INSERT OR IGNORE INTO assessment_thread_inputs(account_id, thread_id, revision)
SELECT DISTINCT account_id, thread_id, 0 FROM messages;
CREATE TRIGGER IF NOT EXISTS assessment_input_messages_insert AFTER INSERT ON messages

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT NEW.account_id AS account_id, NEW.thread_id AS thread_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT NEW.account_id AS account_id, lower(NEW.from_email) AS email
    UNION SELECT NEW.account_id, lower(json_extract(a.value, '$.email'))
        FROM json_each(json_array(json(NEW.to_addrs), json(NEW.cc_addrs), json(NEW.bcc_addrs))) lists, json_each(lists.value) a
        WHERE NEW.direction != 'inbound' ) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_messages_update AFTER UPDATE ON messages
WHEN OLD.account_id IS NOT NEW.account_id OR OLD.thread_id IS NOT NEW.thread_id OR OLD.message_id_header IS NOT NEW.message_id_header OR OLD.in_reply_to IS NOT NEW.in_reply_to OR OLD.reference_headers IS NOT NEW.reference_headers OR OLD.from_name IS NOT NEW.from_name OR OLD.from_email IS NOT NEW.from_email OR OLD.to_addrs IS NOT NEW.to_addrs OR OLD.cc_addrs IS NOT NEW.cc_addrs OR OLD.bcc_addrs IS NOT NEW.bcc_addrs OR OLD.subject IS NOT NEW.subject OR OLD.date IS NOT NEW.date OR OLD.unsubscribe_method IS NOT NEW.unsubscribe_method OR OLD.direction IS NOT NEW.direction OR OLD.list_id IS NOT NEW.list_id OR (OLD.flags & 48) != (NEW.flags & 48)
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT OLD.account_id AS account_id, OLD.thread_id AS thread_id
    UNION SELECT NEW.account_id AS account_id, NEW.thread_id AS thread_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT OLD.account_id AS account_id, lower(OLD.from_email) AS email
    UNION SELECT OLD.account_id, lower(json_extract(a.value, '$.email'))
        FROM json_each(json_array(json(OLD.to_addrs), json(OLD.cc_addrs), json(OLD.bcc_addrs))) lists, json_each(lists.value) a
        WHERE OLD.direction != 'inbound'
    UNION SELECT NEW.account_id AS account_id, lower(NEW.from_email) AS email
    UNION SELECT NEW.account_id, lower(json_extract(a.value, '$.email'))
        FROM json_each(json_array(json(NEW.to_addrs), json(NEW.cc_addrs), json(NEW.bcc_addrs))) lists, json_each(lists.value) a
        WHERE NEW.direction != 'inbound' ) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_messages_delete AFTER DELETE ON messages

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT OLD.account_id AS account_id, OLD.thread_id AS thread_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT OLD.account_id AS account_id, lower(OLD.from_email) AS email
    UNION SELECT OLD.account_id, lower(json_extract(a.value, '$.email'))
        FROM json_each(json_array(json(OLD.to_addrs), json(OLD.cc_addrs), json(OLD.bcc_addrs))) lists, json_each(lists.value) a
        WHERE OLD.direction != 'inbound' ) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_bodies_insert AFTER INSERT ON bodies

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT account_id, lower(from_email) AS email FROM messages WHERE id = NEW.message_id) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_bodies_update AFTER UPDATE ON bodies
WHEN OLD.message_id IS NOT NEW.message_id OR OLD.text_plain IS NOT NEW.text_plain OR OLD.metadata_json IS NOT NEW.metadata_json
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id
    UNION SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT account_id, lower(from_email) AS email FROM messages WHERE id = OLD.message_id
    UNION SELECT account_id, lower(from_email) AS email FROM messages WHERE id = NEW.message_id) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_bodies_delete AFTER DELETE ON bodies

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT account_id, lower(from_email) AS email FROM messages WHERE id = OLD.message_id) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_account_addresses_insert AFTER INSERT ON account_addresses

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = NEW.account_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_account_addresses_update AFTER UPDATE ON account_addresses
WHEN OLD.account_id IS NOT NEW.account_id OR OLD.email IS NOT NEW.email
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = OLD.account_id UNION SELECT account_id, thread_id FROM messages WHERE account_id = NEW.account_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_account_addresses_delete AFTER DELETE ON account_addresses

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = OLD.account_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_accounts_insert AFTER INSERT ON accounts

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = NEW.id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_accounts_update AFTER UPDATE ON accounts
WHEN OLD.email IS NOT NEW.email
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = OLD.id UNION SELECT account_id, thread_id FROM messages WHERE account_id = NEW.id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_accounts_delete AFTER DELETE ON accounts

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = OLD.id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_contacts_insert AFTER INSERT ON contacts

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT NEW.account_id AS account_id, lower(NEW.email) AS email) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_contacts_update AFTER UPDATE ON contacts
WHEN OLD.account_id IS NOT NEW.account_id OR OLD.email IS NOT NEW.email OR OLD.total_outbound IS NOT NEW.total_outbound
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT OLD.account_id AS account_id, lower(OLD.email) AS email
    UNION SELECT NEW.account_id AS account_id, lower(NEW.email) AS email) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_contacts_delete AFTER DELETE ON contacts

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_sender_inputs(account_id, email, revision)
    SELECT account_id, email, (SELECT revision FROM assessment_inputs WHERE id = 1) FROM (SELECT OLD.account_id AS account_id, lower(OLD.email) AS email) affected WHERE email IS NOT NULL AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, email) DO UPDATE SET revision = excluded.revision;
END;
-- A sender decision also changes whether a To/Cc participant is human,
-- even in another sender's thread. Invalidate that account, not only From.
CREATE TRIGGER IF NOT EXISTS assessment_input_screener_decisions_insert AFTER INSERT ON screener_decisions

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = NEW.account_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_screener_decisions_update AFTER UPDATE ON screener_decisions
WHEN OLD.account_id IS NOT NEW.account_id OR OLD.sender_email IS NOT NEW.sender_email OR OLD.disposition IS NOT NEW.disposition OR OLD.decided_at IS NOT NEW.decided_at
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = OLD.account_id UNION SELECT account_id, thread_id FROM messages WHERE account_id = NEW.account_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_screener_decisions_delete AFTER DELETE ON screener_decisions

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE account_id = OLD.account_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_arrivals_insert AFTER INSERT ON arrivals
WHEN NEW.moved_to IS NOT NULL
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_arrivals_update AFTER UPDATE ON arrivals
WHEN OLD.message_id IS NOT NEW.message_id OR OLD.account_id IS NOT NEW.account_id OR OLD.sender_email IS NOT NEW.sender_email OR OLD.moved_to IS NOT NEW.moved_to OR OLD.moved_at IS NOT NEW.moved_at OR OLD.superseded_by IS NOT NEW.superseded_by
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id UNION SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_arrivals_delete AFTER DELETE ON arrivals
WHEN OLD.moved_to IS NOT NULL
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rule_treatments_insert AFTER INSERT ON rule_treatments

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rule_treatments_update AFTER UPDATE ON rule_treatments
WHEN OLD.message_id IS NOT NEW.message_id OR OLD.rule_id IS NOT NEW.rule_id OR OLD.rule_updated_at IS NOT NEW.rule_updated_at OR OLD.treatment IS NOT NEW.treatment OR OLD.rule_name IS NOT NEW.rule_name
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id UNION SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rule_treatments_delete AFTER DELETE ON rule_treatments

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_calendar_invites_insert AFTER INSERT ON calendar_invites

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_calendar_invites_update AFTER UPDATE ON calendar_invites
WHEN OLD.message_id IS NOT NEW.message_id
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id UNION SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_calendar_invites_delete AFTER DELETE ON calendar_invites

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_delivery_messages_insert AFTER INSERT ON delivery_messages

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_delivery_messages_update AFTER UPDATE ON delivery_messages
WHEN OLD.message_id IS NOT NEW.message_id
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id UNION SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_delivery_messages_delete AFTER DELETE ON delivery_messages

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_message_labels_insert AFTER INSERT ON message_labels
WHEN EXISTS (SELECT 1 FROM labels WHERE id = NEW.label_id AND provider_id IN ('SPAM', 'TRASH'))
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_message_labels_update AFTER UPDATE ON message_labels
WHEN EXISTS (SELECT 1 FROM labels WHERE id = OLD.label_id AND provider_id IN ('SPAM', 'TRASH')) OR EXISTS (SELECT 1 FROM labels WHERE id = NEW.label_id AND provider_id IN ('SPAM', 'TRASH'))
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id UNION SELECT account_id, thread_id FROM messages WHERE id = NEW.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_message_labels_delete AFTER DELETE ON message_labels
WHEN EXISTS (SELECT 1 FROM labels WHERE id = OLD.label_id AND provider_id IN ('SPAM', 'TRASH'))
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages WHERE id = OLD.message_id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_labels_update AFTER UPDATE ON labels
WHEN OLD.provider_id IS NOT NEW.provider_id AND (OLD.provider_id IN ('SPAM','TRASH') OR NEW.provider_id IN ('SPAM','TRASH'))
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT m.account_id, m.thread_id FROM messages m JOIN message_labels ml ON ml.message_id = m.id WHERE ml.label_id = NEW.id) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rules_insert AFTER INSERT ON rules

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rules_update AFTER UPDATE ON rules

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rules_delete AFTER DELETE ON rules

BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM (SELECT account_id, thread_id FROM messages) affected WHERE EXISTS (SELECT 1 FROM accounts own WHERE own.id = affected.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;

-- Cascade deletes remove the junction rows before an AFTER trigger can find them.
CREATE TRIGGER IF NOT EXISTS assessment_input_labels_delete BEFORE DELETE ON labels
WHEN OLD.provider_id IN ('SPAM', 'TRASH')
BEGIN
    UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1;
    INSERT INTO assessment_thread_inputs(account_id, thread_id, revision, dirty)
    SELECT DISTINCT m.account_id, m.thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1
    FROM messages m JOIN message_labels ml ON ml.message_id = m.id WHERE ml.label_id = OLD.id AND EXISTS (SELECT 1 FROM accounts own WHERE own.id = m.account_id)
    ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1;
END;
