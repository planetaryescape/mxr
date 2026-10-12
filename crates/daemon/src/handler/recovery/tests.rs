use super::*;
use mxr_core::*;
use mxr_protocol::Request;
async fn fixture() -> (
    Arc<AppState>,
    Arc<mxr_provider_fake::FakeProvider>,
    Draft,
    ArchiveWorkspaceData,
) {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let now = chrono::Utc::now();
    let draft = Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: state.default_account_id(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "synthetic@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "held original".into(),
        content: DraftContent::markdown("private synthetic body"),
        attachments: vec![],
        managed_attachments: Some(vec![]),
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    state.store.insert_draft(&draft).await.unwrap();
    sqlx::query("INSERT INTO restored_draft_holds VALUES(?,?,?,?,NULL)")
        .bind(draft.id.as_str())
        .bind(draft.account_id.as_str())
        .bind(1)
        .bind(&draft.subject)
        .execute(state.store.writer())
        .await
        .unwrap();
    sqlx::query("INSERT INTO restore_recovery VALUES(1,1,0)")
        .execute(state.store.writer())
        .await
        .unwrap();
    state
        .restore_pending
        .store(true, std::sync::atomic::Ordering::Release);
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await.unwrap();
    let owner = ArchiveWorkspaceData {
        workspace_id,
        recovery_generation,
    };
    (state, fake, draft, owner)
}
#[tokio::test]
async fn paused_internal_schedule_and_rules_do_not_consume_attempts_or_call_provider() {
    let (state, fake, draft, owner) = fixture().await;
    sqlx::query(
        "UPDATE drafts SET send_at=unixepoch(),scheduled_send_revision=revision WHERE id=?",
    )
    .bind(draft.id.as_str())
    .execute(state.store.writer())
    .await
    .unwrap();
    assert_eq!(
        crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
            .await
            .unwrap(),
        0
    );
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scheduled_send_attempts")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    assert!(state
        .store
        .get_scheduled_send_identity(&draft.id)
        .await
        .unwrap()
        .is_some());
    assert!(
        crate::loops::apply_rules_to_messages(&state, &draft.account_id, fake.as_ref(), &[])
            .await
            .is_err()
    );
    assert!(fake.sent_drafts().is_empty());
    assert!(fake.mutations().is_empty());
    assert!(activate(&state, &owner, false, false).await.is_err());
    assert!(activate(&state, &owner, false, true).await.is_ok());
    assert!(state.is_restore_pending());
    let preview = Request::ActivateRecovery {
        expected_workspace: owner.clone(),
        source_owner_stopped: false,
        dry_run: true,
    };
    assert!(matches!(
        super::super::classify_request(&preview),
        super::super::RequestClass::Read
    ));
    let commit = Request::ActivateRecovery {
        expected_workspace: owner,
        source_owner_stopped: true,
        dry_run: false,
    };
    assert!(matches!(
        super::super::classify_request(&commit),
        super::super::RequestClass::Admin
    ));
    state.request_shutdown();
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}
#[tokio::test]
async fn originals_stay_held_new_preparation_is_idempotent_and_preview_validates_bytes() {
    let (state, fake, mut draft, owner) = fixture().await;
    let bytes = b"immutable resource";
    use sha2::Digest;
    let metadata = DraftAttachment {
        id: DraftAttachmentId::new(),
        draft_id: draft.id.clone(),
        account_id: draft.account_id.clone(),
        filename: "safe.txt".into(),
        mime_type: "text/plain".into(),
        length_bytes: bytes.len() as u64,
        sha256: sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    };
    let path = state
        .store
        .draft_attachment_dir()
        .join(metadata.id.as_str())
        .join("final/safe.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"corrupt").unwrap();
    state
        .store
        .insert_draft_attachment(&metadata)
        .await
        .unwrap();
    state
        .store
        .finalize_draft_attachment(&metadata.id)
        .await
        .unwrap();
    draft.managed_attachments = Some(vec![metadata]);
    sqlx::query("UPDATE drafts SET managed_attachments=? WHERE id=?")
        .bind(serde_json::to_string(&draft.managed_attachments).unwrap())
        .bind(draft.id.as_str())
        .execute(state.store.writer())
        .await
        .unwrap();
    state.store.activate_recovery().await.unwrap();
    state
        .restore_pending
        .store(false, std::sync::atomic::Ordering::Release);
    assert!(prepare(&state, &draft.id, 1, &owner, true)
        .await
        .unwrap_err()
        .to_string()
        .contains("bytes changed"));
    assert!(prepare(&state, &draft.id, 1, &owner, false).await.is_err());
    assert!(state.store.recovery_status().await.unwrap().held_drafts[0]
        .prepared_draft_id
        .is_none());
    std::fs::write(&path, bytes).unwrap();
    let before = serde_json::to_value(state.store.get_draft(&draft.id).await.unwrap()).unwrap();
    let action = prepare(&state, &draft.id, 1, &owner, false).await.unwrap();
    let ResponseData::RecoveryAction { action } = action else {
        panic!("wrong recovery response")
    };
    let new = action.prepared_draft.unwrap();
    assert_ne!(new.id, draft.id);
    assert_eq!(
        before,
        serde_json::to_value(state.store.get_draft(&draft.id).await.unwrap()).unwrap()
    );
    let ResponseData::RecoveryAction { action } =
        prepare(&state, &draft.id, 1, &owner, false).await.unwrap()
    else {
        panic!("wrong retry response")
    };
    assert_eq!(action.prepared_draft.unwrap().id, new.id);
    assert!(super::super::mutations::send_stored_draft_reviewed(
        &state,
        &draft.id,
        Some(&SendIdentity::for_draft(&draft)),
        None
    )
    .await
    .is_err());
    let identity = SendIdentity::for_draft(&new);
    super::super::mutations::send_stored_draft_reviewed(&state, &new.id, Some(&identity), None)
        .await
        .unwrap();
    super::super::mutations::send_stored_draft_reviewed(&state, &new.id, Some(&identity), None)
        .await
        .unwrap();
    assert_eq!(fake.sent_drafts().len(), 1);
    let wrong = ArchiveWorkspaceData {
        workspace_id: owner.workspace_id,
        recovery_generation: uuid::Uuid::now_v7().to_string(),
    };
    assert!(prepare(&state, &draft.id, 1, &wrong, true).await.is_err());
    state.request_shutdown();
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
    let _ = std::fs::remove_dir_all(state.store.draft_attachment_dir());
}
#[tokio::test]
async fn held_provider_link_cannot_push_local_edits_or_delete_content_after_activation() {
    let (state, fake, mut draft, owner) = fixture().await;
    state
        .store
        .set_provider_draft_link(&draft.id, "deleted-upstream", Some("old-revision"))
        .await
        .unwrap();
    draft.content = DraftContent::markdown("locally edited while recovery is paused");
    super::super::mutations::update_draft(&state, &draft)
        .await
        .unwrap();
    let edited = state.store.get_draft(&draft.id).await.unwrap().unwrap();
    assert_eq!(edited.content, draft.content);
    assert!(fake.sent_drafts().is_empty());
    state.store.activate_recovery().await.unwrap();
    state
        .restore_pending
        .store(false, std::sync::atomic::Ordering::Release);
    assert_eq!(
        super::super::mutations::reconcile_provider_drafts(&state, &draft.account_id)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        state
            .store
            .get_draft(&draft.id)
            .await
            .unwrap()
            .unwrap()
            .content,
        edited.content
    );
    let preview = Request::PrepareRecoveredDraft {
        draft_id: draft.id.clone(),
        expected_revision: edited.revision.unwrap(),
        expected_workspace: owner,
        dry_run: true,
    };
    assert!(
        super::super::enforce_safety_policy(mxr_config::SafetyPolicy::ReadOnly, &preview).is_ok()
    );
    state.request_shutdown();
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}

#[tokio::test]
async fn preparation_interruption_rolls_back_publication_and_retry_returns_canonical_content() {
    let (state, _, mut original, owner) = fixture().await;
    state.store.activate_recovery().await.unwrap();
    state
        .restore_pending
        .store(false, std::sync::atomic::Ordering::Release);
    sqlx::query("CREATE TRIGGER interrupt_preparation BEFORE UPDATE OF prepared_draft_id ON restored_draft_holds BEGIN SELECT RAISE(ABORT, 'synthetic publication interruption'); END")
        .execute(state.store.writer()).await.unwrap();
    assert!(prepare(&state, &original.id, 1, &owner, false)
        .await
        .is_err());
    assert_eq!(
        state
            .store
            .list_drafts(&original.account_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(state.store.recovery_status().await.unwrap().held_drafts[0]
        .prepared_draft_id
        .is_none());
    sqlx::query("DROP TRIGGER interrupt_preparation")
        .execute(state.store.writer())
        .await
        .unwrap();
    let ResponseData::RecoveryAction { action } = prepare(&state, &original.id, 1, &owner, false)
        .await
        .unwrap()
    else {
        panic!("wrong response")
    };
    let canonical = action.prepared_draft.unwrap();
    assert_eq!(canonical.revision, Some(1));
    original.content = DraftContent::markdown("original edited after preparation");
    state.store.update_draft(&original).await.unwrap();
    let ResponseData::RecoveryAction { action } = prepare(&state, &original.id, 1, &owner, false)
        .await
        .unwrap()
    else {
        panic!("wrong response")
    };
    assert_eq!(
        serde_json::to_value(action.prepared_draft).unwrap(),
        serde_json::to_value(Some(canonical)).unwrap()
    );
    state.request_shutdown();
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}

#[tokio::test]
async fn paused_shutdown_dispatch_acknowledges_and_stops_runtime() {
    let (state, _, _, _) = fixture().await;
    let response = super::super::dispatch(
        &state,
        mxr_protocol::ClientKind::Cli,
        &mxr_transport::PeerInfo::local(),
        &Request::Shutdown,
    )
    .await;
    assert!(matches!(
        response,
        mxr_protocol::Response::Ok {
            data: ResponseData::Ack
        }
    ));
    assert!(state.shutdown_requested());
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}
