-- Native HTML draft bodies.
--
-- Forward-only and additive. Existing rows keep `body_markdown` and pick up
-- `content_kind = 'markdown'` from the column default, so they decode to
-- `DraftContent::Markdown` with no migration-time rewrite. Nothing reads or
-- rewrites an existing row here.
--
-- `body_html` holds the caller's document byte-for-byte; mxr never reformats
-- it. `body_text` is the caller-supplied `text/plain` alternative, NULL when
-- the outbound builder should generate one. `inline_assets` is a JSON array of
-- `{cid, path}` — paths only, matching how `attachments` already works, so
-- image bytes never enter the database or the IPC frame.

ALTER TABLE drafts ADD COLUMN body_html TEXT;
ALTER TABLE drafts ADD COLUMN body_text TEXT;
ALTER TABLE drafts ADD COLUMN inline_assets TEXT NOT NULL DEFAULT '[]';
ALTER TABLE drafts ADD COLUMN content_kind TEXT NOT NULL DEFAULT 'markdown'
    CHECK (content_kind IN ('markdown', 'html'));
