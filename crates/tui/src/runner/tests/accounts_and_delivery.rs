use super::*;

#[test]
fn send_key_is_ignored_for_unchanged_draft_confirmation() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::Unchanged,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .map(|pending| pending.mode),
        Some(PendingSendMode::Unchanged)
    );
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn send_key_uses_pending_compose_account() {
    let mut app = App::new();
    let pending_account_id = AccountId::new();
    let other_account_id = AccountId::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].account_id = other_account_id;
    app.compose.pending_send_confirm = Some(PendingSend {
        draft_revision: None,
        account_id: pending_account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    match app
        .pending_mutation_queue
        .first()
        .map(|queued| &queued.request)
    {
        Some(Request::SendDraft { draft, .. }) => {
            assert_eq!(draft.account_id, pending_account_id);
        }
        other => panic!("Expected SendDraft request, got {other:?}"),
    }
}

#[test]
fn send_at_prompt_saves_draft_then_schedules_send() {
    let mut app = App::new();
    let pending_account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        draft_revision: None,
        account_id: pending_account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Scheduled hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/scheduled-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    for c in "in 2h".chars() {
        let _ = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(app.compose.pending_send_at_input.is_none());
    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].prelude.len(), 1);
    let draft_id = match &queue[0].prelude[0] {
        Request::SaveDraft { draft } => {
            assert_eq!(draft.account_id, pending_account_id);
            assert_eq!(draft.subject, "Scheduled hello");
            assert_eq!(
                draft.id,
                DraftId::from_provider_id("compose-file", "/tmp/scheduled-draft.md")
            );
            draft.id.clone()
        }
        other => panic!("Expected SaveDraft prelude, got {other:?}"),
    };
    match &queue[0].request {
        Request::ScheduleSend {
            managed_attachments_reviewed: true,
            draft_id: scheduled_id,
            send_at,
            ..
        } => {
            assert_eq!(scheduled_id, &draft_id);
            assert!(*send_at > chrono::Utc::now());
        }
        other => panic!("Expected ScheduleSend request, got {other:?}"),
    }
}

fn test_draft(subject: &str) -> Draft {
    let now = chrono::Utc::now();
    Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id: AccountId::new(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "alice@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: subject.into(),
        content: DraftContent::markdown("Body."),
        managed_attachments: None,
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    }
}

fn test_draft_account(
    account_id: AccountId,
    supports_server_drafts: bool,
) -> mxr_protocol::AccountSummaryData {
    let capabilities = mxr_protocol::AccountCapabilitiesData {
        supports_server_drafts,
        ..Default::default()
    };
    mxr_protocol::AccountSummaryData {
        identity: None,
        account_id,
        key: Some("work".into()),
        name: "Work".into(),
        email: "me@example.com".into(),
        provider_kind: "gmail".into(),
        sync_kind: Some("gmail".into()),
        send_kind: Some("gmail".into()),
        enabled: true,
        is_default: true,
        source: mxr_protocol::AccountSourceData::Runtime,
        editable: mxr_protocol::AccountEditModeData::RuntimeOnly,
        sync: None,
        send: None,
        capabilities,
    }
}

#[test]
fn stored_drafts_delete_previews_then_commits_the_captured_draft() {
    let mut app = App::new();
    let drafts = vec![test_draft("First"), test_draft("Second")];
    let first_id = drafts[0].id.clone();
    app.modals.drafts.open_loading();
    app.modals.drafts.set_drafts(drafts);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('#'), KeyModifiers::SHIFT));
    assert_eq!(action, Some(Action::StoredDraftsModalPreviewDelete));
    app.apply(action.unwrap());
    assert!(app.pending_draft_operation.is_none());

    app.modals.drafts.selected_index = 1;
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::StoredDraftsModalConfirm));
    app.apply(action.unwrap());

    assert!(matches!(
        app.pending_draft_operation,
        Some(crate::app::StoredDraftOperation::Delete { ref draft }) if draft.id == first_id
    ));
}

#[test]
fn the_draft_delete_confirm_opens_on_keep_so_a_bare_enter_deletes_nothing() {
    let open = || {
        let mut app = App::new();
        app.modals.drafts.open_loading();
        app.modals.drafts.set_drafts(vec![test_draft("First")]);
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('#'), KeyModifiers::SHIFT));
        app.apply(action.expect("# previews the delete"));
        app
    };
    let mut app = open();
    let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, Some(Action::StoredDraftsModalCancelConfirmation));
    app.apply(action.unwrap());
    assert!(app.pending_draft_operation.is_none());

    let mut app = open();
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        None
    );
    let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, Some(Action::StoredDraftsModalConfirm));
    app.apply(action.unwrap());
    assert!(matches!(
        app.pending_draft_operation,
        Some(crate::app::StoredDraftOperation::Delete { .. })
    ));
}

#[test]
fn stored_drafts_provider_copy_previews_then_queues_same_draft() {
    let mut app = App::new();
    let draft = test_draft("Copy me");
    let draft_id = draft.id.clone();
    app.accounts.page.accounts = vec![test_draft_account(draft.account_id.clone(), true)];
    app.modals.drafts.open_loading();
    app.modals.drafts.set_drafts(vec![draft]);

    app.apply(Action::StoredDraftsModalPreviewPush);
    assert!(matches!(
        app.modals.drafts.confirmation,
        Some(crate::app::StoredDraftOperation::Push { ref draft, ref provider })
            if draft.id == draft_id && provider == "gmail"
    ));

    app.apply(Action::StoredDraftsModalConfirm);
    assert!(matches!(
        app.pending_draft_operation,
        Some(crate::app::StoredDraftOperation::Push { ref draft, ref provider })
            if draft.id == draft_id && provider == "gmail"
    ));
}

#[test]
fn send_key_is_ignored_for_missing_recipient_draft_confirmation() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .map(|pending| pending.mode),
        Some(PendingSendMode::DraftOnlyNoRecipients)
    );
    assert!(app.pending_mutation_queue.is_empty());
}

fn missing_recipient_confirm(draft_path: std::path::PathBuf, account_id: AccountId) -> PendingSend {
    PendingSend {
        draft_revision: None,
        account_id,
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Hello".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "Body".into(),
        draft_path,
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    }
}

#[test]
fn escape_returns_to_the_draft_and_only_hash_discards_it() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-discard-draft-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "draft").unwrap();

    let mut app = App::new();
    let account_id = AccountId::new();
    app.compose.pending_send_confirm =
        Some(missing_recipient_confirm(temp.clone(), account_id.clone()));

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(temp.exists());
    assert!(app.compose.pending_draft_cleanup.is_empty());
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::EditDraft {
            path: temp.clone(),
            account_id: account_id.clone(),
        })
    );

    app.compose.pending_compose = None;
    app.compose.pending_send_confirm = Some(missing_recipient_confirm(temp.clone(), account_id));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('#'), KeyModifiers::SHIFT));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(app.compose.pending_compose.is_none());
    assert_eq!(app.compose.pending_draft_cleanup, vec![temp.clone()]);
    assert_eq!(app.status_message.as_deref(), Some("Discarded"));

    let _ = std::fs::remove_file(temp);
}

#[test]
fn diagnostics_recovery_keys_preview_and_confirm_the_same_workspace_owner() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;
    let paused_status = mxr_core::RecoveryStatusData {
        workspace_id: "workspace-a".into(),
        recovery_generation: "generation-2".into(),
        restore_pending: true,
        source_owner_stopped: false,
        retired_intentions: 1,
        retained_intentions: vec![],
        held_drafts: vec![],
    };
    app.diagnostics.page.recovery = Some(paused_status.clone());

    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        Some(Action::PreviewRecoveryActivation)
    );
    app.apply(Action::PreviewRecoveryActivation);
    let Some((
        Request::ActivateRecovery {
            expected_workspace,
            dry_run: true,
            ..
        },
        owner,
    )) = app.diagnostics.pending_recovery_request.take()
    else {
        panic!("activation preview must use the owner shown by status");
    };
    assert_eq!(expected_workspace, owner);
    app.apply_recovery_preview_result(
        owner.clone(),
        Request::ActivateRecovery {
            expected_workspace: owner.clone(),
            source_owner_stopped: true,
            dry_run: true,
        },
        mxr_core::RecoveryActionData {
            status: paused_status,
            prepared_draft: None,
            dry_run: true,
        },
    );
    assert!(matches!(
        app.diagnostics.page.recovery_preview.as_ref(),
        Some(crate::app::RecoveryPreview::Activation { owner: preview_owner })
            if preview_owner.workspace_id == owner.workspace_id
                && preview_owner.recovery_generation == owner.recovery_generation
    ));

    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE)),
        Some(Action::ConfirmRecoveryActivation)
    );
    app.apply(Action::ConfirmRecoveryActivation);
    let Some((
        Request::ActivateRecovery {
            expected_workspace,
            source_owner_stopped: true,
            dry_run: false,
        },
        confirm_owner,
    )) = app.diagnostics.pending_recovery_request.take()
    else {
        panic!("activation confirmation must explicitly acknowledge the stopped source owner");
    };
    assert_eq!(expected_workspace, owner);
    assert_eq!(confirm_owner, owner);

    let draft_id = DraftId::new();
    let account_id = AccountId::new();
    let activated_status = mxr_core::RecoveryStatusData {
        workspace_id: owner.workspace_id.clone(),
        recovery_generation: owner.recovery_generation.clone(),
        restore_pending: false,
        source_owner_stopped: true,
        retired_intentions: 1,
        retained_intentions: vec![],
        held_drafts: vec![mxr_core::RecoveredDraftData {
            draft_id: draft_id.clone(),
            account_id: account_id.clone(),
            revision: 7,
            subject: "Reviewed subject".into(),
            prepared_draft_id: None,
        }],
    };
    app.diagnostics.page.recovery_preview = None;
    app.apply_recovery_status_snapshot(Some(activated_status.clone()));
    let now = chrono::Utc::now();
    let prepared_draft = mxr_core::Draft {
        revision: Some(1),
        id: DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "Reviewed subject".into(),
        content: mxr_core::DraftContent::markdown("Reviewed body"),
        attachments: vec![],
        managed_attachments: None,
        inline_assets: vec![],
        created_at: now,
        updated_at: now,
        inline_calendar_reply: None,
    };
    app.apply_recovery_preview_result(
        owner.clone(),
        Request::PrepareRecoveredDraft {
            draft_id: draft_id.clone(),
            expected_revision: 7,
            expected_workspace: owner.clone(),
            dry_run: true,
        },
        mxr_core::RecoveryActionData {
            status: activated_status,
            prepared_draft: Some(prepared_draft),
            dry_run: true,
        },
    );
    let Some(crate::app::RecoveryPreview::Draft {
        owner: preview_owner,
        draft_id: preview_draft_id,
        revision,
        prepared_draft,
    }) = app.diagnostics.page.recovery_preview.as_ref()
    else {
        panic!("valid draft preview should remain available for confirmation");
    };
    assert_eq!(preview_owner.workspace_id, owner.workspace_id);
    assert_eq!(preview_owner.recovery_generation, owner.recovery_generation);
    assert_eq!(preview_draft_id, &draft_id);
    assert_eq!(*revision, 7);
    assert_eq!(prepared_draft.subject, "Reviewed subject");
    assert_eq!(
        prepared_draft.content.markdown_source(),
        Some("Reviewed body")
    );

    app.apply(Action::ConfirmRecoveredDraft);
    let Some((
        Request::PrepareRecoveredDraft {
            draft_id: confirmed_draft_id,
            expected_revision,
            expected_workspace,
            dry_run: false,
        },
        confirmed_owner,
    )) = app.diagnostics.pending_recovery_request.take()
    else {
        panic!("valid prepared-draft preview should confirm through the action path");
    };
    assert_eq!(confirmed_draft_id, draft_id);
    assert_eq!(expected_revision, 7);
    assert_eq!(expected_workspace.workspace_id, owner.workspace_id);
    assert_eq!(
        expected_workspace.recovery_generation,
        owner.recovery_generation
    );
    assert_eq!(confirmed_owner.workspace_id, owner.workspace_id);
    assert_eq!(
        confirmed_owner.recovery_generation,
        owner.recovery_generation
    );
}

#[test]
fn diagnostics_refuses_draft_confirmation_after_original_revision_changes() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;
    let owner = mxr_protocol::ArchiveWorkspaceData {
        workspace_id: "workspace-a".into(),
        recovery_generation: "generation-2".into(),
    };
    let draft_id = DraftId::new();
    let account_id = AccountId::new();
    app.diagnostics.page.recovery = Some(mxr_core::RecoveryStatusData {
        workspace_id: owner.workspace_id.clone(),
        recovery_generation: owner.recovery_generation.clone(),
        restore_pending: false,
        source_owner_stopped: true,
        retired_intentions: 1,
        retained_intentions: vec![],
        held_drafts: vec![mxr_core::RecoveredDraftData {
            draft_id: draft_id.clone(),
            account_id: account_id.clone(),
            revision: 8,
            subject: "Reviewed subject".into(),
            prepared_draft_id: None,
        }],
    });
    let now = chrono::Utc::now();
    app.diagnostics.page.recovery_preview = Some(crate::app::RecoveryPreview::Draft {
        owner,
        draft_id,
        revision: 7,
        prepared_draft: Box::new(mxr_core::Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id,
            from: None,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Reviewed subject".into(),
            content: mxr_core::DraftContent::markdown("Reviewed body"),
            attachments: vec![],
            managed_attachments: None,
            inline_assets: vec![],
            created_at: now,
            updated_at: now,
            inline_calendar_reply: None,
        }),
    });

    app.apply(Action::ConfirmRecoveredDraft);

    assert!(app.diagnostics.pending_recovery_request.is_none());
    assert!(app
        .status_message
        .as_deref()
        .unwrap_or_default()
        .contains("revision changed"));
}

#[test]
fn bulk_archive_requires_confirmation_before_queueing() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.selected_set = app
        .mailbox
        .envelopes
        .iter()
        .map(|env| env.id.clone())
        .collect();

    app.apply(Action::Archive);

    assert!(app.pending_mutation_queue.is_empty());
    match app.modals.pending_bulk_confirm.as_ref() {
        Some(confirm) => match &confirm.request {
            Request::Mutation {
                mutation: MutationCommand::Archive { message_ids },
                ..
            } => {
                assert_eq!(message_ids.len(), 3);
            }
            other => panic!("Expected Archive bulk request, got {other:?}"),
        },
        None => panic!("Expected pending bulk confirmation"),
    }
}

#[test]
fn snooze_action_opens_modal_then_queues_request() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::Snooze);
    assert!(app.modals.snooze_panel.visible);
    app.modals.snooze_panel.selected_index = 1;

    app.apply(Action::Snooze);
    assert!(!app.modals.snooze_panel.visible);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Snooze {
            message_id,
            wake_at,
        } => {
            assert_eq!(message_id, &app.mailbox.envelopes[0].id);
            assert!(*wake_at > chrono::Utc::now());
        }
        other => panic!("expected snooze request, got {other:?}"),
    }
}

fn pending_session() -> mxr_protocol::AuthSessionData {
    mxr_protocol::AuthSessionData {
        session_id: mxr_protocol::AuthSessionId("test-session".into()),
        state: mxr_protocol::AuthSessionStateData::WaitingForUser,
        flow: mxr_protocol::AuthFlowData::Device,
        account_key: "test-key".into(),
        auth_url: None,
        user_code: Some("ABC-DEF".into()),
        verification_uri: None,
        expires_at_unix: None,
        poll_interval_secs: Some(1),
        message: None,
        error: None,
    }
}

fn authorized_session() -> mxr_protocol::AuthSessionData {
    mxr_protocol::AuthSessionData {
        session_id: mxr_protocol::AuthSessionId("test-session".into()),
        state: mxr_protocol::AuthSessionStateData::Authorized,
        flow: mxr_protocol::AuthFlowData::Device,
        account_key: "test-key".into(),
        auth_url: None,
        user_code: None,
        verification_uri: None,
        expires_at_unix: None,
        poll_interval_secs: None,
        message: None,
        error: None,
    }
}

fn session_response(session: mxr_protocol::AuthSessionData) -> mxr_protocol::Response {
    mxr_protocol::Response::Ok {
        data: mxr_protocol::ResponseData::AuthSession { session },
    }
}

fn fake_ipc_channel() -> (
    tokio::sync::mpsc::UnboundedSender<crate::ipc::IpcRequest>,
    tokio::sync::mpsc::UnboundedReceiver<crate::ipc::IpcRequest>,
) {
    tokio::sync::mpsc::unbounded_channel()
}

#[tokio::test(start_paused = true)]
async fn auth_session_poller_retries_after_transient_error() {
    use crate::async_result::AsyncResult;
    use tokio::sync::mpsc;

    let (ipc_tx, mut ipc_rx) = fake_ipc_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    let account = mxr_protocol::AccountConfigData {
        color: None,
        tag: None,
        key: "test".into(),
        name: "Test".into(),
        email: "test@example.com".into(),
        enabled: true,
        sync: None,
        send: None,
        is_default: false,
    };

    let start_resp = session_response(pending_session());
    tokio::spawn(async move {
        let req = ipc_rx.recv().await.unwrap();
        let _ = req.reply.send(Ok(start_resp));
        tokio::time::advance(std::time::Duration::from_secs(2)).await;

        let req = ipc_rx.recv().await.unwrap();
        let _ = req
            .reply
            .send(Err(mxr_core::MxrError::Ipc("transient".into())));
        tokio::time::advance(std::time::Duration::from_secs(2)).await;

        let req = ipc_rx.recv().await.unwrap();
        let _ = req.reply.send(Ok(session_response(authorized_session())));
    });

    super::super::spawn_outlook_auth_session(&ipc_tx, result_tx, account, false).await;

    let mut got_auth_session = false;
    while let Ok(result) = result_rx.try_recv() {
        if matches!(result, AsyncResult::AuthSession(_)) {
            got_auth_session = true;
        }
    }
    assert!(
        got_auth_session,
        "poller must deliver AuthSession after recovering from one error"
    );
}

#[tokio::test(start_paused = true)]
async fn auth_session_poller_aborts_after_max_consecutive_failures() {
    use crate::async_result::AsyncResult;
    use tokio::sync::mpsc;

    let (ipc_tx, mut ipc_rx) = fake_ipc_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    let account = mxr_protocol::AccountConfigData {
        color: None,
        tag: None,
        key: "test".into(),
        name: "Test".into(),
        email: "test@example.com".into(),
        enabled: true,
        sync: None,
        send: None,
        is_default: false,
    };

    tokio::spawn(async move {
        let req = ipc_rx.recv().await.unwrap();
        let _ = req.reply.send(Ok(session_response(pending_session())));

        for _ in 0..5u32 {
            tokio::time::advance(std::time::Duration::from_secs(2)).await;
            let req = ipc_rx.recv().await.unwrap();
            let _ = req
                .reply
                .send(Err(mxr_core::MxrError::Ipc("persistent".into())));
        }
    });

    super::super::spawn_outlook_auth_session(&ipc_tx, result_tx, account, false).await;

    let mut got_account_op_err = false;
    while let Ok(result) = result_rx.try_recv() {
        if matches!(result, AsyncResult::AccountOperation(Err(_))) {
            got_account_op_err = true;
        }
    }
    assert!(
        got_account_op_err,
        "poller must deliver AccountOperation(Err) after 5 consecutive failures"
    );
}
