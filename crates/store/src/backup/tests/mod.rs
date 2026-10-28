mod approved_sends;
mod asks;

use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::ByWhenSource;
use mxr_core::types::MessageDirection;
use mxr_core::*;

type QueueSnapshot = (String, String, String, String, String, i64, i64, i64, i64);
async fn fixture(root: &Path) -> (String, DraftId) {
    let store = Store::fresh_backup(&root.join("mxr.db")).await.unwrap();
    let account = crate::test_fixtures::test_account();
    store.insert_account(&account).await.unwrap();
    let now = chrono::Utc::now();
    let id = DraftId::new();
    let bytes = b"synthetic durable private attachment";
    let metadata = DraftAttachment {
        id: DraftAttachmentId::new(),
        draft_id: id.clone(),
        account_id: account.id.clone(),
        filename: "original.txt".into(),
        mime_type: "text/plain".into(),
        content_id: None,
        content_location: None,
        length_bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    };
    let path = root.join(format!(
        "mxr.draft-attachments/{}/final/original.txt",
        metadata.id
    ));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    let draft = Draft {
        id: id.clone(),
        account_id: account.id.clone(),
        revision: Some(1),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "useful restored draft".into(),
        content: DraftContent::markdown("private synthetic body"),
        attachments: vec![],
        managed_attachments: Some(vec![metadata.clone()]),
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    store.insert_draft(&draft).await.unwrap();
    store
        .set_provider_draft_link(&id, "old-upstream-draft", Some("old-upstream-revision"))
        .await
        .unwrap();
    store.insert_draft_attachment(&metadata).await.unwrap();
    store.finalize_draft_attachment(&metadata.id).await.unwrap();
    store.schedule_send(&id, now).await.unwrap();
    sqlx::query("INSERT INTO user_activity(ts,source,action,context_json) VALUES(1,'cli','search','S09_ACTIVITY_CANARY_982731')").execute(store.writer()).await.unwrap();
    sqlx::query("INSERT INTO saved_activity_filters VALUES('canary','S09_FILTER_CANARY_982731','{}',1,1,NULL)").execute(store.writer()).await.unwrap();
    sqlx::query(
        "INSERT INTO draft_safety_overrides VALUES('S09_OVERRIDE_CANARY_982731',NULL,'[]',1,NULL)",
    )
    .execute(store.writer())
    .await
    .unwrap();
    let (owner, _) = store.archive_workspace().await.unwrap();
    store.writer().close().await;
    (owner, id)
}

fn write_manifest(stage: &Path, mut manifest: BackupManifest) -> BackupManifest {
    std::fs::write(stage.join("settings.json"), b"{}").unwrap();
    manifest
        .files
        .push(file_metadata(stage, "settings.json").unwrap());
    std::fs::write(
        stage.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    manifest
}

async fn seed_resolution_queue(store: &Store) -> Vec<QueueSnapshot> {
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let now = chrono::Utc::now();
    let mut source = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    source.provider_id = "backup-source".into();
    store
        .upsert_envelope_with_direction(&source, MessageDirection::Inbound)
        .await
        .unwrap();
    sqlx::query("INSERT INTO todos (id, account_id, thread_id, source_message_id, source_date,
        kind, verb, title, origin, reason, rules_version, dedup_key, created_at, updated_at)
        VALUES ('backup-task', ?1, ?2, ?3, ?4, 'bill', 'pay', 'Pay bill', 'manual', 'test', 1, 'backup-task', ?4, ?4)")
        .bind(account.id.as_str())
        .bind(source.thread_id.as_str())
        .bind(source.id.as_str())
        .bind(source.date.timestamp())
        .execute(store.writer())
        .await
        .unwrap();

    let cases = [
        ("pending", 0_i64, 0_i64, 0_i64),
        ("reserved", 2, 0, 1),
        ("exhausted", 5, 0, 2),
        ("completed", 3, 1, 3),
    ];
    let mut expected = Vec::new();
    for (name, attempts, completed, offset) in cases {
        let mut message = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        message.provider_id = format!("backup-{name}");
        let message_id = store
            .upsert_envelope_with_direction(&message, MessageDirection::Inbound)
            .await
            .unwrap();
        let revision = format!("revision-{name}");
        let fingerprint = format!("fingerprint-{name}");
        let source_date = message.date.timestamp();
        let next_attempt_at = now.timestamp() + offset * 3600;
        sqlx::query("INSERT INTO todo_resolution_queue
            (account_id, todo_id, message_id, revision, input_fingerprint, source_date, attempts, next_attempt_at, completed)
            VALUES (?1, 'backup-task', ?2, ?3, ?4, ?5, ?6, ?7, ?8)")
            .bind(account.id.as_str())
            .bind(message_id.as_str())
            .bind(&revision)
            .bind(&fingerprint)
            .bind(source_date)
            .bind(attempts)
            .bind(next_attempt_at)
            .bind(completed)
            .execute(store.writer())
            .await
            .unwrap();
        expected.push((
            account.id.to_string(),
            "backup-task".to_owned(),
            message_id.to_string(),
            revision,
            fingerprint,
            source_date,
            attempts,
            next_attempt_at,
            completed,
        ));
    }
    expected
}
#[tokio::test]
async fn logical_bundle_omits_physical_activity_and_tokens_and_restores_held_bytes() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let (owner, id) = fixture(source.path()).await;
    let before = std::fs::read(source.path().join("mxr.db")).unwrap();
    let mut manifest = populate(source.path(), stage.path()).await.unwrap();
    std::fs::write(stage.path().join("settings.json"), b"{}").unwrap();
    manifest
        .files
        .push(file_metadata(stage.path(), "settings.json").unwrap());
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();
    for entry in &manifest.files {
        let bytes = std::fs::read(stage.path().join(&entry.path)).unwrap();
        for canary in [
            "S09_ACTIVITY_CANARY_982731",
            "S09_FILTER_CANARY_982731",
            "S09_OVERRIDE_CANARY_982731",
        ] {
            assert!(
                !bytes.windows(canary.len()).any(|w| w == canary.as_bytes()),
                "{} leaked {}",
                entry.path,
                canary
            );
        }
    }
    assert_eq!(before, std::fs::read(source.path().join("mxr.db")).unwrap());
    retire_restored(&stage.path().join("mxr.db")).await.unwrap();
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(stage.path().join("mxr.db")),
    )
    .await
    .unwrap();
    let restored: (String, String) =
        sqlx::query_as("SELECT workspace_id,recovery_generation FROM archive_workspace")
            .fetch_one(&mut db)
            .await
            .unwrap();
    assert_eq!(owner, restored.0);
    assert_ne!(manifest.recovery_generation, restored.1);
    let held: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM restored_draft_holds WHERE draft_id=?)")
            .bind(id.as_str())
            .fetch_one(&mut db)
            .await
            .unwrap();
    assert!(held);
    let scheduled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM drafts WHERE send_at IS NOT NULL")
            .fetch_one(&mut db)
            .await
            .unwrap();
    assert_eq!(scheduled, 0);
    let linked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM drafts WHERE provider_draft_id IS NOT NULL")
            .fetch_one(&mut db)
            .await
            .unwrap();
    assert_eq!(linked, 0);
    let link_evidence: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM restored_intentions WHERE kind='provider-draft-link'",
    )
    .fetch_one(&mut db)
    .await
    .unwrap();
    assert_eq!(link_evidence, 1);
    let pending: bool = sqlx::query_scalar("SELECT pending FROM restore_recovery")
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert!(pending);
    let original: String = sqlx::query_scalar(
        "SELECT outcome_reference FROM restored_intentions WHERE kind='scheduled-send'",
    )
    .fetch_one(&mut db)
    .await
    .unwrap();
    assert!(original.starts_with(&format!("scheduled:{id}:1:")));
    db.close().await.unwrap();
    for file in manifest
        .files
        .iter()
        .filter(|f| f.path.starts_with("mxr.draft-attachments/"))
    {
        assert_eq!(
            std::fs::read(stage.path().join(&file.path)).unwrap(),
            b"synthetic durable private attachment"
        );
    }
}

#[tokio::test]
async fn resolution_queue_round_trips_through_backup_and_restore() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let restored = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let expected = seed_resolution_queue(&store).await;
    store.writer().close().await;

    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 104);
    inspect(stage.path()).await.unwrap();
    std::fs::copy(stage.path().join("mxr.db"), restored.path().join("mxr.db")).unwrap();
    retire_restored(&restored.path().join("mxr.db"))
        .await
        .unwrap();

    let restored_store = Store::new(&restored.path().join("mxr.db")).await.unwrap();
    let actual: Vec<QueueSnapshot> = sqlx::query_as(
        "SELECT account_id, todo_id, message_id, revision, input_fingerprint, source_date, attempts, next_attempt_at, completed
         FROM todo_resolution_queue ORDER BY revision",
    )
    .fetch_all(restored_store.reader())
    .await
    .unwrap();
    let mut expected = expected;
    expected.sort_by(|a, b| a.3.cmp(&b.3));
    assert_eq!(actual, expected);
    restored_store.writer().close().await;
}

#[tokio::test]
async fn unsubscribe_receipts_round_trip_through_backup_and_restore() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let restored = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mut envelope = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    envelope.provider_id = "backup-unsubscribed".into();
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    store
        .claim_unsubscribe(&envelope.id, &account.id, 1)
        .await
        .unwrap();
    store
        .confirm_unsubscribe(&envelope.id, "one_click", 2)
        .await
        .unwrap();
    store.writer().close().await;

    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    inspect(stage.path()).await.unwrap();
    std::fs::copy(stage.path().join("mxr.db"), restored.path().join("mxr.db")).unwrap();
    retire_restored(&restored.path().join("mxr.db"))
        .await
        .unwrap();

    assert_eq!(manifest.schema_version, 104);
    let restored_store = Store::new(&restored.path().join("mxr.db")).await.unwrap();
    assert!(
        restored_store
            .has_unsubscribe_receipt(&envelope.id)
            .await
            .unwrap(),
        "a restored backup must still refuse a second unsubscribe"
    );
    restored_store.writer().close().await;
}

#[tokio::test]
async fn inspect_accepts_exact_schema_81_and_rejects_future_schema() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(crate::backup::LEGACY_ACTIVITY_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DROP TABLE assistant_messages; DROP TABLE assistant_conversations; DROP TABLE reading_marginalia; DROP INDEX idx_reading_highlights_origin; ALTER TABLE reading_highlights DROP COLUMN paragraph_index; ALTER TABLE reading_highlights DROP COLUMN origin_id; ALTER TABLE reading_highlights DROP COLUMN origin; DROP TABLE remote_pairing_retirements; DROP TABLE remote_host; DROP TABLE remote_pairings; DROP TABLE remote_clients; DROP TABLE todo_resolution_queue; DELETE FROM schema_migrations WHERE version>=82;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 81;
    let database = file_metadata(stage.path(), "mxr.db").unwrap();
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = database;
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();

    manifest.schema_version = 85;
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
}

#[tokio::test]
async fn microsoft_link_baselines_and_uncertain_receipts_survive_backup_and_restore() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    seed_resolution_queue(&store).await;
    sqlx::query("INSERT INTO todos (id,account_id,kind,verb,title,origin,reason,rules_version,dedup_key,created_at,updated_at) SELECT 'uncertain-task',account_id,kind,verb,title,origin,reason,rules_version,'uncertain-task',created_at,updated_at FROM todos WHERE id='backup-task'")
        .execute(store.writer()).await.unwrap();
    let baseline = r#"{"title":"Pay bill","due":null,"scheduled":null,"completed":false}"#;
    let mut expected = Vec::new();
    for id in ["backup-task", "uncertain-task"] {
        let record = store.get_todo(id).await.unwrap().unwrap();
        assert!(store
            .claim_todo_microsoft_export(&record, baseline)
            .await
            .unwrap());
        if id == "backup-task" {
            store
                .accept_todo_microsoft_export(id, "remote-task", "operation", baseline)
                .await
                .unwrap();
            store
                .reserve_todo_microsoft_sync(id, Some(baseline))
                .await
                .unwrap();
            store
                .note_todo_microsoft_sync(id, None, Some("Unconfirmed write"))
                .await
                .unwrap();
        }
        expected.push(store.todo_microsoft_export(id).await.unwrap().unwrap());
    }
    store.writer().close().await;
    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 104);
    inspect(stage.path()).await.unwrap();
    retire_restored(&stage.path().join("mxr.db")).await.unwrap();
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    assert!(restored.restore_pending().await.unwrap());
    for receipt in expected {
        assert_eq!(
            restored
                .todo_microsoft_export(&receipt.todo_id)
                .await
                .unwrap(),
            Some(receipt)
        );
    }
    restored.writer().close().await;
}

#[tokio::test]
async fn exact_schema_90_backup_without_microsoft_links_remains_inspectable() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DELETE FROM schema_migrations WHERE version>=91;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 90;
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();
}

#[tokio::test]
async fn exact_schema_91_backup_without_the_thread_index_remains_inspectable() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DELETE FROM schema_migrations WHERE version>=92;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 91;
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();
}

#[tokio::test]
async fn exact_schema_92_backup_without_unsubscribe_receipts_remains_inspectable() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DELETE FROM schema_migrations WHERE version>=93;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 92;
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();
}

#[tokio::test]
async fn exact_schema_94_backup_without_draft_candidate_date_source_remains_inspectable() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DELETE FROM schema_migrations WHERE version>=95;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 94;
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();
}

#[tokio::test]
async fn exact_schema_93_backup_without_commitment_date_source_remains_inspectable() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DELETE FROM schema_migrations WHERE version>=94;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 93;
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    inspect(stage.path()).await.unwrap();
}

#[tokio::test]
async fn schema_92_source_backfills_receipts_from_its_event_log() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mut envelope = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    envelope.provider_id = "pre-093-unsubscribed".into();
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    // A mail whose subject contains the words; its mutation row must not count.
    let mut decoy = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    decoy.provider_id = "pre-093-decoy".into();
    store
        .upsert_envelope_with_direction(&decoy, MessageDirection::Inbound)
        .await
        .unwrap();
    for (timestamp, message, summary) in [
        (5, &envelope.id, "Sent unsubscribe request for \"News\""),
        (6, &decoy.id, "Marked \"How to unsubscribe\" as read"),
    ] {
        sqlx::query(
            "INSERT INTO event_log (timestamp, level, category, account_id, message_id, summary)
             VALUES (?, 'info', 'mutation', ?, ?, ?)",
        )
        .bind(timestamp)
        .bind(account.id.as_str())
        .bind(message.as_str())
        .bind(summary)
        .execute(store.writer())
        .await
        .unwrap();
    }
    store.writer().close().await;

    // Make the source look like schema 92: no receipts table at all.
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(source.path().join("mxr.db")),
    )
    .await
    .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DELETE FROM schema_migrations WHERE version>=93;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();

    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 104);
    inspect(stage.path()).await.unwrap();

    let mut staged = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(stage.path().join("mxr.db")),
    )
    .await
    .unwrap();
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT message_id, method, sent_at FROM unsubscribe_receipts")
            .fetch_all(&mut staged)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![(envelope.id.as_str(), "unknown".to_string(), 5)],
        "the pre-093 unsubscribe must still block a second send, and a mail whose subject mentions unsubscribing must not"
    );
    staged.close().await.unwrap();
}

#[tokio::test]
async fn inspect_rejects_unrecognized_schema_objects_in_supported_bundle() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::query("CREATE TABLE unsupported_future_object (id INTEGER)")
        .execute(&mut db)
        .await
        .unwrap();
    db.close().await.unwrap();
    *manifest
        .files
        .iter_mut()
        .find(|file| file.path == "mxr.db")
        .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
}

#[tokio::test]
async fn bundle_rejects_missing_corrupt_extra_and_traversal_files() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    fixture(source.path()).await;
    let mut manifest = populate(source.path(), stage.path()).await.unwrap();
    std::fs::write(stage.path().join("settings.json"), b"{}").unwrap();
    manifest
        .files
        .push(file_metadata(stage.path(), "settings.json").unwrap());
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(stage.path().join("daemon-token"), b"forbidden").unwrap();
    assert!(inspect(stage.path()).await.is_err());
    std::fs::remove_file(stage.path().join("daemon-token")).unwrap();
    let resource = &manifest.files[0].path;
    let bytes = std::fs::read(stage.path().join(resource)).unwrap();
    std::fs::write(stage.path().join(resource), b"bad").unwrap();
    assert!(inspect(stage.path()).await.is_err());
    std::fs::remove_file(stage.path().join(resource)).unwrap();
    assert!(inspect(stage.path()).await.is_err());
    std::fs::write(stage.path().join(resource), bytes).unwrap();
    let original = manifest.clone();
    std::fs::remove_file(stage.path().join(&manifest.files[0].path)).unwrap();
    manifest.files.remove(0);
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        inspect(stage.path()).await.is_err(),
        "missing reference must fail even when manifest entry is removed"
    );
    manifest = original;
    manifest.files[0].path = "../outside".into();
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
}

#[test]
fn file_metadata_hashes_multiple_chunks_and_the_partial_tail() {
    let root = tempfile::tempdir().unwrap();
    let payload = vec![b'a'; 64 * 1024 * 3 + 123];
    std::fs::write(root.path().join("multi-chunk.bin"), &payload).unwrap();
    let metadata = file_metadata(root.path(), "multi-chunk.bin").unwrap();
    assert_eq!(metadata.length, 196731);
    assert_eq!(
        metadata.sha256,
        "b0613e943c8daf84a9091df6d629a481eb71f6089877e7bb60a47942fe1d95c7"
    );
    std::fs::write(root.path().join("empty.bin"), []).unwrap();
    let empty = file_metadata(root.path(), "empty.bin").unwrap();
    assert_eq!(empty.length, 0);
    assert_eq!(
        empty.sha256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[tokio::test]
async fn portable_legacy_resource_keeps_the_multichunk_content_digest() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let (_, draft) = fixture(source.path()).await;
    let original = source.path().join("multi-chunk.bin");
    let payload = vec![b'a'; 64 * 1024 * 3 + 123];
    std::fs::write(&original, &payload).unwrap();
    let expected = file_metadata(source.path(), "multi-chunk.bin").unwrap();
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(source.path().join("mxr.db")),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE drafts SET attachments=? WHERE id=?")
        .bind(serde_json::to_string(&vec![original]).unwrap())
        .bind(draft.as_str())
        .execute(&mut db)
        .await
        .unwrap();
    db.close().await.unwrap();
    let manifest = populate(source.path(), stage.path()).await.unwrap();
    let relative = format!("resources/cache/{}/multi-chunk.bin", expected.sha256);
    let copied = manifest
        .files
        .iter()
        .find(|file| file.path == relative)
        .unwrap();
    assert_eq!(copied.length, expected.length);
    assert_eq!(copied.sha256, expected.sha256);
    assert_eq!(std::fs::read(stage.path().join(relative)).unwrap(), payload);
}

#[tokio::test]
async fn portable_backup_excludes_and_restore_erases_remote_authority() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let mail = mxr_core::RemoteSecret::new("a".repeat(64));
    let cleanup = mxr_core::RemoteSecret::new("b".repeat(64));
    let hash = |secret: &mxr_core::RemoteSecret| {
        mxr_core::RemoteSecret::new(base16ct::lower::encode_string(&Sha256::digest(
            secret.expose().as_bytes(),
        )))
    };
    let p = store
        .start_remote_pairing(
            "fixture",
            &[mxr_core::RemoteCapability::MailRead],
            &hash(&mail),
            &hash(&cleanup),
            100,
        )
        .await
        .unwrap();
    store
        .approve_remote_pairing(&p.request_id, false, 101)
        .await
        .unwrap();
    let c = store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, 102)
        .await
        .unwrap();
    store.writer().close().await;
    write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    inspect(stage.path()).await.unwrap();
    let portable = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    assert!(portable.remote_clients().await.unwrap().is_empty());
    assert!(portable
        .remote_authority_status()
        .await
        .unwrap()
        .host_id
        .is_none());
    portable.writer().close().await;
    portable.reader().close().await;
    // Raw copies are not portable backups; retirement still explicitly erases their grants.
    retire_restored(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let restored = Store::new(&source.path().join("mxr.db")).await.unwrap();
    assert!(restored.remote_clients().await.unwrap().is_empty());
    assert!(restored
        .authenticate_remote_client(&c.client_id, &mail, 103)
        .await
        .is_err());
    assert!(restored.restore_pending().await.unwrap());
}

#[tokio::test]
async fn exact_schema_82_backup_remains_restorable() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
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
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(crate::backup::LEGACY_ACTIVITY_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DROP TABLE assistant_messages; DROP TABLE assistant_conversations; DROP TABLE reading_marginalia; DROP INDEX idx_reading_highlights_origin; ALTER TABLE reading_highlights DROP COLUMN paragraph_index; ALTER TABLE reading_highlights DROP COLUMN origin_id; ALTER TABLE reading_highlights DROP COLUMN origin; DROP TABLE remote_pairing_retirements; DROP TABLE remote_host; DROP TABLE remote_pairings; DROP TABLE remote_clients; DELETE FROM schema_migrations WHERE version>=83;").execute(&mut db).await.unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 82;
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
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    assert!(restored
        .remote_authority_status()
        .await
        .unwrap()
        .host_id
        .is_none());
    assert!(restored.restore_pending().await.unwrap());
}

#[tokio::test]
async fn exact_schema_83_and_84_backups_remain_restorable_before_migrations() {
    for schema_version in [83, 84] {
        let source = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        let store = Store::fresh_backup(&source.path().join("mxr.db"))
            .await
            .unwrap();
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
        sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
            .execute(&mut db)
            .await
            .unwrap();
        // Rebuild authority from the immutable migration, without today's added columns.
        sqlx::raw_sql(crate::backup::LEGACY_ACTIVITY_SCHEMA)
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::raw_sql("ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DROP TABLE assistant_messages; DROP TABLE assistant_conversations; DROP TABLE reading_marginalia; DROP INDEX idx_reading_highlights_origin; ALTER TABLE reading_highlights DROP COLUMN paragraph_index; ALTER TABLE reading_highlights DROP COLUMN origin_id; ALTER TABLE reading_highlights DROP COLUMN origin; DROP TABLE remote_pairing_retirements; DROP TABLE remote_clients; DROP TABLE remote_pairings; DROP TABLE remote_host;")
        .execute(&mut db).await.unwrap();
        sqlx::raw_sql(include_str!("../../../migrations/083_remote_authority.sql"))
            .execute(&mut db)
            .await
            .unwrap();
        if schema_version == 84 {
            sqlx::raw_sql(include_str!(
                "../../../migrations/084_remote_pairing_retirements.sql"
            ))
            .execute(&mut db)
            .await
            .unwrap();
        }
        sqlx::raw_sql("ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag;")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE version>?")
            .bind(schema_version)
            .execute(&mut db)
            .await
            .unwrap();
        for table in ["remote_pairings", "remote_clients"] {
            let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name='allow_send'"
            )))
            .fetch_one(&mut db)
            .await
            .unwrap();
            assert_eq!(count, 0, "historical schema {schema_version}");
        }
        db.close().await.unwrap();
        manifest.schema_version = schema_version;
        *manifest
            .files
            .iter_mut()
            .find(|file| file.path == "mxr.db")
            .unwrap() = file_metadata(stage.path(), "mxr.db").unwrap();
        std::fs::write(
            stage.path().join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        inspect(stage.path()).await.unwrap();

        // Restore retirement runs before Store::new applies current migrations.
        retire_restored(&stage.path().join("mxr.db")).await.unwrap();
        let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
        assert!(restored
            .remote_authority_status()
            .await
            .unwrap()
            .host_id
            .is_none());
        assert!(restored.restore_pending().await.unwrap());
    }
}

#[tokio::test]
async fn older_sources_back_up_their_highlights_as_made_in_mxr() {
    // 83 predates the remote send grant too; 85 is the schema just before Marginalia.
    for source_version in [83, 85] {
        let source = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        let store = Store::fresh_backup(&source.path().join("mxr.db"))
            .await
            .unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        store.writer().close().await;
        let mut db = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(source.path().join("mxr.db")),
        )
        .await
        .unwrap();
        sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::raw_sql(crate::backup::LEGACY_ACTIVITY_SCHEMA)
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::raw_sql(
            "ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DROP TABLE assistant_messages; DROP TABLE assistant_conversations; DROP TABLE reading_marginalia; DROP INDEX idx_reading_highlights_origin;
             ALTER TABLE reading_highlights DROP COLUMN paragraph_index;
             ALTER TABLE reading_highlights DROP COLUMN origin_id;
             ALTER TABLE reading_highlights DROP COLUMN origin;",
        )
        .execute(&mut db)
        .await
        .unwrap();
        sqlx::raw_sql("ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag;")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM schema_migrations WHERE version>?")
            .bind(source_version)
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO reading_highlights(id,account_id,message_id,idx,view,quote,note,created_at)
             VALUES('h1',?,?,0,'issue','kept quote',NULL,1)",
        )
        .bind(account.id.as_str())
        .bind(envelope.id.as_str())
        .execute(&mut db)
        .await
        .unwrap();
        db.close().await.unwrap();

        let manifest = populate(source.path(), stage.path()).await.unwrap();
        assert_eq!(manifest.schema_version, 104, "source {source_version}");
        let mut copied = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(stage.path().join("mxr.db"))
                .read_only(true),
        )
        .await
        .unwrap();
        let row: (String, String, Option<String>) =
            sqlx::query_as("SELECT quote, origin, origin_id FROM reading_highlights WHERE id='h1'")
                .fetch_one(&mut copied)
                .await
                .unwrap();
        assert_eq!(
            row,
            ("kept quote".to_string(), "mxr".to_string(), None),
            "source {source_version}"
        );
    }
}

#[tokio::test]
async fn exact_schema_85_backup_without_marginalia_is_inspectable_and_86_is_not_reverted() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    store.writer().close().await;
    let mut manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    // Written at the current schema: the bundle keeps Marginalia and the paragraph and inspects as-is.
    assert_eq!(manifest.schema_version, 104);
    inspect(stage.path()).await.unwrap();

    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(stage.path().join("mxr.db")),
    )
    .await
    .unwrap();
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(crate::backup::LEGACY_ACTIVITY_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DROP TABLE assistant_messages; DROP TABLE assistant_conversations; DROP TABLE reading_marginalia; DROP INDEX idx_reading_highlights_origin;
         ALTER TABLE reading_highlights DROP COLUMN paragraph_index;
         ALTER TABLE reading_highlights DROP COLUMN origin_id;
         ALTER TABLE reading_highlights DROP COLUMN origin;
         DELETE FROM schema_migrations WHERE version>=86;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 85;
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

    // A bundle claiming 86 but missing the schema must not pass.
    manifest.schema_version = 86;
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
    manifest.schema_version = 91;
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
}

#[tokio::test]
async fn exact_schema_86_backup_without_paragraph_is_inspectable_and_87_is_not_reverted() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    store.writer().close().await;
    let mut manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 104);

    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(stage.path().join("mxr.db")),
    )
    .await
    .unwrap();
    sqlx::raw_sql(PRE_ASSESSMENT_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(crate::backup::LEGACY_ACTIVITY_SCHEMA)
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source; DROP TABLE unsubscribe_receipts; DROP INDEX idx_messages_account_thread_date; DROP TABLE todo_microsoft_exports; DROP TABLE assistant_messages; DROP TABLE assistant_conversations;
         ALTER TABLE reading_highlights DROP COLUMN paragraph_index;
         DELETE FROM schema_migrations WHERE version>=87;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    manifest.schema_version = 86;
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

    // A bundle claiming 87 but missing the column must not pass.
    manifest.schema_version = 87;
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
}

#[tokio::test]
async fn assistant_conversations_stay_out_of_backups() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    store
        .create_assistant_conversation("c1", "Bank mail")
        .await
        .unwrap();
    store
        .append_assistant_message("c1", "{\"role\":\"user\"}", None)
        .await
        .unwrap();
    store.writer().close().await;
    write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    inspect(stage.path()).await.unwrap();
    let mut copied = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(stage.path().join("mxr.db"))
            .read_only(true),
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM assistant_conversations) + (SELECT COUNT(*) FROM assistant_messages)",
    )
    .fetch_one(&mut copied)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn a_backup_from_before_95_marks_dated_draft_candidates_as_unverified() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    store.writer().close().await;
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(source.path().join("mxr.db")),
    )
    .await
    .unwrap();
    // Schema 94: no source on draft candidates.
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source;
         DELETE FROM schema_migrations WHERE version>=95;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    let draft = mxr_core::DraftId::new();
    for (what, by_when) in [("dated", Some(1000_i64)), ("undated", None)] {
        sqlx::query(
            "INSERT INTO draft_commitment_candidates (id, draft_id, account_id, email, direction,
                 who_owes, what, by_when, extracted_at)
             VALUES (?, ?, ?, 'alice@example.com', 'yours', 'me', ?, ?, 5)",
        )
        .bind(what)
        .bind(draft.as_str())
        .bind(account.id.as_str())
        .bind(what)
        .bind(by_when)
        .execute(&mut db)
        .await
        .unwrap();
    }
    db.close().await.unwrap();

    let manifest = populate(source.path(), stage.path()).await.unwrap();

    assert_eq!(manifest.schema_version, 104);
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    let rows = restored
        .list_draft_commitment_candidates(&draft)
        .await
        .unwrap();
    let source_of = |what: &str| {
        rows.iter()
            .find(|row| row.what == what)
            .map(|row| row.by_when_source)
    };
    assert_eq!(source_of("dated"), Some(Some(ByWhenSource::ModelDate)));
    assert_eq!(source_of("undated"), Some(None));
}

#[tokio::test]
async fn a_backup_from_before_94_backfills_where_each_commitment_date_came_from() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Outbound)
        .await
        .unwrap();
    store.writer().close().await;
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(source.path().join("mxr.db")),
    )
    .await
    .unwrap();
    // Schema 93: no provenance columns.
    sqlx::raw_sql(
        "ALTER TABLE accounts DROP COLUMN identity_color; ALTER TABLE accounts DROP COLUMN identity_tag; ALTER TABLE contact_commitments DROP COLUMN by_when_source;
         ALTER TABLE draft_commitment_candidates DROP COLUMN by_when_source;
         DELETE FROM schema_migrations WHERE version>=94;",
    )
    .execute(&mut db)
    .await
    .unwrap();
    for (id, by_when) in [
        ("commitment-detected".to_string(), Some(1000_i64)),
        ("commitment-undated".to_string(), None),
        (format!("{}::promise::0190", envelope.id), Some(2000)),
        (format!("{}::promise::undated", envelope.id), None),
    ] {
        sqlx::query(
            "INSERT INTO contact_commitments (id, account_id, email, thread_id, direction, status,
                 who_owes, what, by_when, evidence_msg_id, extracted_at)
             VALUES (?, ?, 'alice@example.com', ?, 'yours', 'open', 'me', ?, ?, ?, 5)",
        )
        .bind(&id)
        .bind(account.id.as_str())
        .bind(envelope.thread_id.as_str())
        .bind(&id)
        .bind(by_when)
        .bind(envelope.id.as_str())
        .execute(&mut db)
        .await
        .unwrap();
    }
    db.close().await.unwrap();

    let manifest = populate(source.path(), stage.path()).await.unwrap();

    assert_eq!(manifest.schema_version, 104);
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    let mut sources: Vec<(String, Option<ByWhenSource>)> = restored
        .list_contact_commitments(&account.id, None, None)
        .await
        .unwrap()
        .into_iter()
        .map(|record| (record.id, record.by_when_source))
        .collect();
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    let mut expected = vec![
        (
            "commitment-detected".to_string(),
            Some(ByWhenSource::ModelDate),
        ),
        ("commitment-undated".to_string(), None),
        (
            format!("{}::promise::0190", envelope.id),
            Some(ByWhenSource::User),
        ),
        (format!("{}::promise::undated", envelope.id), None),
    ];
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(sources, expected);
    let active = restored
        .list_contact_commitments(&account.id, None, Some(crate::CommitmentStatus::Open))
        .await
        .unwrap();
    assert_eq!(
        active.len(),
        2,
        "only explicit user promises remain active without stored asks"
    );
    assert!(active.iter().all(|row| row.id.contains("::promise::")));
}

#[tokio::test]
async fn stored_assessment_policy_history_survives_schema_97_backup_and_restore() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = Store::fresh_backup(&source.path().join("mxr.db"))
        .await
        .unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let message = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store
        .upsert_envelope_with_direction(&message, MessageDirection::Inbound)
        .await
        .unwrap();
    let facts = assessment::AssessmentFacts {
        sender_shape: "person-like address".into(),
        has_list_id: false,
        has_unsubscribe: false,
        auto_generated: false,
        bulk: false,
        templated_sender: false,
        written_to: false,
        body_cached: false,
        recipients: 1,
        addressed: true,
    };
    for policy in ["retained-policy", assessment::CLASSIFIER_POLICY_VERSION] {
        let value = assessment::MailAssessment {
            message_id: message.id.clone(),
            account_id: account.id.clone(),
            policy_version: policy.into(),
            home: "messages".into(),
            who: "person".into(),
            facts: facts.clone(),
            deciding: assessment::AssessmentClaim {
                rule: "fallback".into(),
                reason: "No rule decided; Messages for now".into(),
                source: "fallback".into(),
                evidence: vec![format!("message:{}", message.id)],
            },
            overridden: vec![],
            not_sure: Some("fallback".into()),
            fallback: true,
            pending_body: true,
            input_fingerprint: format!("fingerprint-{policy}"),
        };
        let revision = store.assessment_input_revision().await.unwrap();
        assert!(store.save_assessment(&value, revision).await.unwrap());
    }
    store.writer().close().await;
    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 104);
    inspect(stage.path()).await.unwrap();
    retire_restored(&stage.path().join("mxr.db")).await.unwrap();
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    assert_eq!(
        restored
            .assessment_versions(std::slice::from_ref(&account.id))
            .await
            .unwrap(),
        vec!["r2-1", "retained-policy"]
    );
    for policy in ["retained-policy", assessment::CLASSIFIER_POLICY_VERSION] {
        let rows = restored.assessments(&account.id, policy).await.unwrap();
        assert_eq!(
            rows[&message.id].input_fingerprint,
            format!("fingerprint-{policy}")
        );
    }
    assert!(restored.assessment_input_revision().await.unwrap() > 0);
    restored.writer().close().await;
}
