-- =========================================================================
-- Message pins: the exceptions in Reading and Paper trail.
--
-- A pinned message stays where it is when its bundle or place is swept.
-- Pins are local to this machine on purpose: a provider star syncs to every
-- client and means "important" everywhere, while a pin only means "keep
-- this one out of the sweep".
-- =========================================================================
CREATE TABLE IF NOT EXISTS message_pins (
    message_id  TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    pinned_at   INTEGER NOT NULL
);
