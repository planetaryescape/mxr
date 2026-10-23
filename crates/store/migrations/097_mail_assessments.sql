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
CREATE TRIGGER IF NOT EXISTS assessment_input_accounts_insert AFTER INSERT ON accounts
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_accounts_update AFTER UPDATE ON accounts
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_accounts_delete AFTER DELETE ON accounts
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_account_addresses_insert AFTER INSERT ON account_addresses
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_account_addresses_update AFTER UPDATE ON account_addresses
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_account_addresses_delete AFTER DELETE ON account_addresses
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_messages_insert AFTER INSERT ON messages
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_messages_update AFTER UPDATE ON messages
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_messages_delete AFTER DELETE ON messages
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_message_labels_insert AFTER INSERT ON message_labels
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_message_labels_update AFTER UPDATE ON message_labels
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_message_labels_delete AFTER DELETE ON message_labels
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_labels_insert AFTER INSERT ON labels
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_labels_update AFTER UPDATE ON labels
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_labels_delete AFTER DELETE ON labels
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_bodies_insert AFTER INSERT ON bodies
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_bodies_update AFTER UPDATE ON bodies
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_bodies_delete AFTER DELETE ON bodies
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_contacts_insert AFTER INSERT ON contacts
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_contacts_update AFTER UPDATE ON contacts
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_contacts_delete AFTER DELETE ON contacts
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_screener_decisions_insert AFTER INSERT ON screener_decisions
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_screener_decisions_update AFTER UPDATE ON screener_decisions
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_screener_decisions_delete AFTER DELETE ON screener_decisions
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_arrivals_insert AFTER INSERT ON arrivals
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_arrivals_update AFTER UPDATE ON arrivals
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_arrivals_delete AFTER DELETE ON arrivals
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rules_insert AFTER INSERT ON rules
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rules_update AFTER UPDATE ON rules
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rules_delete AFTER DELETE ON rules
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rule_treatments_insert AFTER INSERT ON rule_treatments
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rule_treatments_update AFTER UPDATE ON rule_treatments
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_rule_treatments_delete AFTER DELETE ON rule_treatments
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_calendar_invites_insert AFTER INSERT ON calendar_invites
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_calendar_invites_update AFTER UPDATE ON calendar_invites
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_calendar_invites_delete AFTER DELETE ON calendar_invites
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_deliveries_insert AFTER INSERT ON deliveries
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_deliveries_update AFTER UPDATE ON deliveries
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_deliveries_delete AFTER DELETE ON deliveries
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_delivery_messages_insert AFTER INSERT ON delivery_messages
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_delivery_messages_update AFTER UPDATE ON delivery_messages
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS assessment_input_delivery_messages_delete AFTER DELETE ON delivery_messages
BEGIN UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1; END;
