use super::*;
use mxr_core::*;
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
