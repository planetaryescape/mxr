CREATE TABLE IF NOT EXISTS remote_host (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    host_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    recovery_generation TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS remote_pairings (
    request_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    capability TEXT NOT NULL DEFAULT 'mail_read' CHECK(capability = 'mail_read'),
    challenge TEXT NOT NULL,
    mail_verifier TEXT NOT NULL,
    cleanup_verifier TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    approved INTEGER NOT NULL DEFAULT 0,
    failed_attempts INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS remote_clients (
    client_id TEXT PRIMARY KEY,
    pairing_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    capability TEXT NOT NULL DEFAULT 'mail_read' CHECK(capability = 'mail_read'),
    mail_verifier TEXT NOT NULL,
    cleanup_verifier TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER
);
