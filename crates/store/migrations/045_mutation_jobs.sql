-- Persist batch-mutation job history so `mxr jobs` / `mxr history` survive a
-- daemon restart. Previously jobs lived only in an in-memory Vec and were
-- lost on restart, leaving no record a job ever ran.
--
-- The full JobData is stored as a JSON blob in `data_json`; the scalar
-- columns exist for ordering, pruning, and cheap filtering without parsing
-- every row. The store layer is protocol-agnostic, so the daemon owns the
-- JobData <-> JSON serialization.
CREATE TABLE IF NOT EXISTS mutation_jobs (
    job_id      TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    status      TEXT NOT NULL,
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    data_json   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mutation_jobs_started_at
    ON mutation_jobs (started_at DESC);
