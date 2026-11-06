CREATE TABLE IF NOT EXISTS remote_browser_codes (
    code_verifier TEXT PRIMARY KEY,
    parent_client_id TEXT NOT NULL REFERENCES remote_clients(client_id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS remote_browser_codes_parent ON remote_browser_codes(parent_client_id);
CREATE TABLE IF NOT EXISTS remote_browser_sessions (
    session_id TEXT PRIMARY KEY,
    mail_verifier TEXT NOT NULL,
    parent_client_id TEXT NOT NULL REFERENCES remote_clients(client_id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS remote_browser_sessions_parent ON remote_browser_sessions(parent_client_id);
