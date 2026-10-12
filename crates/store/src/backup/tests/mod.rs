use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
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
    assert_eq!(manifest.schema_version, 83);
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
    sqlx::raw_sql(
        "DROP TABLE remote_host; DROP TABLE remote_pairings; DROP TABLE remote_clients; DROP TABLE todo_resolution_queue; DELETE FROM schema_migrations WHERE version>=82;",
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

    manifest.schema_version = 84;
    std::fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(inspect(stage.path()).await.is_err());
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
    sqlx::raw_sql("DROP TABLE remote_host; DROP TABLE remote_pairings; DROP TABLE remote_clients; DELETE FROM schema_migrations WHERE version=83;").execute(&mut db).await.unwrap();
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
