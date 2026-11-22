use super::*;
use crate::app::ComposeAction;

async fn next_owned_request(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::ipc::IpcRequest>,
) -> crate::ipc::IpcRequest {
    loop {
        let request = rx.recv().await.unwrap();
        if matches!(request.request, Request::GetArchiveWorkspace) {
            request
                .reply
                .send(Ok(mxr_protocol::Response::Ok {
                    data: mxr_protocol::ResponseData::ArchiveWorkspace {
                        workspace: mxr_protocol::ArchiveWorkspaceData {
                            workspace_id: "fixture-workspace".into(),
                            recovery_generation: "fixture-generation".into(),
                        },
                    },
                }))
                .unwrap();
            continue;
        }
        if !matches!(request.request, Request::ResolveSendFrom { .. }) {
            assert_eq!(
                request.expected_workspace,
                Some(mxr_protocol::ArchiveWorkspaceData {
                    workspace_id: "fixture-workspace".into(),
                    recovery_generation: "fixture-generation".into(),
                })
            );
        }
        return request;
    }
}

#[tokio::test]
async fn compose_editor_success_opens_send_confirmation() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-editor-success-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let data = ComposeReadyData {
        draft_revision: None,
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();
    let (bg, mut bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
    let drain = tokio::spawn(async move {
        if let Some(req) = bg_rx.recv().await {
            let _ = req.reply.send(Err(MxrError::Ipc("test fixture".into())));
        }
    });

    handle_compose_editor_status(&mut app, &data, Ok(exit_status(0)), &bg).await;
    drop(bg);
    drain.await.ok();

    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .map(|pending| pending.mode),
        Some(PendingSendMode::SendOrSave)
    );
    assert!(app.status_message.is_none());

    let _ = std::fs::remove_file(temp);
}

#[test]
fn pressing_s_with_blocked_verdict_is_a_noop() {
    let mut app = App::new();
    let issue = mxr_core::DraftSafetyIssue::new(
        mxr_core::DraftSafetyIssueCode::PiiSecret,
        mxr_core::DraftSafetySeverity::Blocker,
        "secret",
    );
    let report = mxr_core::DraftSafetyReport::from_issues(vec![issue]);
    app.compose.pending_send_confirm = Some(PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: Some(report),
        safety_check_failed: None,
        override_token: Some("tok-1".into()),
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    });
    let mutations_before = app.pending_mutation_queue.len();

    let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
    let _ = app.handle_key(key);

    assert!(
        app.compose.pending_send_confirm.is_some(),
        "modal must stay open"
    );
    assert_eq!(
        app.pending_mutation_queue.len(),
        mutations_before,
        "no mutation queued"
    );
}

fn app_with_send_confirm(verdict_blocked: bool) -> App {
    let mut app = App::new();
    let report = if verdict_blocked {
        let issue = mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::PiiSecret,
            mxr_core::DraftSafetySeverity::Blocker,
            "secret",
        )
        .with_override_token("tok-override-9");
        mxr_core::DraftSafetyReport::from_issues(vec![issue])
    } else {
        mxr_core::DraftSafetyReport::from_issues(vec![])
    };
    app.compose.pending_send_confirm = Some(PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "alice@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
            forward_source: None,
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: Some(report),
        safety_check_failed: None,
        override_token: Some("tok-override-9".into()),
        suggested_collaborators: vec![],
        invite_reply: None,
        resolved_from: None,
        stored_draft: None,
    });

    app
}

#[test]
fn ctrl_o_dispatches_send_with_override_token() {
    let mut app = app_with_send_confirm(true);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert_override_dispatched(&app);
}

fn assert_override_dispatched(app: &App) {
    let queued = app.pending_mutation_queue.first().expect("mutation queued");
    match &queued.request {
        Request::SendDraft {
            override_safety_token,
            ..
        } => {
            assert_eq!(override_safety_token.as_deref(), Some("tok-override-9"));
        }
        other => panic!("expected SendDraft with override, got: {other:?}"),
    }
}

fn desk_row(lane: mxr_protocol::DeskLaneKind) -> mxr_protocol::DeskRowData {
    mxr_protocol::DeskRowData {
        lane,
        account_id: mxr_core::AccountId::new(),
        thread_id: mxr_core::ThreadId::new(),
        message_id: mxr_core::MessageId::new(),
        message_ids: vec![],
        counterparty_email: "maya@example.com".into(),
        counterparty_name: None,
        subject: "Launch plan".into(),
        reason: "replied to your message".into(),
        since: chrono::Utc::now(),
        age_seconds: 3_600,
        usual_seconds: None,
        usual_samples: 0,
        overdue: false,
        unread: true,
        starred: false,
        commitment_id: None,
        back_at: None,
    }
}

fn desk_with(rows: Vec<mxr_protocol::DeskRowData>) -> crate::app::DeskPageState {
    use mxr_protocol::{DeskLaneData, DeskLaneKind};
    let lane = |kind: DeskLaneKind| {
        let rows: Vec<_> = rows.iter().filter(|r| r.lane == kind).cloned().collect();
        (
            kind,
            DeskLaneData {
                total: rows.len() as u32,
                rows,
            },
        )
    };
    crate::app::DeskPageState {
        lanes: vec![
            lane(DeskLaneKind::Owed),
            lane(DeskLaneKind::Due),
            lane(DeskLaneKind::Waiting),
            lane(DeskLaneKind::PeopleNew),
        ],
        elsewhere: Default::default(),
        loaded: true,
        low_tide: false,
    }
}

#[test]
fn unsubscribe_opens_confirm_modal_and_scopes_archive_to_sender_and_account() {
    let mut app = App::new();
    let account_id = AccountId::new();
    let other_account_id = AccountId::new();
    let target = make_unsubscribe_envelope(
        account_id.clone(),
        "news@example.com",
        UnsubscribeMethod::HttpLink {
            url: "https://example.com/unsub".into(),
        },
    );
    let same_sender_same_account = make_unsubscribe_envelope(
        account_id.clone(),
        "news@example.com",
        UnsubscribeMethod::None,
    );
    let same_sender_other_account = make_unsubscribe_envelope(
        other_account_id,
        "news@example.com",
        UnsubscribeMethod::None,
    );
    let different_sender_same_account =
        make_unsubscribe_envelope(account_id, "other@example.com", UnsubscribeMethod::None);

    app.mailbox.envelopes = vec![target.clone()];
    app.mailbox.all_envelopes = vec![
        target.clone(),
        same_sender_same_account.clone(),
        same_sender_other_account,
        different_sender_same_account,
    ];

    app.apply(Action::Unsubscribe);

    let pending = app
        .modals
        .pending_unsubscribe_confirm
        .as_ref()
        .expect("unsubscribe modal should open");
    assert_eq!(pending.sender_email, "news@example.com");
    assert_eq!(pending.method_label, "browser link");
    assert_eq!(pending.archive_message_ids.len(), 2);
    assert!(pending.archive_message_ids.contains(&target.id));
    assert!(pending
        .archive_message_ids
        .contains(&same_sender_same_account.id));
}

fn requested(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .chain(
            app.modals
                .pending_bulk_confirm
                .as_ref()
                .map(|pending| pending.request.clone()),
        )
        .collect()
}

#[test]
fn trash_on_a_desk_row_targets_that_rows_messages() {
    use mxr_protocol::DeskLaneKind;
    let mut app = App::new();
    let unrelated = make_test_envelopes(1).remove(0);
    app.mailbox.envelopes = vec![unrelated.clone()];
    app.mailbox.viewing_envelope = Some(unrelated.clone());
    let mut owed = desk_row(DeskLaneKind::Owed);
    owed.message_ids = vec![mxr_core::MessageId::new(), owed.message_id.clone()];
    app.apply(Action::OpenDesk);
    app.set_desk(desk_with(vec![owed.clone()]));

    app.apply(Action::Trash);
    let queued = requested(&app);
    assert!(
        matches!(
            queued.as_slice(),
            [Request::Mutation { mutation: MutationCommand::Trash { message_ids }, .. }]
                if message_ids == &owed.message_ids && !message_ids.contains(&unrelated.id)
        ),
        "{queued:?}"
    );

    app.pending_mutation_queue.clear();
    app.modals.pending_bulk_confirm = None;
    let mut starred = owed.clone();
    starred.starred = true;
    app.set_desk(desk_with(vec![starred]));
    app.apply(Action::Star);
    assert!(
        matches!(
            requested(&app).as_slice(),
            [Request::Mutation { mutation: MutationCommand::Star { message_ids, starred: false }, .. }]
                if message_ids == &owed.message_ids
        ),
        "{:?}",
        requested(&app)
    );

    app.apply(Action::PatternSelect(crate::action::PatternKind::All));
    assert!(app.mailbox.selected_set.is_empty());

    app.pending_mutation_queue.clear();
    app.modals.pending_bulk_confirm = None;
    app.apply(Action::ApplyLabel);
    assert!(requested(&app).is_empty());
    assert!(app
        .status_message
        .as_deref()
        .unwrap_or("")
        .contains("Enter"));
}

fn recovery_pending(path: std::path::PathBuf) -> PendingSend {
    PendingSend {
        draft_revision: None,
        account_id: AccountId::new(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "recipient@example.com".into(),
            from: "sender@example.com".into(),
            subject: "Reviewed original".into(),
            ..Default::default()
        },
        body: "Original text".into(),
        draft_path: path,
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
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
fn send_failure_key_result_path_reopens_the_same_reviewed_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("review.md");
    std::fs::write(&file, "Original text").unwrap();
    let mut app = App::new();
    let pending = recovery_pending(file.clone());
    let draft_id = pending.draft_id();
    let account = pending.account_id.clone();
    app.compose.pending_send_confirm = Some(pending);
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let queued = app.pending_mutation_queue.pop().unwrap();
    assert!(app.compose.pending_send_confirm.is_none());
    assert!(matches!(&queued.request, Request::SendDraft { draft, .. } if draft.id == draft_id));
    app.finish_pending_mutation();
    app.handle_mutation_failure_result(
        queued.id,
        false,
        &mxr_core::MxrError::Ipc("Provider did not accept this operation".into()),
    );
    assert_eq!(
        app.modals.error.as_ref().unwrap().title,
        "Send outcome needs checking"
    );
    assert!(file.is_file());
    assert!(app.take_pending_draft_cleanup().is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .unwrap()
            .draft_id(),
        draft_id
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert_eq!(
        app.compose.pending_compose,
        Some(ComposeAction::EditDraft {
            path: file,
            account_id: account
        })
    );
    assert!(
        app.pending_mutation_queue.is_empty(),
        "failure never resubmits automatically"
    );
}

#[test]
fn older_send_failure_preserves_newer_compose_and_remains_reopenable() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new();
    let failed = recovery_pending(directory.path().join("A.md"));
    let failed_path = failed.draft_path.clone();
    app.compose.pending_send_confirm = Some(failed);
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let queued = app.pending_mutation_queue.pop().unwrap();
    let newer = recovery_pending(directory.path().join("B.md"));
    let newer_id = newer.draft_id();
    app.compose.pending_send_confirm = Some(newer);
    app.handle_mutation_failure_result(
        queued.id,
        false,
        &mxr_core::MxrError::Ipc("connection closed".into()),
    );
    assert_eq!(
        app.compose
            .pending_send_confirm
            .as_ref()
            .unwrap()
            .draft_id(),
        newer_id
    );
    assert_eq!(
        app.compose.failed_send.as_ref().unwrap().draft_path,
        failed_path
    );
    app.apply(Action::ReopenLastSendDraft);
    assert!(
        app.compose.pending_compose.is_none(),
        "cannot replace newer user work"
    );
    app.compose.pending_send_confirm = None;
    app.apply(Action::ReopenLastSendDraft);
    assert!(
        matches!(app.compose.pending_compose, Some(ComposeAction::EditDraft { path, .. }) if path == failed_path)
    );
}

#[test]
fn provider_save_and_schedule_keep_scratch_until_acknowledgement() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("review.md");
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(recovery_pending(file.clone()));
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    let queued = app.pending_mutation_queue.pop().unwrap();
    assert!(app.take_pending_draft_cleanup().is_empty());
    app.handle_mutation_failure_result(
        queued.id,
        false,
        &mxr_core::MxrError::Ipc("save refused".into()),
    );
    assert!(app.take_pending_draft_cleanup().is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    let queued = app.pending_mutation_queue.pop().unwrap();
    app.finish_compose_mutation(queued.id);
    assert_eq!(app.take_pending_draft_cleanup(), vec![file.clone()]);
    let pending = recovery_pending(file.clone());
    app.finish_compose_schedule(pending.clone(), false);
    assert!(app.compose.pending_send_confirm.is_some());
    assert!(app.take_pending_draft_cleanup().is_empty());
    app.finish_compose_schedule(pending, true);
    assert_eq!(app.take_pending_draft_cleanup(), vec![file]);
}

#[test]
fn last_send_outcome_uses_a_durable_modal_without_mutation_status() {
    let mut app = App::new();
    let account = AccountId::new();
    let identity = mxr_core::SendIdentity {
        operation_id: "retained-reviewed-operation".into(),
        expected_revision: 3,
    };
    app.compose.last_send_identity = Some((account.clone(), identity.clone()));
    app.apply(Action::CheckLastSendOutcome);
    assert!(app.modals.platform.visible);
    assert!(app.modals.platform.loading);
    assert_eq!(app.pending_mutation_count, 0);
    assert!(app.status_message.is_none());
    let queued = app.take_pending_platform_dispatch();
    assert_eq!(queued.len(), 1);
    assert!(
        matches!(&queued[0].request, mxr_protocol::Request::GetSendOutcome {
        account_id, operation_id
    } if *account_id == account && *operation_id == identity.operation_id)
    );

    let outcome = mxr_core::DraftSendOutcome {
        identity,
        draft_id: DraftId::new(),
        account_id: account,
        state: mxr_core::SendOutcomeState::Unknown,
        local_message_id: None,
        provider_message_id: None,
        rfc2822_message_id: None,
        recovery: None,
    };
    let body = super::super::format_platform_response(&mxr_protocol::ResponseData::SendOutcome {
        outcome: Some(outcome),
    });
    app.modals.platform.set_body(body);
    assert!(!app.modals.platform.loading);
    let rendered = mxr_test_support::render_to_string(80, 24, |frame| {
        crate::ui::platform_modal::draw(
            frame,
            ratatui::layout::Rect::new(0, 0, 80, 24),
            &app.modals.platform,
            &crate::theme::Theme::default(),
        );
    });
    assert!(
        rendered.contains("Acceptance uncertain. Do not resend."),
        "{rendered}"
    );
    assert!(
        rendered.contains("retained-reviewed-operation"),
        "{rendered}"
    );
    assert!(!rendered.contains("Checking send outcome"), "{rendered}");
}

#[tokio::test]
async fn alternate_compose_actions_block_unknown_or_accepted_before_persistence() {
    for state in [
        mxr_core::SendOutcomeState::Unknown,
        mxr_core::SendOutcomeState::Accepted,
        mxr_core::SendOutcomeState::Waiting,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("review.md");
        std::fs::write(&file, "review").unwrap();
        let mut app = App::new();
        app.compose.pending_send_confirm = Some(recovery_pending(file.clone()));
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        let queued = app.pending_mutation_queue.pop().unwrap();
        let Request::SaveDraftToServer { draft } = queued.request else {
            panic!("save")
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
        let account = draft.account_id.clone();
        let id = draft.id.clone();
        let server = tokio::spawn(async move {
            let discovery = rx.recv().await.unwrap();
            assert!(matches!(discovery.request, Request::GetArchiveWorkspace));
            discovery
                .reply
                .send(Ok(mxr_protocol::Response::Ok {
                    data: mxr_protocol::ResponseData::ArchiveWorkspace {
                        workspace: mxr_protocol::ArchiveWorkspaceData {
                            workspace_id: "fixture-workspace".into(),
                            recovery_generation: "fixture-generation".into(),
                        },
                    },
                }))
                .unwrap();
            let q = next_owned_request(&mut rx).await;
            assert!(matches!(q.request, Request::GetSendOutcome { .. }));
            q.reply
                .send(Ok(mxr_protocol::Response::Ok {
                    data: mxr_protocol::ResponseData::SendOutcome {
                        outcome: Some(mxr_core::DraftSendOutcome {
                            identity: mxr_core::SendIdentity::for_revision(&id, 1),
                            draft_id: id,
                            account_id: account,
                            state,
                            local_message_id: None,
                            provider_message_id: None,
                            rfc2822_message_id: None,
                            recovery: None,
                        }),
                    },
                }))
                .unwrap();
            assert!(rx.recv().await.is_none());
        });
        let error = crate::compose_persistence::prepare_reviewed_compose(&tx, draft, &file)
            .await
            .unwrap_err();
        app.finish_pending_mutation();
        app.handle_mutation_failure_result(queued.id, false, &error);
        assert!(file.exists());
        assert!(app.take_pending_draft_cleanup().is_empty());
        assert!(app.compose.pending_send_confirm.is_some());
        drop(tx);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn stale_provider_save_keeps_edited_file_and_has_no_provider_effect() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("stale.md");
    std::fs::write(&file, "unsaved edit").unwrap();
    let mut app = App::new();
    let mut pending = recovery_pending(file.clone());
    pending.draft_revision = Some(2);
    app.compose.pending_send_confirm = Some(pending);
    app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    let queued = app.pending_mutation_queue.pop().unwrap();
    let Request::SaveDraftToServer { draft } = queued.request else {
        panic!("save")
    };
    assert_eq!(draft.revision, Some(2));
    let mut newer = draft.clone();
    newer.revision = Some(3);
    newer.content = mxr_core::DraftContent::markdown("another writer's text");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let q = next_owned_request(&mut rx).await;
            assert!(matches!(q.request, Request::GetSendOutcome { .. }));
            q.reply
                .send(Ok(mxr_protocol::Response::Ok {
                    data: mxr_protocol::ResponseData::SendOutcome { outcome: None },
                }))
                .unwrap();
        }
        let q = next_owned_request(&mut rx).await;
        assert!(matches!(q.request, Request::GetDraft { .. }));
        q.reply
            .send(Ok(mxr_protocol::Response::Ok {
                data: mxr_protocol::ResponseData::Draft { draft: newer },
            }))
            .unwrap();
        assert!(rx.recv().await.is_none());
    });
    let error = crate::compose_persistence::prepare_reviewed_compose(&tx, draft, &file)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("stale"));
    app.finish_pending_mutation();
    app.handle_mutation_failure_result(queued.id, false, &error);
    assert_eq!(std::fs::read_to_string(file).unwrap(), "unsaved edit");
    assert!(app.take_pending_draft_cleanup().is_empty());
    assert!(app.compose.pending_send_confirm.is_some());
    drop(tx);
    server.await.unwrap();
}

#[test]
fn retained_file_cannot_reopen_during_send_save_or_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    for key in ['s', 'd'] {
        let file = directory.path().join(format!("{key}.md"));
        std::fs::write(&file, "reviewed text").unwrap();
        let pending = recovery_pending(file.clone());
        let mut app = App::new();
        app.compose.pending_send_confirm = Some(pending.clone());
        app.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
        let queued = app.pending_mutation_queue.pop().unwrap();
        app.compose.failed_send = Some(pending);
        app.apply(Action::ReopenLastSendDraft);
        assert!(
            app.compose.pending_compose.is_none(),
            "same file is owned by submission"
        );
        assert!(app.compose.failed_send.is_some());
        app.finish_compose_mutation(queued.id);
        app.apply_mutation_completion(queued.effect, true);
        assert_eq!(app.take_pending_draft_cleanup(), vec![file.clone()]);
        app.apply(Action::ReopenLastSendDraft);
        assert!(
            app.compose.pending_compose.is_none(),
            "cleanup owns it after queue drain"
        );
        crate::local_io::handle_result(
            &mut app,
            crate::async_result::AsyncResult::DraftCleanup {
                path: file.clone(),
                result: Err("synthetic failure".into()),
            },
        );
        app.apply(Action::ReopenLastSendDraft);
        assert!(
            matches!(app.compose.pending_compose, Some(ComposeAction::EditDraft { path, .. }) if path == file)
        );
    }
}

#[test]
fn cleanup_submission_failure_releases_file_without_deleting_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.md");
    std::fs::write(&path, "retained text").unwrap();
    let mut app = App::new();
    app.compose.failed_send = Some(recovery_pending(path.clone()));
    app.schedule_draft_cleanup(path.clone());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    drop(rx);
    crate::local_io::submit_pending_work(&mut app, &tx);
    assert!(!app.compose_file_owned(&path));
    assert!(path.exists());
    app.apply(Action::ReopenLastSendDraft);
    assert!(
        matches!(app.compose.pending_compose,Some(ComposeAction::EditDraft{path:opened,..}) if opened==path)
    );
}

#[cfg(unix)]
fn stored_review() -> (PendingSend, mxr_core::Draft) {
    let stored = mxr_core::Draft {
        revision: Some(4),
        id: mxr_core::DraftId::new(),
        account_id: AccountId::new(),
        from: None,
        reply_headers: Some(mxr_core::types::ReplyHeaders {
            in_reply_to: "<source@example.com>".into(),
            references: vec![],
            thread_id: None,
        }),
        intent: mxr_core::DraftIntent::Reply,
        to: vec![mxr_core::types::Address {
            name: None,
            email: "pat@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Re: Renewal".into(),
        content: mxr_core::DraftContent::markdown("Thursday at ten works."),
        managed_attachments: Some(vec![]),
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let mut pending = recovery_pending(std::path::PathBuf::from("/tmp/mxr-stored-review.md"));
    pending.account_id = stored.account_id.clone();
    pending.draft_revision = stored.revision;
    pending.intent = stored.intent;
    pending.body = "Thursday at ten works.".into();
    pending.stored_draft = Some(Box::new(stored.clone()));
    (pending, stored)
}

#[test]
fn sending_a_resumed_draft_sends_that_stored_draft_as_saved() {
    let mut app = App::new();
    let (pending, stored) = stored_review();
    app.compose.pending_send_confirm = Some(pending);
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let queued = app.pending_mutation_queue.pop().expect("a send is queued");
    match &queued.request {
        Request::SendDraft {
            draft,
            send_identity: Some(identity),
            ..
        } => {
            assert_eq!(draft.id, stored.id);
            assert_eq!(draft.revision, Some(4));
            assert_eq!(draft.managed_attachments, Some(vec![]));
            assert_eq!(draft.content.analysis_text(), "Thursday at ten works.");
            assert_eq!(identity.expected_revision, 4);
        }
        other => panic!("expected SendDraft, got {other:?}"),
    }
}

#[test]
fn a_resumed_drafts_review_edits_it_again_and_never_discards_it() {
    let mut app = App::new();
    let (pending, stored) = stored_review();
    app.compose.pending_send_confirm = Some(pending);
    app.handle_key(KeyEvent::new(KeyCode::Char('#'), KeyModifiers::NONE));
    assert!(
        app.compose.pending_send_confirm.is_some(),
        "# leaves a saved draft alone"
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert_eq!(
        app.pending_draft_edit_request.map(|draft| draft.id),
        Some(stored.id)
    );
    assert!(app.compose.pending_compose.is_none());
}

#[test]
fn c_adds_the_suggested_person_to_cc_and_ctrl_a_says_where_it_went() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new();
    let mut pending = recovery_pending(directory.path().join("reply.md"));
    pending.suggested_collaborators = vec![mxr_protocol::SuggestedRecipientData {
        email: "ana@example.com".into(),
        display_name: None,
        reason: "often copied".into(),
        confidence: "high".into(),
        evidence_msg_ids: Vec::new(),
    }];
    app.compose.pending_send_confirm = Some(pending);
    let before = app.status_message.clone();
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(
        app.status_message.as_deref(),
        notice_while_announced(
            "send-review",
            "Ctrl+a",
            "Add the suggested person to Cc is now c",
            before.as_deref()
        )
    );
    assert_eq!(app.compose.pending_send_confirm.as_ref().unwrap().fm.cc, "");
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    let pending = app.compose.pending_send_confirm.as_ref().unwrap();
    assert_eq!(pending.fm.cc, "ana@example.com");
    assert!(pending.suggested_collaborators.is_empty());
}
