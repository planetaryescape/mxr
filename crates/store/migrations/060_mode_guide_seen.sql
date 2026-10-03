-- First-encounter cards each mode has retired on this profile (D118). The
-- daemon keeps it, so a card closed in the web app stays closed in the TUI.
CREATE TABLE IF NOT EXISTS mode_guide_seen (
    mode    TEXT PRIMARY KEY,
    seen_at INTEGER NOT NULL
);
