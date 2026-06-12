use super::*;

#[test]
fn input_j_moves_down() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
        Some(Action::MoveDown)
    );
}

#[test]
fn input_k_moves_up() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE)),
        Some(Action::MoveUp)
    );
}

#[test]
fn suspended_handoff_drops_old_event_source_before_running_action() {
    let old_dropped = Arc::new(AtomicBool::new(false));
    let new_created = Arc::new(AtomicBool::new(false));
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut terminal = 1usize;
    let mut events = Some(TestEventSource {
        id: 1,
        dropped: old_dropped.clone(),
    });

    let result = run_with_terminal_suspended_with(
        &mut terminal,
        &mut events,
        {
            let order = order.clone();
            move || order.lock().unwrap().push("restore")
        },
        {
            let order = order.clone();
            move || {
                order.lock().unwrap().push("init");
                2usize
            }
        },
        {
            let order = order.clone();
            let new_created = new_created.clone();
            move || {
                order.lock().unwrap().push("events");
                new_created.store(true, Ordering::SeqCst);
                TestEventSource {
                    id: 2,
                    dropped: Arc::new(AtomicBool::new(false)),
                }
            }
        },
        {
            let order = order.clone();
            let old_dropped = old_dropped.clone();
            let new_created = new_created.clone();
            move || {
                assert!(old_dropped.load(Ordering::SeqCst));
                assert!(!new_created.load(Ordering::SeqCst));
                order.lock().unwrap().push("run");
                "done"
            }
        },
    );

    assert_eq!(result, "done");
    assert_eq!(terminal, 2);
    assert_eq!(events.as_ref().map(|event| event.id), Some(2));
    assert_eq!(
        order.lock().unwrap().as_slice(),
        ["restore", "run", "init", "events"]
    );
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
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();
    let (bg, mut bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
    // Drain the safety-check IPC the new wiring fires; reply with an
    // error so the modal opens with `safety_report = None` (the
    // contract under test here is mode/state, not safety).
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

#[tokio::test]
async fn compose_editor_cancel_discards_draft() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-editor-cancel-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "---\n").unwrap();

    let data = ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();
    // Editor exited non-zero, so the safety-check path is never
    // taken; bg never receives a request.
    let (bg, _bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();

    handle_compose_editor_status(&mut app, &data, Ok(exit_status(1)), &bg).await;

    assert_eq!(app.status_message.as_deref(), Some("Draft discarded"));
    assert!(app.compose.pending_send_confirm.is_none());
    assert!(!temp.exists());
}

/// Slice 1.5 wiring contract (C2.1): the editor-finished handler
/// MUST fire `Request::CheckDraftSafety` before showing the modal,
/// and MUST stamp the response onto `pending_send_confirm`.
#[tokio::test]
async fn compose_editor_finish_stamps_safety_report_onto_pending() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-safety-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let data = ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    };
    let mut app = App::new();

    // Fake daemon: returns a Blocked report with a single PiiSecret
    // issue carrying override_token = Some("tok-test"). This is the
    // exact shape the daemon mints for blocker verdicts.
    let (bg, mut bg_rx) = mpsc::unbounded_channel::<crate::ipc::IpcRequest>();
    let fake_daemon = tokio::spawn(async move {
        let req = bg_rx.recv().await.expect("safety check IPC fired");
        // Verify the wiring sent a CheckDraftSafety, not some other
        // request.
        assert!(
            matches!(req.request, Request::CheckDraftSafety { .. }),
            "expected CheckDraftSafety, got: {:?}",
            req.request
        );
        let issue = mxr_core::DraftSafetyIssue::new(
            mxr_core::DraftSafetyIssueCode::PiiSecret,
            mxr_core::DraftSafetySeverity::Blocker,
            "secret pattern",
        )
        .with_override_token("tok-test");
        let report = mxr_core::DraftSafetyReport::from_issues(vec![issue]);
        let _ = req.reply.send(Ok(mxr_protocol::Response::Ok {
            data: mxr_protocol::ResponseData::DraftSafetyReportResponse { report },
        }));
    });

    handle_compose_editor_status(&mut app, &data, Ok(exit_status(0)), &bg).await;
    drop(bg);
    fake_daemon.await.unwrap();

    let pending = app
        .compose
        .pending_send_confirm
        .as_ref()
        .expect("modal should open");
    let report = pending
        .safety_report
        .as_ref()
        .expect("safety_report stamped onto pending");
    assert_eq!(report.verdict, mxr_core::DraftSafetyVerdict::Blocked);
    assert_eq!(pending.override_token.as_deref(), Some("tok-test"));
    assert!(pending.is_blocked());

    let _ = std::fs::remove_file(temp);
}

/// Slice 1.5 wiring contract (C2.1): pressing `[s] send` while
/// the safety verdict is Blocked is a no-op — the modal stays
/// open, no SendDraft mutation is queued. The user must use
/// `Ctrl-O` to override or edit the draft.
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
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: Some(report),
        override_token: Some("tok-1".into()),
        suggested_collaborators: vec![],
        invite_reply: None,
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

/// Slice 1.5 wiring contract (C2.1): Ctrl-O on a Blocked verdict
/// dispatches SendDraft with override_safety_token = the token
/// the daemon minted.
#[test]
fn ctrl_o_dispatches_send_with_override_token() {
    let mut app = App::new();
    let issue = mxr_core::DraftSafetyIssue::new(
        mxr_core::DraftSafetyIssueCode::PiiSecret,
        mxr_core::DraftSafetySeverity::Blocker,
        "secret",
    )
    .with_override_token("tok-override-9");
    let report = mxr_core::DraftSafetyReport::from_issues(vec![issue]);
    app.compose.pending_send_confirm = Some(PendingSend {
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
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: Some(report),
        override_token: Some("tok-override-9".into()),
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    let _ = app.handle_key(key);

    // The mutation queue must contain a SendDraft with the
    // override token.
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

/// Slice 2.3 wiring contract (C2.2): selecting the Owed sidebar
/// entry switches MailboxView to Owed AND requests a fresh
/// ListOwedReplies fetch.
#[test]
fn opening_owed_lens_switches_view_and_queues_refresh() {
    let mut app = App::new();
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Messages);
    assert!(!app.mailbox.pending_owed_refresh);

    app.apply(Action::OpenOwedReplies);

    assert_eq!(app.mailbox.mailbox_view, MailboxView::Owed);
    assert!(
        app.mailbox.pending_owed_refresh,
        "OpenOwedReplies must queue a refresh"
    );
}

/// Slice 2.3 wiring contract (C2.2): a successful SendDraft
/// mutation queues a ListOwedReplies refresh so a sent reply
/// disappears from the lens without manual intervention.
#[test]
fn sent_success_effect_triggers_owed_refresh() {
    let mut app = App::new();
    // Pretend the user is sitting on the owed lens.
    app.mailbox.mailbox_view = MailboxView::Owed;
    app.mailbox.pending_owed_refresh = false;

    // Apply the SentSuccess mutation completion directly. The
    // contract is: this branch sets pending_owed_refresh = true.
    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        true,
    );

    assert!(
        app.mailbox.pending_owed_refresh,
        "SentSuccess effect must trigger an owed refresh"
    );
}

/// Slice 5.1 wiring contract (C2.6): pressing Action::OpenThreadBriefing
/// when a thread is focused must open the modal in loading state AND
/// queue a pending briefing fetch.
#[test]
fn open_thread_briefing_action_opens_modal_and_queues_fetch() {
    let mut app = App::new();
    // Seed an envelope so context_envelope() returns something.
    let env = TestEnvelopeBuilder::new().build();
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::OpenSelected);

    app.apply(Action::OpenThreadBriefing);

    assert!(app.modals.briefing.visible, "briefing modal must open");
    assert!(app.modals.briefing.loading);
    assert!(matches!(
        app.modals.briefing.subject,
        Some(crate::app::BriefingModalSubject::Thread(_))
    ));
    assert!(
        matches!(
            app.pending_briefing_request,
            Some(crate::app::BriefingRequest::Thread(_))
        ),
        "pending request must be queued for the runtime to drain"
    );
}

/// Slice 5.1 (C2.6 cont): dormant_thread_hint returns Some when
/// the focused row's representative is >=30 days old AND the
/// thread has >=3 messages.
#[test]
fn dormant_hint_fires_at_30d_3msgs_and_nothing_below() {
    let mut app = App::new();
    // 31 days old + 3 messages -> dormant.
    let mut old = TestEnvelopeBuilder::new().build();
    old.date = chrono::Utc::now() - chrono::Duration::days(31);
    app.mailbox.envelopes = vec![old.clone()];
    app.mailbox.all_envelopes = vec![old.clone()];
    // Force the row to think there are 3 messages in the thread.
    app.apply(Action::OpenSelected);
    // Inject message_count by re-constructing the row via the
    // helper. The cleanest way is to overwrite the thread row
    // count through the existing aggregation; instead, we test
    // via a row count we control. Use 2 messages -> not dormant.
    let mut row = app.mail_list_rows().into_iter().next().unwrap();
    row.message_count = 2;
    assert!(
        row_to_dormant(&row, 31).is_none(),
        "2-message thread isn't dormant even if old"
    );
    row.message_count = 3;
    assert!(
        row_to_dormant(&row, 31).is_some(),
        "30d-old 3-message thread IS dormant"
    );
    row.message_count = 3;
    // Just below threshold.
    let mut fresh_row = row.clone();
    fresh_row.representative.date = chrono::Utc::now() - chrono::Duration::days(29);
    assert!(
        row_to_dormant(&fresh_row, 29).is_none(),
        "29d-old must NOT trigger the dormant hint"
    );
}

/// Slice 5.3 (C2.7 cont): Ctrl-A on the compose-confirm modal
/// adds the top "maybe include" suggestion to the draft's Cc
/// and removes it from the list.
#[test]
fn ctrl_a_adds_top_suggestion_to_cc() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
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
        },
        body: "hi".into(),
        draft_path: std::path::PathBuf::from("/tmp/draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        invite_reply: None,
        suggested_collaborators: vec![mxr_protocol::SuggestedRecipientData {
            email: "bob@example.com".into(),
            display_name: None,
            reason: "co-participant on 3 threads".into(),
            confidence: "medium".into(),
            evidence_msg_ids: vec![],
        }],
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));

    let pending = app.compose.pending_send_confirm.as_ref().unwrap();
    assert_eq!(pending.fm.cc, "bob@example.com");
    assert!(
        pending.suggested_collaborators.is_empty(),
        "consumed suggestion must be removed from the list"
    );
}

/// Slice 5.4 (C2.8 cont): FindExpertOnFocusedMessage opens
/// the expert modal in loading state and queues a query.
#[test]
fn find_expert_action_opens_modal_and_queues_query() {
    let mut app = App::new();
    let mut env = TestEnvelopeBuilder::new().build();
    env.subject = "kafka rebalance question".into();
    env.snippet = "how do consumers rebalance?".into();
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::OpenSelected);

    app.apply(Action::FindExpertOnFocusedMessage);

    assert!(app.modals.expert.visible);
    assert!(app.modals.expert.loading);
    let q = app.modals.expert.query.as_deref().unwrap_or("");
    assert!(q.contains("kafka rebalance"));
    assert!(app.pending_expert_query.is_some());
}

#[test]
fn close_expert_modal_clears_state() {
    let mut app = App::new();
    app.modals.expert.open_loading("kafka".into());
    assert!(app.modals.expert.visible);
    app.apply(Action::CloseExpertModal);
    assert!(!app.modals.expert.visible);
}

/// Slice 6.1 wiring contract (C2.9): pressing
/// OpenWhoisOnFocusedSender opens the whois modal in loading
/// state and queues a pending whois fetch with the focused
/// sender's email as the query.
#[test]
fn open_whois_action_seeds_modal_and_queues_query() {
    let mut app = App::new();
    let mut env = TestEnvelopeBuilder::new().build();
    env.from = mxr_core::Address {
        name: None,
        email: "carol@example.com".into(),
    };
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::OpenSelected);

    app.apply(Action::OpenWhoisOnFocusedSender);

    assert!(app.modals.whois.visible);
    assert!(app.modals.whois.loading);
    assert_eq!(app.modals.whois.query.as_deref(), Some("carol@example.com"));
    assert_eq!(
        app.pending_whois_query.as_deref(),
        Some("carol@example.com")
    );
}

/// Esc on the whois modal closes it.
#[test]
fn close_whois_modal_action_clears_state() {
    let mut app = App::new();
    app.modals.whois.open_loading("alice@example.com".into());
    assert!(app.modals.whois.visible);

    app.apply(Action::CloseWhoisModal);

    assert!(!app.modals.whois.visible);
    assert!(app.modals.whois.query.is_none());
}

/// Esc on the briefing modal closes it.
#[test]
fn close_briefing_modal_action_clears_state() {
    let mut app = App::new();
    app.modals
        .briefing
        .open_thread_loading(mxr_core::ThreadId::new());
    assert!(app.modals.briefing.visible);

    app.apply(Action::CloseBriefingModal);

    assert!(!app.modals.briefing.visible);
    assert!(app.modals.briefing.subject.is_none());
}

#[test]
fn suspended_handoff_preserves_non_compose_results() {
    let old_dropped = Arc::new(AtomicBool::new(false));
    let mut terminal = 1usize;
    let mut events = Some(TestEventSource {
        id: 1,
        dropped: old_dropped.clone(),
    });

    let result: Result<String, MxrError> = run_with_terminal_suspended_with(
        &mut terminal,
        &mut events,
        || {},
        || 2usize,
        || TestEventSource {
            id: 2,
            dropped: Arc::new(AtomicBool::new(false)),
        },
        || {
            assert!(old_dropped.load(Ordering::SeqCst));
            Ok("Log open cancelled".into())
        },
    );

    assert_eq!(result.unwrap(), "Log open cancelled");
    assert_eq!(terminal, 2);
    assert_eq!(events.as_ref().map(|event| event.id), Some(2));
}

#[test]
fn replaceable_request_queue_supersedes_older_status_refresh() {
    let mut pending = VecDeque::new();
    enqueue_replaceable_request(
        &mut pending,
        ReplaceableRequest::Status {
            request_id: 1,
            enqueued_at: Instant::now(),
        },
    );
    enqueue_replaceable_request(
        &mut pending,
        ReplaceableRequest::Status {
            request_id: 2,
            enqueued_at: Instant::now(),
        },
    );

    assert_eq!(pending.len(), 1);
    match pending.pop_front() {
        Some(ReplaceableRequest::Status { request_id, .. }) => assert_eq!(request_id, 2),
        other => panic!("expected status request, got {other:?}"),
    }
}

#[test]
fn input_gg_jumps_top() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        Some(Action::JumpTop)
    );
}

#[test]
fn input_zz_centers() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE)),
        Some(Action::CenterCurrent)
    );
}

#[test]
fn input_enter_opens() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenSelected)
    );
}

#[test]
fn input_o_opens() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
        Some(Action::OpenSelected)
    );
}

#[test]
fn input_escape_back() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Some(Action::Back)
    );
}

#[test]
fn input_q_quits() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        Some(Action::QuitView)
    );
}

#[test]
fn input_hml_viewport() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)),
        Some(Action::ViewportTop)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::SHIFT)),
        Some(Action::ViewportMiddle)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
        Some(Action::ViewportBottom)
    );
}

#[test]
fn input_uppercase_shortcuts_work_without_explicit_shift_modifier() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE)),
        Some(Action::ViewportTop)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE)),
        Some(Action::AttachmentList)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE)),
        Some(Action::OpenLogs)
    );
}

#[test]
fn input_ctrl_du_page() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        Some(Action::PageDown)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        Some(Action::PageUp)
    );
}

#[test]
fn app_move_down() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.apply(Action::MoveDown);
    assert_eq!(app.mailbox.selected_index, 1);
}

#[test]
fn app_move_up_at_zero() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.apply(Action::MoveUp);
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn app_jump_top() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(10);
    app.mailbox.selected_index = 5;
    app.apply(Action::JumpTop);
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn app_switch_pane() {
    let mut app = App::new();
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::Sidebar);
    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
}

#[test]
fn app_quit() {
    let mut app = App::new();
    app.apply(Action::QuitView);
    assert!(app.should_quit);
}

#[test]
fn app_new_uses_original_html_as_default_message_view() {
    let app = App::new();
    assert!(app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
}

#[test]
fn app_from_render_config_respects_text_reader_mode() {
    let config = RenderConfig {
        reader_mode: true,
        ..Default::default()
    };
    let app = App::from_render_config(&config);
    assert!(app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
}

#[test]
fn apply_runtime_config_updates_tui_settings() {
    let mut app = App::new();
    let mut config = mxr_config::MxrConfig::default();
    config.render.reader_mode = false;
    config.snooze.morning_hour = 7;
    config.appearance.theme = "light".into();

    app.apply_runtime_config(&config);

    assert!(!app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
    assert_eq!(app.modals.snooze_config.morning_hour, 7);
    assert_eq!(
        app.theme.selection_fg,
        crate::theme::Theme::light().selection_fg
    );
}

#[test]
fn edit_config_action_sets_pending_flag() {
    let mut app = App::new();

    app.apply(Action::EditConfig);

    assert!(app.diagnostics.pending_config_edit);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Opening config in editor...")
    );
}

#[test]
fn open_logs_action_sets_pending_flag() {
    let mut app = App::new();

    app.apply(Action::OpenLogs);

    assert!(app.diagnostics.pending_log_open);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Opening log file in editor...")
    );
}

#[test]
fn open_in_browser_action_queues_html_body_open() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Plain body".into()),
            text_html: Some("<p>Hello html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenInBrowser);

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("browser open should be queued");
    assert_eq!(pending.message_id, env.id);
    assert_eq!(pending.document, "<p>Hello html</p>");
    assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
}

#[test]
fn open_in_browser_action_wraps_plain_text_when_html_is_missing() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Plain body".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenInBrowser);

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("plain text should still open in browser");
    assert_eq!(pending.message_id, env.id);
    assert!(pending.document.contains("<pre>Plain body</pre>"));
    assert!(pending.document.contains("<!doctype html>"));
    assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
}

#[test]
fn open_in_browser_action_wraps_best_effort_fallback_body() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "invite.ics".into(),
                mime_type: "text/calendar".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Demo call".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenInBrowser);

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("best-effort fallback should open in browser");
    assert_eq!(pending.message_id, env.id);
    assert!(pending.document.contains("Calendar invite"));
    assert!(pending.document.contains("Summary: Demo call"));
    assert_eq!(app.status_message.as_deref(), Some("Opening in browser..."));
}

#[test]
fn open_in_browser_action_missing_body_queues_fetch_and_opens_on_success() {
    let mut app = App::new();
    let env = make_test_envelopes(1).remove(0);
    app.mailbox.viewing_envelope = Some(env.clone());

    app.apply(Action::OpenInBrowser);

    assert_eq!(app.mailbox.queued_body_fetches, vec![env.id.clone()]);
    assert!(app.mailbox.in_flight_body_requests.contains(&env.id));
    assert_eq!(
        app.mailbox.pending_browser_open_after_load,
        Some(env.id.clone())
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Loading message body...")
    );

    app.resolve_body_success(MessageBody {
        message_id: env.id.clone(),
        text_plain: Some("Loaded later".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    let pending = app
        .mailbox
        .pending_browser_open
        .as_ref()
        .expect("browser open should resume after body load");
    assert_eq!(pending.message_id, env.id);
    assert!(pending.document.contains("<pre>Loaded later</pre>"));
    assert!(app.mailbox.pending_browser_open_after_load.is_none());
}

#[test]
fn app_move_down_bounds() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.apply(Action::MoveDown);
    app.apply(Action::MoveDown);
    app.apply(Action::MoveDown);
    assert_eq!(app.mailbox.selected_index, 2);
}

#[test]
fn layout_mode_switching() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    app.apply(Action::OpenMessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    app.apply(Action::CloseMessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
}

#[test]
fn fullscreen_opens_selected_message_from_mail_list() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::ToggleFullscreen);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::FullScreen);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert!(app.mailbox.viewing_envelope.is_some());
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[0].id.clone())
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Showing full message view")
    );
}

#[test]
fn fullscreen_keeps_sidebar_visible() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::ToggleFullscreen);

    let output = render_to_string(120, 20, |frame| app.draw(frame));
    assert!(output.contains("Sidebar"));
    assert!(output.contains("Inbox"));
    assert!(output.contains("Subject 0"));
}

#[test]
fn fullscreen_switch_pane_skips_hidden_mail_list() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::ToggleFullscreen);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);

    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::Sidebar);

    app.apply(Action::SwitchPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
}

#[test]
fn command_palette_toggle() {
    let mut p = CommandPalette::default();
    assert!(!p.visible);
    p.toggle(crate::action::UiContext::MailboxList);
    assert!(p.visible);
    p.toggle(crate::action::UiContext::MailboxList);
    assert!(!p.visible);
}

#[test]
fn command_palette_fuzzy_filter() {
    let mut p = CommandPalette::default();
    p.toggle(crate::action::UiContext::MailboxList);
    p.on_char('i');
    p.on_char('n');
    p.on_char('b');
    let labels: Vec<&str> = p
        .filtered
        .iter()
        .map(|&i| p.commands[i].label.as_str())
        .collect();
    assert!(labels.contains(&"Go to Inbox"));
}

#[test]
fn command_palette_shortcut_filter_finds_edit_config() {
    let mut p = CommandPalette::default();
    p.toggle(crate::action::UiContext::MailboxList);
    p.on_char('g');
    p.on_char('c');
    let labels: Vec<&str> = p
        .filtered
        .iter()
        .map(|&i| p.commands[i].label.as_str())
        .collect();
    assert!(labels.contains(&"Edit Config"));
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

#[test]
fn unsubscribe_without_method_sets_status_error() {
    let mut app = App::new();
    let env = make_unsubscribe_envelope(
        AccountId::new(),
        "news@example.com",
        UnsubscribeMethod::None,
    );
    app.mailbox.envelopes = vec![env];

    app.apply(Action::Unsubscribe);

    assert!(app.modals.pending_unsubscribe_confirm.is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("No unsubscribe option found for this message")
    );
}

#[test]
fn unsubscribe_confirm_archive_populates_pending_action() {
    let mut app = App::new();
    let env = make_unsubscribe_envelope(
        AccountId::new(),
        "news@example.com",
        UnsubscribeMethod::OneClick {
            url: "https://example.com/one-click".into(),
        },
    );
    app.mailbox.envelopes = vec![env.clone()];
    app.mailbox.all_envelopes = vec![env.clone()];
    app.apply(Action::Unsubscribe);
    app.apply(Action::ConfirmUnsubscribeAndArchiveSender);

    let pending = app
        .modals
        .pending_unsubscribe_action
        .as_ref()
        .expect("unsubscribe action should be queued");
    assert_eq!(pending.message_id, env.id);
    assert_eq!(pending.archive_message_ids.len(), 1);
    assert_eq!(pending.sender_email, "news@example.com");
}

#[test]
fn search_input_lifecycle() {
    let mut bar = SearchBar::default();
    bar.activate();
    assert!(bar.active);
    bar.on_char('h');
    bar.on_char('e');
    bar.on_char('l');
    bar.on_char('l');
    bar.on_char('o');
    assert_eq!(bar.query, "hello");
    let q = bar.submit();
    assert_eq!(q, "hello");
    assert!(!bar.active);
}

#[test]
fn search_bar_cycles_modes() {
    let mut bar = SearchBar::default();
    assert_eq!(bar.mode, mxr_core::SearchMode::Lexical);
    bar.cycle_mode();
    assert_eq!(bar.mode, mxr_core::SearchMode::Hybrid);
    bar.cycle_mode();
    assert_eq!(bar.mode, mxr_core::SearchMode::Semantic);
    bar.cycle_mode();
    assert_eq!(bar.mode, mxr_core::SearchMode::Lexical);
}

#[test]
fn reopening_active_search_preserves_query() {
    let mut app = App::new();
    app.search.active = true;
    app.search.bar.query = "deploy".to_string();
    app.search.bar.cursor_pos = 0;

    app.apply(Action::OpenMailboxFilter);

    assert!(app.search.bar.active);
    assert_eq!(app.search.bar.query, "deploy");
    assert_eq!(app.search.bar.cursor_pos, "deploy".len());
}

#[test]
fn g_prefix_navigation() {
    let mut h = InputHandler::new();
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
        Some(Action::GoToInbox)
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
        Some(Action::GoToStarred)
    );
}

#[test]
fn status_bar_sync_formats() {
    assert_eq!(
        status_bar::format_sync_status(12, Some("synced 2m ago")),
        "[INBOX] 12 unread | synced 2m ago"
    );
    assert_eq!(
        status_bar::format_sync_status(0, None),
        "[INBOX] 0 unread | not synced"
    );
}

#[test]
fn threepane_l_loads_new_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Open first message
    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    let first_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    // Move focus back to mail list
    app.mailbox.active_pane = ActivePane::MailList;
    // Navigate to second message
    app.apply(Action::MoveDown);
    // Press l (which triggers OpenSelected)
    app.apply(Action::OpenSelected);
    let second_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    assert_ne!(
        first_id, second_id,
        "l should load the new message, not stay on old one"
    );
    assert_eq!(app.mailbox.selected_index, 1);
}

#[test]
fn threepane_jk_auto_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Open first message to enter ThreePane
    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    let first_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    // Move focus back to mail list
    app.mailbox.active_pane = ActivePane::MailList;
    // Move down — should auto-preview
    app.apply(Action::MoveDown);
    let preview_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    assert_ne!(first_id, preview_id, "j/k should auto-preview in ThreePane");
    // Body should be loaded from cache (or None if not cached in test)
    // No async fetch needed — bodies are inline with envelopes
}

#[test]
fn twopane_jk_no_auto_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    // Don't open message — stay in TwoPane
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    app.apply(Action::MoveDown);
    assert!(
        app.mailbox.viewing_envelope.is_none(),
        "j/k should not auto-preview in TwoPane"
    );
    // No body fetch triggered in TwoPane mode
}

#[test]
fn back_in_message_view_closes_preview_pane() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    app.apply(Action::Back);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert!(app.mailbox.viewing_envelope.is_none());
}

#[test]
fn back_in_mail_list_clears_label_filter() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.labels = make_test_labels();
    let inbox_id = app
        .mailbox
        .labels
        .iter()
        .find(|l| l.name == "INBOX")
        .unwrap()
        .id
        .clone();
    // Simulate label filter active
    app.mailbox.active_label = Some(inbox_id);
    app.mailbox.envelopes = vec![app.mailbox.envelopes[0].clone()]; // Filtered down
                                                                    // Esc should clear filter
    app.apply(Action::Back);
    assert!(
        app.mailbox.active_label.is_none(),
        "Esc should clear label filter"
    );
    assert_eq!(
        app.mailbox.envelopes.len(),
        5,
        "Should restore all envelopes"
    );
}

#[test]
fn back_in_mail_list_closes_threepane_when_no_filter() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected); // ThreePane
    app.mailbox.active_pane = ActivePane::MailList; // Move back
                                                    // No filter active — Esc should close ThreePane
    app.apply(Action::Back);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
}

#[test]
fn sidebar_system_labels_before_user_labels() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    // System labels should come first
    let first_user_idx = ordered.iter().position(|l| l.kind == LabelKind::User);
    let last_system_idx = ordered.iter().rposition(|l| l.kind == LabelKind::System);
    if let (Some(first_user), Some(last_system)) = (first_user_idx, last_system_idx) {
        assert!(
            last_system < first_user,
            "All system labels should come before user labels"
        );
    }
}

/// Phase 4: Tab inside the help modal cycles the context filter so a
/// user can browse another view's bindings without leaving the modal.
/// Cycling all the way around lands back on the focused view, which
/// clears the override so help tracks focus again.
#[test]
fn help_modal_tab_cycles_context_filter_and_wraps_to_focused_view() {
    use crate::action::UiContext;
    let mut app = App::new();
    app.apply(Action::Help);
    assert!(app.modals.help_open, "help must open");
    assert!(
        app.modals.help_context_filter.is_none(),
        "filter defaults to the focused view"
    );
    let focused = app.help_modal_context();

    let _ = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let first_override = app
        .modals
        .help_context_filter
        .expect("first Tab must set an override");
    assert_ne!(first_override, focused, "override must move off focus");
    assert_eq!(app.help_modal_context(), first_override);

    // Cycle through the remaining contexts; the wrap back to the focused
    // view clears the override instead of storing a redundant Some.
    for _ in 0..16 {
        if app.modals.help_context_filter.is_none() {
            break;
        }
        let _ = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert!(
        app.modals.help_context_filter.is_none(),
        "cycling must eventually wrap back to following focus"
    );
    assert_eq!(app.help_modal_context(), focused);

    // Closing and reopening help resets any leftover filter.
    let _ = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    app.apply(Action::Help);
    app.apply(Action::Help);
    assert!(app.modals.help_context_filter.is_none());
}

/// Phase 4: while a multi-key chord is pending ("g …"), the input
/// handler exposes the prefix so the hint bar can render it.
#[test]
fn pending_chord_prefix_is_exposed_for_the_hint_bar() {
    let mut app = App::new();
    assert_eq!(app.pending_input_prefix(), None);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert_eq!(app.pending_input_prefix(), Some('g'));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    assert_eq!(app.pending_input_prefix(), None, "chord completion clears the prefix");
}
