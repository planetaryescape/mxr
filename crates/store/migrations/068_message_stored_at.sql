-- =========================================================================
-- When mxr stored each message, so "when did mail last arrive?" does not
-- trust the sender's Date header. A header up to a day ahead of the clock
-- would otherwise stay the newest mail for hours (GetFreshness).
--
-- Set on insert only; rows from before this migration stay NULL and fall
-- back to their Date header.
-- =========================================================================
ALTER TABLE messages ADD COLUMN stored_at INTEGER;
