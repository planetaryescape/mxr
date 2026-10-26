use super::*;

#[tokio::test]
async fn portable_backup_preserves_outcomes_without_exporting_runnable_approvals() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let (_, draft_id) = fixture(source.path()).await;
    let store = Store::new(&source.path().join("mxr.db")).await.unwrap();
    let draft = store.get_draft(&draft_id).await.unwrap().unwrap();
    let identity = SendIdentity::for_draft(&draft);
    let (workspace_id, recovery_generation) = store.archive_workspace().await.unwrap();
    let mut approved = draft.clone();
    approved.content = DraftContent::markdown("APPROVAL_ONLY_CANARY_90371");
    let frozen = source
        .path()
        .join("mxr.draft-attachments/approved-sends/synthetic/snapshot/0/frozen.bin");
    let frozen_bytes = b"FROZEN_ONLY_CANARY_90371";
    std::fs::create_dir_all(frozen.parent().unwrap()).unwrap();
    std::fs::write(&frozen, frozen_bytes).unwrap();
    approved.attachments = vec![frozen.clone()];
    let approval = crate::SendApproval {
        draft: approved,
        from: Address {
            name: None,
            email: "synthetic@example.test".into(),
        },
        workspace_id,
        recovery_generation,
        stored_revision: 1,
        command_hash: "synthetic-reviewed-command".into(),
        files: vec![crate::ApprovedSendFile {
            path: frozen,
            length: frozen_bytes.len() as u64,
            sha256: Sha256::digest(frozen_bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        }],
        legacy_evidence_sha256: None,
    };
    assert!(store
        .claim_approved_send(&identity, &approval, "<approved@example.test>")
        .await
        .unwrap());
    store
        .record_send_recovery(
            &identity,
            "waiting",
            SendSubmissionPhase::Cooldown,
            Some(chrono::Utc::now() + chrono::Duration::minutes(15)),
            Some("provider_temporarily_unavailable"),
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .0
            .state,
        SendOutcomeState::Waiting
    );
    store.writer().close().await;

    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 103);
    assert!(manifest
        .exclusions
        .iter()
        .any(|item| item.contains("runnable send approvals")));
    assert!(!manifest
        .files
        .iter()
        .any(|file| file.path.contains("approved-sends")));
    let bytes = std::fs::read(stage.path().join("mxr.db")).unwrap();
    assert!(!bytes
        .windows(b"APPROVAL_ONLY_CANARY_90371".len())
        .any(|window| window == b"APPROVAL_ONLY_CANARY_90371"));
    inspect(stage.path()).await.unwrap();
    retire_restored(&stage.path().join("mxr.db")).await.unwrap();
    let restored = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    assert!(restored.is_restored_draft(&draft_id).await.unwrap());
    assert!(restored
        .due_send_recovery(chrono::Utc::now() + chrono::Duration::days(30))
        .await
        .unwrap()
        .is_empty());
    let retained = restored
        .get_send_outcome(&identity.operation_id)
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(retained.state, SendOutcomeState::Unknown);
    assert_eq!(
        retained.rfc2822_message_id.as_deref(),
        Some("<approved@example.test>")
    );
    assert!(restored
        .get_send_approval(&identity.operation_id)
        .await
        .unwrap()
        .is_none());
    assert!(!restored
        .claim_approved_send(&identity, &approval, "<approved@example.test>")
        .await
        .unwrap());
}
