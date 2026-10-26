-- List views need the verdicts, not the potentially megabytes of source zones
-- in an asks explanation. Keep a small transactionally maintained projection.
CREATE INDEX IF NOT EXISTS idx_mail_assessments_home
ON mail_assessments(account_id, policy_version, json_extract(assessment_json, '$.home'), message_id)
WHERE policy_version NOT LIKE 'asks-%';

-- History's missing/stale-home test needs only the revision. Keep that
-- mailbox-wide lookup off the large authoritative JSON table pages.
CREATE INDEX IF NOT EXISTS idx_mail_assessments_revision
ON mail_assessments(message_id, policy_version, account_id, input_revision)
WHERE policy_version NOT LIKE 'asks-%';

CREATE TABLE IF NOT EXISTS ask_view_summaries (
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    policy_version TEXT NOT NULL,
    input_revision INTEGER NOT NULL,
    summary_json TEXT NOT NULL,
    PRIMARY KEY (message_id, policy_version),
    FOREIGN KEY (message_id, policy_version)
        REFERENCES mail_assessments(message_id, policy_version)
        ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE VIEW IF NOT EXISTS ask_view_summary_projection AS
SELECT message_id, policy_version, input_revision,
    json_object(
        'message_id', message_id,
        'input_fingerprint', input_fingerprint,
        'owner_authored', CASE json_extract(assessment_json, '$.facts.owner_authored')
            WHEN 1 THEN json('true') WHEN 0 THEN json('false') ELSE NULL END,
        'context_token', json_extract(assessment_json, '$.context_token'),
        'answers', json((SELECT json_group_array(json_extract(u.value, '$.answer'))
            FROM json_each(assessment_json, '$.units') u))
    ) AS summary_json
FROM mail_assessments WHERE policy_version LIKE 'asks-%';

INSERT INTO ask_view_summaries SELECT * FROM ask_view_summary_projection WHERE true
ON CONFLICT(message_id, policy_version) DO NOTHING;

CREATE TRIGGER IF NOT EXISTS ask_view_summary_insert AFTER INSERT ON mail_assessments
WHEN NEW.policy_version LIKE 'asks-%'
BEGIN
    INSERT INTO ask_view_summaries
    SELECT * FROM ask_view_summary_projection
    WHERE message_id = NEW.message_id AND policy_version = NEW.policy_version
    ON CONFLICT(message_id, policy_version) DO UPDATE SET
        input_revision = excluded.input_revision, summary_json = excluded.summary_json;
END;

CREATE TRIGGER IF NOT EXISTS ask_view_summary_update
AFTER UPDATE OF assessment_json, input_revision, input_fingerprint, policy_version ON mail_assessments
WHEN NEW.policy_version LIKE 'asks-%' OR OLD.policy_version LIKE 'asks-%'
BEGIN
    DELETE FROM ask_view_summaries
    WHERE message_id = NEW.message_id AND policy_version = NEW.policy_version
        AND NEW.policy_version NOT LIKE 'asks-%';
    INSERT INTO ask_view_summaries
    SELECT * FROM ask_view_summary_projection
    WHERE message_id = NEW.message_id AND policy_version = NEW.policy_version
    ON CONFLICT(message_id, policy_version) DO UPDATE SET
        input_revision = excluded.input_revision, summary_json = excluded.summary_json;
END;
