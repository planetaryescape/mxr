-- Retain only cleanup verifiers so stale starts cannot resurrect abandoned pairings.
CREATE TABLE IF NOT EXISTS remote_pairing_retirements (
    cleanup_verifier TEXT PRIMARY KEY
);
