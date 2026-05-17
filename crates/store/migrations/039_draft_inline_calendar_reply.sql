-- Adds JSON column carrying the InlineCalendarReply payload for drafts that
-- represent the "respond with comment" compose path for an iCal invite. When
-- present, the outbound builder switches to the
-- multipart/alternative(text/plain + text/calendar; method=REPLY) MIME layout
-- and the daemon's post-send hook updates the local PARTSTAT on the source
-- message. NULL for all other drafts.
ALTER TABLE drafts ADD COLUMN inline_calendar_reply_json TEXT;
