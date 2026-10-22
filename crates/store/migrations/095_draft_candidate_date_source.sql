-- Where a draft commitment candidate's by_when came from, so promoting it
-- after send carries the provenance (see 094). NULL while by_when is NULL.
ALTER TABLE draft_commitment_candidates ADD COLUMN by_when_source TEXT
    CHECK (by_when_source IS NULL OR by_when_source IN ('model_date', 'words', 'user'));
