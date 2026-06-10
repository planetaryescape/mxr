//! Persistence for batch-mutation job history.
//!
//! The store stays protocol-agnostic: the daemon serializes its
//! `JobData` to JSON and hands it here as an opaque `data_json` blob,
//! alongside the scalar columns used for ordering and pruning. These are
//! internal-maintenance rows, so the queries are unchecked
//! (`sqlx::query(...)`) and need no `.sqlx` offline cache.

use sqlx::Row;

impl super::Store {
    /// Insert or replace a job row. `data_json` is the full serialized
    /// job; the scalar columns mirror fields inside it for cheap
    /// ordering and pruning.
    pub async fn upsert_mutation_job(
        &self,
        job_id: &str,
        kind: &str,
        status: &str,
        started_at: i64,
        finished_at: Option<i64>,
        data_json: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO mutation_jobs
                   (job_id, kind, status, started_at, finished_at, data_json)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)
               ON CONFLICT(job_id) DO UPDATE SET
                   kind = excluded.kind,
                   status = excluded.status,
                   started_at = excluded.started_at,
                   finished_at = excluded.finished_at,
                   data_json = excluded.data_json"#,
        )
        .bind(job_id)
        .bind(kind)
        .bind(status)
        .bind(started_at)
        .bind(finished_at)
        .bind(data_json)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Return the `data_json` of the most recent jobs, newest first,
    /// capped at `limit`.
    pub async fn list_mutation_jobs(&self, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT data_json FROM mutation_jobs
               ORDER BY started_at DESC, job_id DESC
               LIMIT ?1"#,
        )
        .bind(limit)
        .fetch_all(self.reader())
        .await?;
        Ok(rows.iter().map(|row| row.get::<String, _>(0)).collect())
    }

    /// Return the `data_json` for a single job, if present.
    pub async fn get_mutation_job(&self, job_id: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query(r#"SELECT data_json FROM mutation_jobs WHERE job_id = ?1"#)
            .bind(job_id)
            .fetch_optional(self.reader())
            .await?;
        Ok(row.map(|row| row.get::<String, _>(0)))
    }

    /// Keep only the newest `keep` jobs, deleting the rest. Bounds the
    /// table under heavy mutation traffic.
    pub async fn prune_mutation_jobs(&self, keep: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"DELETE FROM mutation_jobs
               WHERE job_id NOT IN (
                   SELECT job_id FROM mutation_jobs
                   ORDER BY started_at DESC, job_id DESC
                   LIMIT ?1
               )"#,
        )
        .bind(keep)
        .execute(self.writer())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[tokio::test]
    async fn mutation_jobs_round_trip_and_prune() {
        let store = Store::in_memory().await.unwrap();

        for i in 0..5 {
            store
                .upsert_mutation_job(
                    &format!("job-{i}"),
                    "archive",
                    "queued",
                    i64::from(i),
                    None,
                    &format!(r#"{{"job_id":"job-{i}"}}"#),
                )
                .await
                .unwrap();
        }

        // Newest first.
        let all = store.list_mutation_jobs(10).await.unwrap();
        assert_eq!(all.len(), 5);
        assert!(all[0].contains("job-4"));
        assert!(all[4].contains("job-0"));

        // Single fetch + update (status transition) replaces in place.
        store
            .upsert_mutation_job(
                "job-2",
                "archive",
                "completed",
                2,
                Some(99),
                r#"{"job_id":"job-2","status":"completed"}"#,
            )
            .await
            .unwrap();
        let one = store.get_mutation_job("job-2").await.unwrap().unwrap();
        assert!(one.contains("completed"));
        assert_eq!(store.list_mutation_jobs(10).await.unwrap().len(), 5);

        // Prune keeps only the newest 2.
        store.prune_mutation_jobs(2).await.unwrap();
        let kept = store.list_mutation_jobs(10).await.unwrap();
        assert_eq!(kept.len(), 2);
        assert!(kept[0].contains("job-4"));
        assert!(kept[1].contains("job-3"));

        assert!(store.get_mutation_job("missing").await.unwrap().is_none());
    }
}
