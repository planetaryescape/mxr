-- The paragraph a highlight was made in, so it marks that passage and not the
-- first place the same words appear. NULL for highlights saved before this
-- (and any read back without one): they keep marking the first match.
ALTER TABLE reading_highlights ADD COLUMN paragraph_index INTEGER;
