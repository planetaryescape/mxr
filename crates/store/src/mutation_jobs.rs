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

    /// Initialize the database identity once, including under concurrent first use.
    pub async fn archive_workspace(&self) -> Result<(String, String), sqlx::Error> {
        sqlx::query(
            "INSERT INTO archive_workspace VALUES (1, ?, ?) ON CONFLICT(singleton) DO NOTHING",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(uuid::Uuid::now_v7().to_string())
        .execute(self.writer())
        .await?;
        let row = sqlx::query(
            "SELECT workspace_id, recovery_generation FROM archive_workspace WHERE singleton = 1",
        )
        .fetch_one(self.reader())
        .await?;
        Ok((row.get(0), row.get(1)))
    }

    pub async fn read_archive_workspace(&self) -> Result<Option<(String, String)>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT workspace_id, recovery_generation FROM archive_workspace WHERE singleton = 1",
        )
        .fetch_optional(self.reader())
        .await?;
        Ok(row.map(|row| (row.get(0), row.get(1))))
    }

    /// A restored database must rotate its generation before accepting recovered work.
    pub async fn rotate_archive_generation(&self) -> Result<(), sqlx::Error> {
        self.archive_workspace().await?;
        sqlx::query("UPDATE archive_workspace SET recovery_generation = ? WHERE singleton = 1")
            .bind(uuid::Uuid::now_v7().to_string())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Insert an immutable review only if this caller's operation ID is unused.
    pub async fn insert_archive_job(
        &self,
        job_id: &str,
        started_at: i64,
        data_json: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("INSERT INTO mutation_jobs (job_id, kind, status, started_at, data_json) VALUES (?, 'recoverable_archive', 'reviewed', ?, ?) ON CONFLICT(job_id) DO NOTHING")
            .bind(job_id).bind(started_at).bind(data_json).execute(self.writer()).await?;
        Ok(result.rows_affected() == 1)
    }

    /// The expected JSON is the durable claim: competing submissions cannot overwrite it.
    pub async fn compare_exchange_archive_job(
        &self,
        job_id: &str,
        expected: &str,
        next: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE mutation_jobs SET data_json = ?, status = 'recoverable' WHERE job_id = ? AND kind = 'recoverable_archive' AND data_json = ?")
            .bind(next).bind(job_id).bind(expected).execute(self.writer()).await?;
        Ok(result.rows_affected() == 1)
    }

    /// Completion and its history event share a commit; only the CAS winner logs it.
    pub async fn compare_exchange_archive_job_with_event(
        &self,
        job_id: &str,
        expected: &str,
        next: &str,
        summary: &str,
        refs: crate::EventLogRefs<'_>,
        details: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let mut transaction = self.writer().begin().await?;
        let result = sqlx::query("UPDATE mutation_jobs SET data_json = ?, status = 'recoverable' WHERE job_id = ? AND kind = 'recoverable_archive' AND data_json = ?")
            .bind(next).bind(job_id).bind(expected).execute(&mut *transaction).await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        crate::event_log::insert_event_refs_on(
            &mut transaction,
            "info",
            "mutation",
            summary,
            refs,
            details,
        )
        .await?;
        transaction.commit().await?;
        Ok(true)
    }

    /// Return the `data_json` of the most recent jobs, newest first,
    /// capped at `limit`.
    pub async fn list_mutation_jobs(&self, limit: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT data_json FROM mutation_jobs WHERE kind != 'recoverable_archive'
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
               WHERE kind != 'recoverable_archive' AND status NOT IN ('queued', 'running') AND job_id NOT IN (
                   SELECT job_id FROM mutation_jobs
                   WHERE kind != 'recoverable_archive'
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
    async fn archive_completion_cas_logs_only_winner_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let store = Store::new(&path).await.unwrap();
        store
            .insert_archive_job("archive", 0, "review")
            .await
            .unwrap();
        let (one, two) = tokio::join!(
            store.compare_exchange_archive_job_with_event(
                "archive",
                "review",
                "completed-1",
                "Archived message",
                crate::EventLogRefs::default(),
                None
            ),
            store.compare_exchange_archive_job_with_event(
                "archive",
                "review",
                "completed-2",
                "Archived message",
                crate::EventLogRefs::default(),
                None
            ),
        );
        assert_ne!(one.unwrap(), two.unwrap());
        assert_eq!(
            store
                .list_events(100, None, Some("mutation"))
                .await
                .unwrap()
                .len(),
            1
        );
        drop(store);
        let reopened = Store::new(&path).await.unwrap();
        assert!(!reopened
            .compare_exchange_archive_job_with_event(
                "archive",
                "review",
                "completed-3",
                "Archived message",
                crate::EventLogRefs::default(),
                None
            )
            .await
            .unwrap());
        assert_eq!(
            reopened
                .list_events(100, None, Some("mutation"))
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn protected_archive_receipts_do_not_displace_ordinary_job_history() {
        let store = Store::in_memory().await.unwrap();
        for i in 0..3 {
            store
                .upsert_mutation_job(
                    &format!("job-{i}"),
                    "star",
                    "completed",
                    i,
                    None,
                    &format!("job-{i}"),
                )
                .await
                .unwrap();
            store
                .insert_archive_job(&format!("archive-{i}"), i + 100, "review")
                .await
                .unwrap();
        }
        store.prune_mutation_jobs(2).await.unwrap();
        assert_eq!(
            store.list_mutation_jobs(10).await.unwrap(),
            ["job-2", "job-1"]
        );
        for i in 0..3 {
            assert!(store
                .get_mutation_job(&format!("archive-{i}"))
                .await
                .unwrap()
                .is_some());
        }
    }

    #[tokio::test]
    async fn mutation_jobs_round_trip_and_prune() {
        let store = Store::in_memory().await.unwrap();

        for i in 0..5 {
            store
                .upsert_mutation_job(
                    &format!("job-{i}"),
                    "archive",
                    "completed",
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
    #[tokio::test]
    async fn archive_workspace_and_protected_receipts_survive_restart_and_pruning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.db");
        let store = Store::new(&path).await.unwrap();
        let identity = store.archive_workspace().await.unwrap();
        let (first, concurrent) = tokio::join!(
            store.insert_archive_job("archive-1", 0, "review"),
            store.insert_archive_job("archive-1", 0, "different")
        );
        assert_ne!(first.unwrap(), concurrent.unwrap());
        let retained = store.get_mutation_job("archive-1").await.unwrap().unwrap();
        assert!(!store
            .compare_exchange_archive_job("archive-1", "not-current", "bad")
            .await
            .unwrap());
        assert!(store
            .compare_exchange_archive_job("archive-1", &retained, "accepted")
            .await
            .unwrap());
        store
            .upsert_mutation_job("active-job", "archive", "running", 0, None, "active")
            .await
            .unwrap();
        store.prune_mutation_jobs(0).await.unwrap();
        assert_eq!(
            store
                .get_mutation_job("archive-1")
                .await
                .unwrap()
                .as_deref(),
            Some("accepted")
        );
        assert_eq!(
            store
                .get_mutation_job("active-job")
                .await
                .unwrap()
                .as_deref(),
            Some("active")
        );
        drop(store);
        let reopened = Store::new(&path).await.unwrap();
        assert_eq!(reopened.archive_workspace().await.unwrap(), identity);
        assert_eq!(
            reopened
                .get_mutation_job("archive-1")
                .await
                .unwrap()
                .as_deref(),
            Some("accepted")
        );
        reopened.rotate_archive_generation().await.unwrap();
        let restored = reopened.archive_workspace().await.unwrap();
        assert_eq!(restored.0, identity.0);
        assert_ne!(restored.1, identity.1);
    }
}
