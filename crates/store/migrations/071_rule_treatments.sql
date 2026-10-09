-- Automatic sorting is separate from chronological personal corrections.
CREATE TABLE IF NOT EXISTS rule_treatments (
    message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    rule_id TEXT NOT NULL,
    rule_updated_at TEXT NOT NULL,
    treatment TEXT NOT NULL CHECK (treatment IN ('messages', 'updates', 'reading')),
    rule_name TEXT NOT NULL
);
