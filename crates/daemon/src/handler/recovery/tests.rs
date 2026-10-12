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

#[tokio::test]
async fn restored_cached_mail_is_visible_while_paused_and_after_activation_without_credentials() {
    let (state, fake, draft, _) = fixture().await;
    state.sync_engine.sync_account(fake.as_ref()).await.unwrap();
    let labels = state
        .store
        .list_labels_by_account(&draft.account_id)
        .await
        .unwrap();
    let inbox = labels
        .iter()
        .find(|label| label.provider_id == "INBOX")
        .unwrap()
        .id
        .clone();
    state
        .store
        .set_account_enabled(&draft.account_id, false)
        .await
        .unwrap();
    assert!(
        state.store.list_accounts().await.unwrap().is_empty(),
        "effect selection stays disabled"
    );
    for activated in [false, true] {
        if activated {
            state.store.activate_recovery().await.unwrap();
            state
                .restore_pending
                .store(false, std::sync::atomic::Ordering::Release);
        }
        let accounts = super::super::account_config::list_runtime_accounts(&state)
            .await
            .unwrap();
        assert!(accounts
            .iter()
            .any(|account| account.account_id == draft.account_id
                && !account.enabled
                && !account.capabilities.supports_send));
        let mut message = None;
        for (account_id, label_id) in [
            (None, None),
            (Some(draft.account_id.clone()), None),
            (None, Some(inbox.clone())),
        ] {
            let response = super::super::dispatch(
                &state,
                mxr_protocol::ClientKind::Cli,
                &mxr_transport::PeerInfo::local(),
                &Request::ListEnvelopes {
                    account_id,
                    label_id,
                    limit: 100,
                    offset: 0,
                },
            )
            .await;
            let mxr_protocol::Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } = response
            else {
                panic!("wrong envelopes response")
            };
            assert!(
                !envelopes.is_empty(),
                "cached envelopes disappeared after activation={activated}"
            );
            message = Some(envelopes[0].id.clone());
        }
        let response = super::super::dispatch(
            &state,
            mxr_protocol::ClientKind::Cli,
            &mxr_transport::PeerInfo::local(),
            &Request::GetBody {
                message_id: message.unwrap(),
            },
        )
        .await;
        let mxr_protocol::Response::Ok {
            data: ResponseData::Body { body },
        } = response
        else {
            panic!("cached body unavailable")
        };
        assert!(body
            .text_plain
            .as_ref()
            .is_some_and(|text| !text.is_empty()));
        for account_id in [None, Some(draft.account_id.clone())] {
            let response = super::super::dispatch(
                &state,
                mxr_protocol::ClientKind::Cli,
                &mxr_transport::PeerInfo::local(),
                &Request::Search {
                    query: "label:inbox".into(),
                    limit: 100,
                    offset: 0,
                    account_id,
                    mode: None,
                    sort: None,
                    explain: false,
                },
            )
            .await;
            let mxr_protocol::Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            } = response
            else {
                panic!("wrong search response")
            };
            assert!(
                !results.is_empty(),
                "cached search disappeared after activation={activated}"
            );
        }
        for request in [
            Request::Count {
                query: "label:inbox".into(),
                account_id: None,
                mode: None,
            },
            Request::ListSubscriptions {
                account_id: None,
                limit: 100,
            },
        ] {
            let response = super::super::dispatch(
                &state,
                mxr_protocol::ClientKind::Cli,
                &mxr_transport::PeerInfo::local(),
                &request,
            )
            .await;
            assert!(
                matches!(response, mxr_protocol::Response::Ok { .. }),
                "cached read failed: {response:?}"
            );
        }
        let freshness = super::super::dispatch(
            &state,
            mxr_protocol::ClientKind::Cli,
            &mxr_transport::PeerInfo::local(),
            &Request::GetFreshness {
                account_id: None,
                limit: None,
            },
        )
        .await;
        assert!(
            matches!(freshness, mxr_protocol::Response::Ok { .. }),
            "cached freshness failed: {freshness:?}"
        );
        assert!(state.store.list_accounts().await.unwrap().is_empty());
        assert!(
            !state
                .store
                .get_account(&draft.account_id)
                .await
                .unwrap()
                .unwrap()
                .enabled
        );
    }
    assert!(fake.sent_drafts().is_empty());
    assert!(fake.mutations().is_empty());
    // An ordinary disabled profile still hides local mail.
    sqlx::query("DELETE FROM restore_recovery")
        .execute(state.store.writer())
        .await
        .unwrap();
    assert!(state
        .store
        .list_cached_mail_accounts()
        .await
        .unwrap()
        .is_empty());
    let response = super::super::dispatch(
        &state,
        mxr_protocol::ClientKind::Cli,
        &mxr_transport::PeerInfo::local(),
        &Request::ListEnvelopes {
            account_id: None,
            label_id: None,
            limit: 100,
            offset: 0,
        },
    )
    .await;
    assert!(
        matches!(response,mxr_protocol::Response::Ok{data:ResponseData::Envelopes{envelopes}} if envelopes.is_empty())
    );
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}

#[test]
fn restored_default_account_selection_changes_only_local_selection() {
    let directory = tempfile::tempdir().unwrap();
    temp_env::with_var("MXR_CONFIG_DIR", Some(directory.path().as_os_str()), || {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (state, fake, draft, _) = fixture().await;
            state.sync_engine.sync_account(fake.as_ref()).await.unwrap();
            let mut account = state.store.get_account(&draft.account_id).await.unwrap().unwrap();
            account.enabled = false;
            state.store.insert_account(&account).await.unwrap();
            let key = &account.sync_backend.as_ref().unwrap().config_key;
            std::fs::write(directory.path().join("config.toml"),format!("[llm]\nenabled=false\n[search.semantic]\nenabled=false\nauto_download_models=false\n[accounts.{key}]\nname=\"Synthetic cached account\"\nemail=\"{}\"\nenabled=false\n[accounts.{key}.sync]\ntype=\"fake\"\n[accounts.{key}.send]\ntype=\"fake\"\n", account.email)).unwrap();
            let response = super::super::dispatch(&state,mxr_protocol::ClientKind::Cli,&mxr_transport::PeerInfo::local(),&Request::SetDefaultAccount{key:key.clone()}).await;
            assert!(matches!(response,mxr_protocol::Response::Ok{..}),"default selection failed: {response:?}");
            assert_eq!(state.config_snapshot().general.default_account.as_deref(),Some(key.as_str()));
            assert!(state.is_restore_pending());
            assert!(state.store.list_accounts().await.unwrap().is_empty());
            assert!(state.runtime_account_ids().is_empty());
            assert!(state.get_provider(Some(&account.id)).is_err());
            assert!(state.send_provider_for_account(&account.id).is_err());
            let response=super::super::dispatch(&state,mxr_protocol::ClientKind::Cli,&mxr_transport::PeerInfo::local(),&Request::ListEnvelopes{account_id:None,label_id:None,limit:100,offset:0}).await;
            assert!(matches!(response,mxr_protocol::Response::Ok{data:ResponseData::Envelopes{envelopes}} if !envelopes.is_empty()));
            assert!(fake.sent_drafts().is_empty());
            assert!(fake.mutations().is_empty());
            state.shutdown_runtime_tasks(std::time::Duration::from_secs(2)).await;
        });
    });
}

#[test]
fn confirmed_activation_retries_after_runtime_reload_failure() {
    const CHILD: &str = "MXR_RECOVERY_RELOAD_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "handler::recovery::tests::confirmed_activation_retries_after_runtime_reload_failure",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("MXR_CONFIG_DIR", &config)
            .env("MXR_DATA_DIR", root.path().join("data"))
            .env("HOME", root.path())
            .env("MXR_KEYCHAIN", "off")
            .env("MXR_ACTIVITY", "off")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("activation reload child timed out: {output:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "activation reload child failed: {output:?}"
        );
        return;
    }
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(activation_reload_child());
}

async fn activation_reload_child() {
    let (state, fake, draft, owner) = fixture().await;
    state
        .store
        .set_account_enabled(&draft.account_id, false)
        .await
        .unwrap();
    let account = state
        .store
        .get_account(&draft.account_id)
        .await
        .unwrap()
        .unwrap();
    let key = &account.sync_backend.as_ref().unwrap().config_key;
    let path = mxr_config::config_dir().join("config.toml");
    let valid = |interval| {
        format!(
            r#"[general]
sync_interval={interval}
[llm]
enabled=false
[search.semantic]
enabled=false
auto_download_models=false
[accounts.{key}]
name="Synthetic recovery"
email="{}"
enabled=false
[accounts.{key}.sync]
type="fake"
[accounts.{key}.send]
type="fake"
"#,
            account.email
        )
    };
    std::fs::write(&path, valid(777)).unwrap();
    state.reload_accounts_from_disk().await.unwrap();
    assert!(state.runtime_account_ids().is_empty());

    std::fs::write(&path, "[invalid configuration").unwrap();
    let error = activate(&state, &owner, true, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("activation is committed") && error.contains("reload failed"),
        "{error}"
    );
    let committed = state.store.recovery_status().await.unwrap();
    assert!(!committed.restore_pending && committed.source_owner_stopped);
    assert!(!state.is_restore_pending());
    assert_eq!(committed.recovery_generation, owner.recovery_generation);
    assert!(
        !state
            .store
            .get_account(&draft.account_id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    std::fs::write(&path, valid(999)).unwrap();
    activate(&state, &owner, false, true).await.unwrap();
    assert_eq!(
        state.config_snapshot().general.sync_interval,
        777,
        "preview does not reload"
    );
    assert!(activate(&state, &owner, false, false).await.is_err());
    let wrong = ArchiveWorkspaceData {
        workspace_id: owner.workspace_id.clone(),
        recovery_generation: uuid::Uuid::now_v7().to_string(),
    };
    assert!(activate(&state, &wrong, true, false).await.is_err());

    activate(&state, &owner, true, false).await.unwrap();
    assert_eq!(
        state.config_snapshot().general.sync_interval,
        999,
        "confirmed retry reloads corrected config"
    );
    assert!(state.runtime_account_ids().is_empty());
    assert!(
        !state
            .store
            .get_account(&draft.account_id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    assert!(fake.sent_drafts().is_empty());
    assert!(fake.mutations().is_empty());
    state
        .shutdown_runtime_tasks(std::time::Duration::from_secs(2))
        .await;
}
