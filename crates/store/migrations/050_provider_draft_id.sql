-- Stable provider draft resource ID for a canonical local draft.
--
-- Gmail replaces the message nested inside a draft when drafts.update is
-- called, but keeps this draft resource ID stable. Retaining it lets a later
-- `mxr drafts push` update the same Gmail draft instead of creating a copy.

ALTER TABLE drafts ADD COLUMN provider_draft_id TEXT;
ALTER TABLE drafts ADD COLUMN provider_draft_revision TEXT;
