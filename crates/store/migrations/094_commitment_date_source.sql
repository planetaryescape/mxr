-- Where a commitment's by_when came from: model_date (a date a model wrote
-- before due words existed), words (resolved from the email's own words) or
-- user (typed by the user). NULL while by_when is NULL.
ALTER TABLE contact_commitments ADD COLUMN by_when_source TEXT
    CHECK (by_when_source IS NULL OR by_when_source IN ('model_date', 'words', 'user'));
