use super::*;

#[test]
fn paired_remote_open_does_not_auto_mark_unread_message_read() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.enable_paired_remote_read_only_session();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn auth_required_form_generates_secret_refs_from_account_key() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.mode = crate::app::AccountFormMode::ImapSmtp;
    app.accounts.page.form.key = "work".into();
    app.accounts.page.form.email = "me@example.com".into();
    app.accounts.page.form.imap_host = "imap.example.com".into();
    app.accounts.page.form.imap_port = "993".into();
    app.accounts.page.form.imap_password = "imap-secret".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();
    app.accounts.page.form.smtp_port = "587".into();
    app.accounts.page.form.smtp_password = "smtp-secret".into();

    app.apply(Action::TestAccountForm);

    let pending = app.accounts.pending_test.take().unwrap();
    match pending.sync.unwrap() {
        mxr_protocol::AccountSyncConfigData::Imap { password_ref, .. } => {
            assert_eq!(password_ref, "mxr/work-imap");
        }
        other => panic!("expected imap config, got {other:?}"),
    }
    match pending.send.unwrap() {
        mxr_protocol::AccountSendConfigData::Smtp { password_ref, .. } => {
            assert_eq!(password_ref, "mxr/work-smtp");
        }
        other => panic!("expected smtp config, got {other:?}"),
    }
}

#[test]
fn search_screen_typing_updates_results_and_queues_search() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(2);
    envelopes[0].subject = "crates.io release".into();
    envelopes[0].snippet = "mxr publish".into();
    envelopes[1].subject = "support request".into();
    envelopes[1].snippet = "billing".into();
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;

    app.apply(Action::OpenSearchScreen);
    app.search.page.query.clear();
    app.search.page.results = app.mailbox.all_envelopes.clone();

    for ch in "crate".chars() {
        let action = app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        assert!(action.is_none());
    }

    assert_eq!(app.search.page.query, "crate");
    assert!(app.search.page.results.is_empty());
    assert!(!app.search.page.loading_more);
    assert!(!app.search.page.count_pending);
    assert_eq!(
        app.search.page.ui_status,
        crate::app::SearchUiStatus::Debouncing
    );
    assert_eq!(
        app.search.pending_debounce,
        Some(crate::app::PendingSearchDebounce {
            query: "crate".into(),
            mode: mxr_core::SearchMode::Lexical,
            session_id: app.search.page.session_id,
            due_at: app
                .search
                .pending_debounce
                .as_ref()
                .map(|pending| pending.due_at)
                .expect("debounce timer should be set"),
        })
    );
    assert!(app.search.pending.is_none());
    assert!(app.search.pending_count.is_none());
}

#[test]
fn visual_line_selection_expands_thread_rows_not_flat_envelope_indices() {
    let mut app = App::new();

    let mut envelopes = make_test_envelopes(3);
    let thread0 = ThreadId::new();
    let thread1 = ThreadId::new();
    envelopes[0].thread_id = thread0.clone();
    envelopes[1].thread_id = thread0;
    envelopes[2].thread_id = thread1;
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes.clone();

    assert_eq!(app.mailbox.mail_list_mode, MailListMode::Threads);
    assert_eq!(
        app.mail_list_rows().len(),
        2,
        "two threads collapse to two rows"
    );

    app.apply(Action::VisualLineMode);
    assert_eq!(app.mailbox.visual_anchor, Some(0));
    app.apply(Action::MoveDown);
    assert_eq!(app.mailbox.selected_index, 1);

    let expected: std::collections::HashSet<MessageId> =
        envelopes.iter().map(|env| env.id.clone()).collect();
    assert_eq!(
        app.mailbox.selected_set, expected,
        "visual-line selection must expand each row to all its thread member ids"
    );

    let flat_slice: std::collections::HashSet<MessageId> =
        envelopes[0..2].iter().map(|env| env.id.clone()).collect();
    assert_ne!(
        app.mailbox.selected_set, flat_slice,
        "must not select the flat envelope slice — in Threads mode row index is not envelope index"
    );
}

#[test]
fn recoverable_archive_never_automatically_creates_a_retry_identity() {
    assert!(!super::super::mutation_can_auto_retry(&Request::mutation(
        MutationCommand::Archive {
            message_ids: vec![MessageId::new()]
        }
    )));
    assert!(super::super::mutation_can_auto_retry(&Request::mutation(
        MutationCommand::SetRead {
            message_ids: vec![MessageId::new()],
            read: true
        }
    )));
}

#[test]
fn archive_preview_retains_selection_without_queueing_a_mutation() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let selected = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::PreviewArchiveSelection);
    assert_eq!(app.pending_archive_preview, Some(vec![selected]));
    assert!(app.pending_mutation_queue.is_empty());
    assert!(app.modals.platform.visible && app.modals.platform.loading);
}

#[test]
fn partial_archive_error_keeps_undo_for_confirmed_completed_items() {
    use mxr_protocol::{
        ArchiveIdentityData, ArchiveItemData, ArchiveItemStateData, ArchiveOperationData,
        ArchiveWorkspaceData,
    };
    let completed = ArchiveItemData {
        message_id: MessageId::new(),
        account_id: mxr_core::AccountId::new(),
        provider_id: "synthetic".into(),
        prior_flags_bits: 0,
        prior_label_provider_ids: vec!["INBOX".into()],
        was_inbox: true,
        state: ArchiveItemStateData::Completed,
        reason: None,
    };
    let mut unknown = completed.clone();
    unknown.message_id = MessageId::new();
    unknown.state = ArchiveItemStateData::Unknown;
    let operation = ArchiveOperationData {
        identity: ArchiveIdentityData {
            operation_id: "op".into(),
            caller_id: "caller".into(),
            workspace: ArchiveWorkspaceData {
                workspace_id: "workspace".into(),
                recovery_generation: "generation".into(),
            },
        },
        review_id: "review".into(),
        mutation_id: "retained-undo".into(),
        accepted: true,
        undo_available: true,
        running: false,
        started_at: 0,
        items: vec![completed, unknown],
    };
    let undo = super::super::archive_partial_undo(
        Some(serde_json::to_value(operation).unwrap()),
        "Archived",
    )
    .unwrap();
    assert_eq!(undo.count, 1);
    assert!(
        matches!(undo.action, crate::app::UndoAction::Mutations(ids) if ids == vec!["retained-undo".to_string()])
    );
}
