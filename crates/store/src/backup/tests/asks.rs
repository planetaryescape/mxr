use super::*;

#[tokio::test]
async fn schema_99_round_trip_preserves_correction_undo_and_projection_retry() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    seed_resolution_queue(&store).await;
    let before = store.get_todo("backup-task").await.unwrap().unwrap();
    let mut edited = before.clone();
    edited.title = "Keep the corrected bill".into();
    edited.user_edited = true;
    let now = chrono::Utc::now();
    store
        .correct_todo(
            "restored-correction",
            "wrong_details",
            &before,
            &edited,
            false,
            now,
        )
        .await
        .unwrap();
    let message = before.source_message_id.as_ref().unwrap();
    let thread = before.thread_id.as_ref().unwrap();
    let revisions = store
        .assessment_thread_revisions(&before.account_id, std::slice::from_ref(thread))
        .await
        .unwrap();
    let revision = revisions[thread];
    let context = crate::asks_context_token().unwrap();
    sqlx::query("INSERT INTO asks_projection_outbox VALUES(?1, ?2, ?3, 'retry-fingerprint', ?4)")
        .bind(message.as_str())
        .bind(before.account_id.as_str())
        .bind(revision)
        .bind(&context)
        .execute(store.writer())
        .await
        .unwrap();
    sqlx::query("INSERT INTO asks_legacy_links VALUES('backup-task', 'protected-successor')")
        .execute(store.writer())
        .await
        .unwrap();
    store.writer().close().await;
    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 99);
    inspect(stage.path()).await.unwrap();
    retire_restored(&stage.path().join("mxr.db")).await.unwrap();
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    let ticket: (String, String, i64, String, String) = sqlx::query_as("SELECT message_id, account_id, input_revision, input_fingerprint, context_token FROM asks_projection_outbox")
        .fetch_one(restored.reader()).await.unwrap();
    assert_eq!(
        ticket,
        (
            message.to_string(),
            before.account_id.to_string(),
            revision,
            "retry-fingerprint".into(),
            context
        )
    );
    let links: Vec<String> =
        sqlx::query_scalar("SELECT dedup_key FROM asks_legacy_links WHERE todo_id = 'backup-task'")
            .fetch_all(restored.reader())
            .await
            .unwrap();
    assert_eq!(links, ["protected-successor"]);
    let reason: String =
        sqlx::query_scalar("SELECT reason FROM todo_corrections WHERE id = 'restored-correction'")
            .fetch_one(restored.reader())
            .await
            .unwrap();
    assert_eq!(reason, "wrong_details");
    let undone = restored
        .undo_todo_correction("backup-task", "restored-correction", false, now)
        .await
        .unwrap();
    assert_eq!(undone, before);
    restored.writer().close().await;
}

#[tokio::test]
async fn exact_schema_97_and_98_backups_upgrade_without_rewriting_closed_history() {
    for version in [97, 98] {
        let source = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        let store = Store::fresh_backup(&source.path().join("mxr.db"))
            .await
            .unwrap();
        seed_resolution_queue(&store).await;
        sqlx::query("UPDATE todos SET title='Keep edited closed history', user_edited=1, state='done', done_at=1 WHERE id='backup-task'")
            .execute(store.writer()).await.unwrap();
        let before = store.get_todo("backup-task").await.unwrap().unwrap();
        store.writer().close().await;
        let mut manifest = write_manifest(
            stage.path(),
            populate(source.path(), stage.path()).await.unwrap(),
        );
        let mut db = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(stage.path().join("mxr.db")),
        )
        .await
        .unwrap();
        sqlx::raw_sql("DROP TABLE asks_projection_outbox; DROP TABLE asks_legacy_links;")
            .execute(&mut db)
            .await
            .unwrap();
        if version == 97 {
            sqlx::query("DROP TABLE todo_corrections")
                .execute(&mut db)
                .await
                .unwrap();
        }
        sqlx::query("DELETE FROM schema_migrations WHERE version > ?")
            .bind(version)
            .execute(&mut db)
            .await
            .unwrap();
        db.close().await.unwrap();
        manifest.schema_version = version;
        *manifest
            .files
            .iter_mut()
            .find(|f| f.path == "mxr.db")
            .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
        std::fs::write(
            stage.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        inspect(stage.path()).await.unwrap();
        retire_restored(&stage.path().join("mxr.db")).await.unwrap();
        for _ in 0..2 {
            let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
            assert_eq!(
                restored.get_todo("backup-task").await.unwrap(),
                Some(before.clone())
            );
            let versions: Vec<i64> = sqlx::query_scalar(
                "SELECT version FROM schema_migrations WHERE version >= 98 ORDER BY version",
            )
            .fetch_all(restored.reader())
            .await
            .unwrap();
            assert_eq!(versions, [98, 99]);
            restored.reader().close().await;
            restored.writer().close().await;
        }
    }
}
