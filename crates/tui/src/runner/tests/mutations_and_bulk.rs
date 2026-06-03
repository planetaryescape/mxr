use super::*;

#[test]
fn single_message_view_uses_jk_to_scroll() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(app.mailbox.viewed_thread_messages.len(), 1);
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 0);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 1);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 2);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 1);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert_eq!(app.mailbox.message_scroll_offset, 0);
}

#[test]
fn thread_move_down_changes_reply_target() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.envelopes[1].flags = MessageFlags::READ;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    assert_eq!(
        app.focused_thread_envelope().map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[0].id.clone())
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    assert_eq!(
        app.focused_thread_envelope().map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[1].id.clone())
    );
    app.apply(Action::Reply);
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::Reply {
            message_id: app.mailbox.envelopes[1].id.clone(),
            account_id: app.mailbox.envelopes[1].account_id.clone(),
            preloaded: None,
        })
    );
}

#[test]
fn thread_focus_change_marks_newly_focused_unread_message_read_after_dwell() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.envelopes[1].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    assert_eq!(app.mailbox.thread_selected_index, 1);
    assert!(app.pending_mutation_queue.is_empty());

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

    assert_eq!(app.mailbox.thread_selected_index, 0);
    assert!(!app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());

    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::SetRead { message_ids, read },
            ..
        } => {
            assert!(*read);
            assert_eq!(message_ids, &vec![app.mailbox.envelopes[0].id.clone()]);
        }
        other => panic!("expected set-read mutation, got {other:?}"),
    }
}

#[test]
fn preview_navigation_only_marks_message_read_after_settling() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.envelopes[1].flags = MessageFlags::empty();
    app.mailbox.envelopes[0].thread_id = ThreadId::new();
    app.mailbox.envelopes[1].thread_id = ThreadId::new();
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(1);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.apply(Action::MoveDown);

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app.mailbox.envelopes[1].flags.contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());

    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(app.mailbox.envelopes[1].flags.contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::SetRead { message_ids, read },
            ..
        } => {
            assert!(*read);
            assert_eq!(message_ids, &vec![app.mailbox.envelopes[1].id.clone()]);
        }
        other => panic!("expected set-read mutation, got {other:?}"),
    }
}

#[test]
fn help_action_toggles_modal_state() {
    let mut app = App::new();

    app.apply(Action::Help);
    assert!(app.modals.help_open);
    assert!(app.modals.help_query.is_empty());
    assert_eq!(app.modals.help_selected, 0);

    app.modals.help_query = "config".into();
    app.modals.help_selected = 3;
    app.apply(Action::Help);
    assert!(!app.modals.help_open);
    assert!(app.modals.help_query.is_empty());
    assert_eq!(app.modals.help_selected, 0);
}

#[test]
fn help_modal_typing_enters_search_mode_and_backspace_clears_it() {
    let mut app = App::new();
    app.apply(Action::Help);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "g");
    assert_eq!(app.modals.help_selected, 0);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "gc");

    let action = app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "g");

    let action = app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(app.modals.help_query.is_empty());
    assert_eq!(app.modals.help_selected, 0);
}

#[test]
fn help_modal_o_types_instead_of_reopening_onboarding() {
    let mut app = App::new();
    app.apply(Action::Help);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.help_query, "o");
    assert!(!app.modals.onboarding.visible);
}

#[test]
fn account_form_validation_points_to_first_invalid_field() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.mode = crate::app::AccountFormMode::ImapSmtp;
    app.accounts.page.form.key = "work".into();
    app.accounts.page.form.email = "me@example.com".into();
    app.accounts.page.form.imap_port = "993".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();
    app.accounts.page.form.smtp_port = "587".into();
    app.accounts.page.form.smtp_auth_required = false;

    app.apply(Action::TestAccountForm);

    assert_eq!(app.accounts.page.form.active_field, 4);
    assert!(!app.accounts.page.operation_in_flight);
    assert!(app.accounts.pending_test.is_none());
    let result = app.accounts.page.form.last_result.as_ref().unwrap();
    assert!(result.summary.contains("Account form has problems."));
    assert_eq!(
            result.sync.as_ref().unwrap().detail,
            "IMAP host is required. IMAP auth is enabled, so IMAP password or IMAP pass ref is required."
        );
}

#[test]
fn smtp_only_form_test_allows_no_auth_and_marks_operation_pending() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.mode = crate::app::AccountFormMode::SmtpOnly;
    app.accounts.page.form.key = "relay".into();
    app.accounts.page.form.email = "relay@example.com".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();
    app.accounts.page.form.smtp_port = "25".into();
    app.accounts.page.form.smtp_auth_required = false;
    app.accounts.page.form.last_result = Some(mxr_protocol::AccountOperationResult {
        ok: false,
        summary: "stale".into(),
        save: None,
        auth: None,
        sync: None,
        send: None,
        device_code_url: None,
        device_code_user_code: None,
    });

    app.apply(Action::TestAccountForm);

    assert!(app.accounts.page.operation_in_flight);
    assert!(app.accounts.page.form.last_result.is_none());
    let pending = app.accounts.pending_test.take().unwrap();
    match pending.send.unwrap() {
        mxr_protocol::AccountSendConfigData::Smtp {
            auth_required,
            username,
            password_ref,
            ..
        } => {
            assert!(!auth_required);
            assert!(username.is_empty());
            assert!(password_ref.is_empty());
        }
        other => panic!("expected smtp config, got {other:?}"),
    }
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
fn failed_account_operation_opens_details_modal() {
    let mut app = App::new();
    let result = mxr_protocol::AccountOperationResult {
            ok: false,
            summary: "Account 'consulting' test failed.".into(),
            save: None,
            auth: None,
            sync: Some(mxr_protocol::AccountOperationStep {
                ok: false,
                detail: "IMAP server returned a NAMESPACE response in an unsupported format during folder discovery. This looks like a server compatibility issue, not necessarily a bad username or password.".into(),
            }),
            send: Some(mxr_protocol::AccountOperationStep {
                ok: true,
                detail: "SMTP send ok".into(),
            }),
            device_code_url: None,
            device_code_user_code: None,
        };

    app.apply_account_operation_result(result);

    let modal = app.modals.error.as_ref().unwrap();
    assert_eq!(modal.title, "Account Test Failed");
    assert!(modal.detail.contains("NAMESPACE response"));
    assert!(modal.detail.contains("compatibility issue"));
}

#[test]
fn account_form_o_reopens_result_details_modal() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.last_result = Some(mxr_protocol::AccountOperationResult {
        ok: false,
        summary: "Account 'consulting' test failed.".into(),
        save: None,
        auth: None,
        sync: Some(mxr_protocol::AccountOperationStep {
            ok: false,
            detail: "IMAP server returned a response mxr could not parse.".into(),
        }),
        send: None,
        device_code_url: None,
        device_code_user_code: None,
    });

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    assert!(action.is_none());
    assert_eq!(
        app.modals.error.as_ref().map(|modal| modal.title.as_str()),
        Some("Account Test Failed")
    );
}

#[test]
fn error_modal_supports_scrolling_keys() {
    let mut app = App::new();
    app.modals.error = Some(crate::app::ErrorModalState::new(
        "Account Test Failed",
        "line1\nline2\nline3\nline4\nline5",
    ));

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 1);

    let action = app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 9);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 8);

    let action = app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(app.modals.error.as_ref().unwrap().scroll_offset, 0);
}

#[test]
fn closing_new_account_form_preserves_draft_and_resume_restores_it() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.key = "draft".into();
    app.accounts.page.form.email = "draft@example.com".into();
    app.accounts.page.form.smtp_host = "smtp.example.com".into();

    let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(!app.accounts.page.form.visible);
    assert_eq!(
        app.accounts.page.new_account_draft.as_ref().unwrap().key,
        "draft"
    );

    app.apply(Action::OpenAccountFormNew);
    assert!(app.accounts.page.resume_new_account_draft_prompt_open);
    assert!(!app.accounts.page.form.visible);

    let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(app.accounts.page.form.visible);
    assert_eq!(app.accounts.page.form.key, "draft");
    assert_eq!(app.accounts.page.form.email, "draft@example.com");
    assert!(app.accounts.page.new_account_draft.is_none());
}

#[test]
fn new_account_draft_prompt_can_start_fresh_form() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.key = "draft".into();
    app.accounts.page.form.email = "draft@example.com".into();

    let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(action.is_none());
    assert_eq!(
        app.accounts
            .page
            .new_account_draft
            .as_ref()
            .map(|draft| draft.email.as_str()),
        Some("draft@example.com")
    );

    app.apply(Action::OpenAccountFormNew);
    assert!(app.accounts.page.resume_new_account_draft_prompt_open);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(action.is_none());
    assert!(app.accounts.page.form.visible);
    assert!(app.accounts.page.form.is_new_account);
    assert!(app.accounts.page.form.key.is_empty());
    assert!(app.accounts.page.new_account_draft.is_none());
    assert!(!app.accounts.page.resume_new_account_draft_prompt_open);
}

#[test]
fn leaving_accounts_screen_preserves_new_account_draft() {
    let mut app = App::new();
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.is_new_account = true;
    app.accounts.page.form.key = "draft".into();
    app.accounts.page.form.email = "draft@example.com".into();

    app.apply(Action::OpenMailboxScreen);

    assert_eq!(app.screen, Screen::Mailbox);
    assert!(!app.accounts.page.form.visible);
    assert_eq!(
        app.accounts.page.new_account_draft.as_ref().unwrap().email,
        "draft@example.com"
    );
}

#[test]
fn open_search_screen_activates_dedicated_search_workspace() {
    let mut app = App::new();
    app.apply(Action::OpenSearchScreen);
    assert_eq!(app.screen, Screen::Search);
    assert!(app.search.page.editing);
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
fn open_search_screen_preserves_existing_search_session() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    app.search.bar.query = "stale overlay".into();
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.search.page.result_selected = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewing_envelope = Some(results[1].clone());

    app.apply(Action::OpenRulesScreen);
    app.apply(Action::OpenSearchScreen);

    assert_eq!(app.screen, Screen::Search);
    assert_eq!(app.search.page.query, "deploy");
    assert_eq!(app.search.page.results.len(), 2);
    assert_eq!(app.search.page.selected_index, 1);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
    assert!(app.search.pending.is_none());
}

#[test]
fn slash_opens_global_search_and_ctrl_f_opens_mailbox_filter() {
    let mut app = App::new();

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::OpenGlobalSearch));
    app.apply(action.expect("slash should map to search"));
    assert_eq!(app.screen, Screen::Search);
    assert!(app.search.page.editing);

    app.apply(Action::OpenMailboxScreen);
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert_eq!(action, Some(Action::OpenMailboxFilter));
}

#[test]
fn search_results_accept_gg_and_g_navigation() {
    let mut app = App::new();
    app.apply(Action::OpenSearchScreen);
    app.search.page.editing = false;
    app.search.page.results = make_test_envelopes(3);
    app.search.page.selected_index = 2;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert!(action.is_none());
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::JumpTop));
    app.apply(action.unwrap());
    assert_eq!(app.search.page.selected_index, 0);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT));
    assert_eq!(action, Some(Action::JumpBottom));
    app.apply(action.unwrap());
    assert_eq!(app.search.page.selected_index, 2);
}

#[test]
fn open_search_screen_without_session_clears_stale_preview_and_query() {
    let mut app = App::new();
    let envelope = make_test_envelopes(1).remove(0);
    app.search.bar.query = "mailbox quick filter".into();
    app.mailbox.viewing_envelope = Some(envelope.clone());
    app.mailbox.viewed_thread_messages = vec![envelope];
    app.search.page.query = "stale".into();
    app.search.page.session_active = false;
    app.search.page.results.clear();

    app.apply(Action::OpenSearchScreen);

    assert_eq!(app.screen, Screen::Search);
    assert!(app.search.page.editing);
    assert!(app.search.page.query.is_empty());
    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
    assert_eq!(app.search.page.ui_status, crate::app::SearchUiStatus::Idle);
}

#[test]
fn non_mail_screens_ignore_label_shortcut() {
    let mut app = App::new();

    for screen in [Screen::Rules, Screen::Accounts, Screen::Diagnostics] {
        app.screen = screen;
        app.modals.label_picker.close();
        let action = app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert!(action.is_none(), "unexpected action on {screen:?}");
        assert!(
            !app.modals.label_picker.visible,
            "label picker opened on {screen:?}"
        );
    }
}

#[test]
fn rules_navigation_refreshes_selected_panel_request() {
    let mut app = App::new();
    app.screen = Screen::Rules;
    app.rules.page.rules = vec![
        serde_json::json!({"id": "rule-1", "name": "One"}),
        serde_json::json!({"id": "rule-2", "name": "Two"}),
    ];
    app.rules.page.panel = crate::app::RulesPanel::History;

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.rules.page.selected_index, 1);
    assert_eq!(app.rules.pending_history.as_deref(), Some("rule-2"));

    app.rules.page.panel = crate::app::RulesPanel::DryRun;
    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.rules.page.selected_index, 0);
    assert_eq!(app.rules.pending_dry_run.as_deref(), Some("rule-1"));
}

#[test]
fn search_open_selected_keeps_search_screen_and_focuses_preview() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;

    app.apply(Action::OpenSelected);

    assert_eq!(app.screen, Screen::Search);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
}

#[test]
fn search_open_message_follows_cursor_after_returning_to_results() {
    let mut app = App::new();
    let results = make_test_envelopes(3);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.mailbox.all_envelopes = results.clone();

    app.apply(Action::OpenSelected);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[0].id.clone())
    );

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.search.page.active_pane, SearchPane::Results);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[0].id.clone())
    );

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.search.page.selected_index, 1);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
}

#[test]
fn search_results_allow_mail_actions_without_preview_focus() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.search.page.active_pane = SearchPane::Results;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::Star));

    app.apply(action.expect("star action should be available from search results"));

    assert!(app.search.page.results[1]
        .flags
        .contains(MessageFlags::STARRED));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation:
                MutationCommand::Star {
                    message_ids,
                    starred,
                },
            ..
        } => {
            assert_eq!(message_ids, &vec![results[1].id.clone()]);
            assert!(*starred);
        }
        other => panic!("expected star mutation, got {other:?}"),
    }
}

#[test]
fn search_results_follow_mail_list_mode_and_open_thread_rows() {
    let mut app = App::new();
    let thread_id = ThreadId::new();
    let now = chrono::Utc::now();
    let older = TestEnvelopeBuilder::new()
        .provider_id("thread-old")
        .thread_id(thread_id.clone())
        .subject("Older hit")
        .date(now - chrono::Duration::minutes(5))
        .build();
    let newer = TestEnvelopeBuilder::new()
        .provider_id("thread-new")
        .thread_id(thread_id)
        .subject("Newer hit")
        .date(now)
        .build();
    let other = TestEnvelopeBuilder::new()
        .provider_id("other-thread")
        .subject("Other thread")
        .date(now - chrono::Duration::minutes(1))
        .build();
    let results = vec![older, newer.clone(), other];
    app.mailbox.mail_list_mode = MailListMode::Messages;
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.mailbox.all_envelopes = results;

    app.apply(Action::ToggleMailListMode);

    assert_eq!(app.search_row_count(), 2);
    assert_eq!(
        app.selected_search_envelope().map(|env| env.id.clone()),
        Some(newer.id.clone())
    );

    app.apply(Action::OpenSelected);

    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(newer.id.clone())
    );
}

#[test]
fn search_results_refresh_preserves_open_row_when_it_still_exists() {
    let mut app = App::new();
    let results = make_test_envelopes(3);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.mailbox.all_envelopes = results.clone();

    app.apply(Action::OpenSelected);
    app.apply_search_page_results(
        false,
        SearchResultData {
            envelopes: vec![results[0].clone(), results[1].clone()],
            scores: std::collections::HashMap::new(),
            triage_verdicts: std::collections::HashMap::new(),
            has_more: false,
        },
    );

    assert_eq!(app.search.page.selected_index, 1);
    assert!(app.search.page.result_selected);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(results[1].id.clone())
    );
}

#[test]
fn search_results_refresh_clears_open_message_when_selected_row_disappears() {
    let mut app = App::new();
    let results = make_test_envelopes(3);
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results.clone();
    app.search.page.session_active = true;
    app.search.page.selected_index = 1;
    app.mailbox.all_envelopes = results.clone();

    app.apply(Action::OpenSelected);
    app.apply_search_page_results(
        false,
        SearchResultData {
            envelopes: vec![results[0].clone()],
            scores: std::collections::HashMap::new(),
            triage_verdicts: std::collections::HashMap::new(),
            has_more: false,
        },
    );

    assert_eq!(app.search.page.selected_index, 0);
    assert!(!app.search.page.result_selected);
    assert_eq!(app.search.page.active_pane, SearchPane::Results);
    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
}

#[test]
fn search_jump_bottom_loads_remaining_pages() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = make_test_envelopes(3);
    app.search.page.session_active = true;
    app.search.page.has_more = true;
    app.search.page.loading_more = false;
    app.search.page.session_id = 9;

    app.apply(Action::JumpBottom);

    assert!(app.search.page.load_to_end);
    assert!(app.search.page.loading_more);
    assert_eq!(
        app.search.pending,
        Some(PendingSearchRequest {
            query: "deploy".into(),
            mode: mxr_core::SearchMode::Lexical,
            sort: mxr_core::SortOrder::DateDesc,
            limit: SEARCH_PAGE_SIZE,
            offset: 3,
            target: SearchTarget::SearchPage,
            append: true,
            session_id: 9,
        })
    );
}

#[test]
fn search_jump_bottom_uses_search_results_viewport_height() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = make_test_envelopes(15);
    app.search.page.session_active = true;

    let _ = render_to_string(120, 20, |frame| app.draw(frame));

    app.apply(Action::JumpBottom);

    assert_eq!(app.visible_height, 10);
    assert_eq!(app.search.page.selected_index, 14);
    assert_eq!(app.search.page.scroll_offset, 5);
}

#[test]
fn search_escape_routes_back_to_inbox() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.session_active = true;
    app.search.page.query = "deploy".into();
    app.search.page.results = make_test_envelopes(2);
    app.search.page.active_pane = SearchPane::Results;

    let action = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenMailboxScreen));
}

#[test]
fn open_rules_screen_marks_refresh_pending() {
    let mut app = App::new();
    app.apply(Action::OpenRulesScreen);
    assert_eq!(app.screen, Screen::Rules);
    assert!(app.rules.page.refresh_pending);
}

#[test]
fn open_diagnostics_screen_marks_refresh_pending() {
    let mut app = App::new();
    app.apply(Action::OpenDiagnosticsScreen);
    assert_eq!(app.screen, Screen::Diagnostics);
    assert!(app.diagnostics.page.refresh_pending);
}

#[test]
fn open_accounts_screen_marks_refresh_pending() {
    let mut app = App::new();
    app.apply(Action::OpenAccountsScreen);
    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.refresh_pending);
}

#[test]
fn new_account_form_opens_from_accounts_screen() {
    let mut app = App::new();
    app.apply(Action::OpenAccountsScreen);
    app.apply(Action::OpenAccountFormNew);

    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.form.visible);
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
}

#[test]
fn app_from_empty_config_enters_account_onboarding() {
    let config = mxr_config::MxrConfig::default();
    let app = App::from_config(&config);

    // Onboarding modal shows on whatever page the user is on (mailbox by default)
    assert_eq!(app.screen, Screen::Mailbox);
    assert!(app.accounts.page.onboarding_required);
    assert!(app.accounts.page.onboarding_modal_open);
}

#[test]
fn onboarding_confirm_opens_new_account_form() {
    let config = mxr_config::MxrConfig::default();
    let mut app = App::from_config(&config);

    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.form.visible);
    assert!(!app.accounts.page.onboarding_modal_open);
}

#[test]
fn onboarding_q_quits() {
    let config = mxr_config::MxrConfig::default();
    let mut app = App::from_config(&config);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::QuitView));
}

#[test]
fn onboarding_blocks_mailbox_screen_until_account_exists() {
    let config = mxr_config::MxrConfig::default();
    let mut app = App::from_config(&config);

    app.apply(Action::OpenMailboxScreen);

    assert_eq!(app.screen, Screen::Accounts);
    assert!(app.accounts.page.onboarding_required);
}

#[test]
fn account_form_h_and_l_switch_modes_from_any_field() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.active_field = 2;

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::ImapSmtp
    );

    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
}

#[test]
fn account_form_tab_on_mode_cycles_modes() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.active_field = 0;

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::ImapSmtp
    );

    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
}

#[test]
fn account_form_mode_switch_with_input_requires_confirmation() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.key = "work".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
    assert_eq!(
        app.accounts.page.form.pending_mode_switch,
        Some(crate::app::AccountFormMode::ImapSmtp)
    );
}

#[test]
fn account_form_mode_switch_confirmation_applies_mode_change() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.key = "work".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::ImapSmtp
    );
    assert!(app.accounts.page.form.pending_mode_switch.is_none());
}

#[test]
fn account_form_mode_switch_confirmation_cancel_keeps_mode() {
    let mut app = App::new();
    app.apply(Action::OpenAccountFormNew);
    app.accounts.page.form.key = "work".into();

    app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

    assert_eq!(
        app.accounts.page.form.mode,
        crate::app::AccountFormMode::Gmail
    );
    assert!(app.accounts.page.form.pending_mode_switch.is_none());
}

#[test]
fn flattened_sidebar_navigation_reaches_saved_searches() {
    let mut app = App::new();
    app.mailbox.labels = vec![Label {
        id: LabelId::new(),
        account_id: AccountId::new(),
        provider_id: "inbox".into(),
        name: "INBOX".into(),
        kind: LabelKind::System,
        color: None,
        unread_count: 1,
        total_count: 3,
        role: None,
    }];
    app.mailbox.saved_searches = vec![SavedSearch {
        id: SavedSearchId::new(),
        account_id: None,
        name: "Unread".into(),
        query: "is:unread".into(),
        search_mode: SearchMode::Lexical,
        sort: SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: chrono::Utc::now(),
    }];
    app.mailbox.active_pane = ActivePane::Sidebar;

    // Sidebar order: INBOX, AllMail, Subscriptions, Owed (Slice 2.3),
    // CalendarInvites, SavedSearch. Five `j` presses to reach the saved search.
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::SavedSearch(_))
    ));
}

#[test]
fn toggle_select_advances_cursor_and_updates_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    app.mailbox.active_pane = ActivePane::MailList;

    app.apply(Action::ToggleSelect);

    assert_eq!(app.mailbox.selected_index, 1);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[1].id.clone())
    );
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 1")
    ));
}

#[test]
fn toggle_select_in_message_view_keeps_current_message_visible() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);

    let original_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    app.apply(Action::ToggleSelect);

    assert_eq!(app.mailbox.selected_index, 0);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(original_id.clone())
    );
    assert!(app.mailbox.selected_set.contains(&original_id));
}

#[test]
fn label_count_updates_preserve_sidebar_selection_identity() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();

    let selected_index = app
        .sidebar_items()
        .iter()
        .position(
            |item| matches!(item, crate::app::SidebarItem::Label(label) if label.name == "Work"),
        )
        .unwrap();
    app.mailbox.sidebar_selected = selected_index;

    handle_daemon_event(
        &mut app,
        DaemonEvent::LabelCountsUpdated {
            counts: vec![
                LabelCount {
                    label_id: LabelId::from_provider_id("test", "STARRED"),
                    unread_count: 0,
                    total_count: 0,
                },
                LabelCount {
                    label_id: LabelId::from_provider_id("test", "SENT"),
                    unread_count: 0,
                    total_count: 0,
                },
            ],
        },
    );

    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::Label(label)) if label.name == "Work"
    ));
}

#[test]
fn labels_refresh_preserves_active_label_context_when_label_becomes_empty() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let work = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "Work")
        .unwrap()
        .clone();
    app.mailbox.active_label = Some(work.id.clone());
    app.mailbox.sidebar_selected = app
        .sidebar_items()
        .iter()
        .position(
            |item| matches!(item, crate::app::SidebarItem::Label(label) if label.id == work.id),
        )
        .unwrap();

    let refreshed = app
        .mailbox
        .labels
        .iter()
        .filter(|label| label.id != work.id)
        .cloned()
        .collect();

    super::super::apply_labels_refresh(&mut app, refreshed);

    let preserved = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.id == work.id)
        .unwrap();
    assert_eq!(preserved.unread_count, 0);
    assert_eq!(preserved.total_count, 0);
    assert_eq!(app.mailbox.active_label.as_ref(), Some(&work.id));
    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::Label(label)) if label.id == work.id
    ));
    assert_eq!(app.status_bar_state().mailbox_name, "Work");
}

#[test]
fn opening_search_result_keeps_search_workspace_open() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.results = make_test_envelopes(2);
    app.search.page.selected_index = 1;

    app.apply(Action::OpenSelected);

    assert_eq!(app.screen, Screen::Search);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(app.search.page.results[1].id.clone())
    );
}

#[test]
fn attachment_list_opens_modal_for_current_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: mxr_core::types::AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 1024,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);
    app.apply(Action::AttachmentList);

    assert!(app.mailbox.attachment_panel.visible);
    assert_eq!(app.mailbox.attachment_panel.attachments.len(), 1);
    assert_eq!(
        app.mailbox.attachment_panel.attachments[0].filename,
        "report.pdf"
    );
}
