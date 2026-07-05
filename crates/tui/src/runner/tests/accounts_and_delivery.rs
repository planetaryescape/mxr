use super::*;

#[test]
fn attachment_list_sorts_file_attachments_before_inline_images() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: Some("<img src=\"cid:inline-1\">".into()),
            attachments: vec![
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "inline-1.png".into(),
                    mime_type: "image/png".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Inline,
                    content_id: Some("inline-1".into()),
                    content_location: None,
                    size_bytes: 10,
                    local_path: None,
                    provider_id: "att-inline-1".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "budget.xlsx".into(),
                    mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                        .into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 20,
                    local_path: None,
                    provider_id: "att-xlsx".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "inline-2.png".into(),
                    mime_type: "image/png".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Inline,
                    content_id: Some("inline-2".into()),
                    content_location: None,
                    size_bytes: 30,
                    local_path: None,
                    provider_id: "att-inline-2".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 40,
                    local_path: None,
                    provider_id: "att-pdf".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);
    app.apply(Action::AttachmentList);

    assert!(app.mailbox.attachment_panel.visible);
    assert_eq!(
        app.mailbox
            .attachment_panel
            .attachments
            .iter()
            .map(|attachment| attachment.filename.as_str())
            .collect::<Vec<_>>(),
        vec!["budget.xlsx", "report.pdf", "inline-1.png", "inline-2.png"]
    );
    assert_eq!(app.mailbox.attachment_panel.selected_index, 0);
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("budget.xlsx")
    );
}

#[test]
fn attachment_list_navigation_follows_sorted_attachment_order() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("hello".into()),
            text_html: Some("<img src=\"cid:inline-1\">".into()),
            attachments: vec![
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "inline-1.png".into(),
                    mime_type: "image/png".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Inline,
                    content_id: Some("inline-1".into()),
                    content_location: None,
                    size_bytes: 10,
                    local_path: None,
                    provider_id: "att-inline-1".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "budget.xlsx".into(),
                    mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                        .into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 20,
                    local_path: None,
                    provider_id: "att-xlsx".into(),
                },
                AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: env.id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 40,
                    local_path: None,
                    provider_id: "att-pdf".into(),
                },
            ],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);
    app.apply(Action::AttachmentList);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("report.pdf")
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("inline-1.png")
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(
        app.selected_attachment()
            .map(|attachment| attachment.filename.as_str()),
        Some("report.pdf")
    );
}

#[test]
fn search_preview_attachment_key_opens_modal() {
    let mut app = App::new();
    let mut results = make_test_envelopes(1);
    results[0].has_attachments = true;
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env.clone());
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

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
    assert_eq!(action, Some(Action::AttachmentList));

    app.apply(Action::AttachmentList);

    assert!(app.mailbox.attachment_panel.visible);
    assert_eq!(app.mailbox.attachment_panel.attachments.len(), 1);
    assert_eq!(
        app.mailbox.attachment_panel.attachments[0].filename,
        "report.pdf"
    );
}

#[test]
fn search_preview_o_opens_in_browser() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenInBrowser));
}

#[test]
fn search_preview_r_toggles_reader_mode_without_shift_modifier() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::ToggleReaderMode));
}

#[test]
fn search_preview_h_and_m_toggle_html_controls_without_shift_modifier() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    let html = app.handle_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE));
    let remote = app.handle_key(KeyEvent::new(KeyCode::Char('M'), KeyModifiers::NONE));

    assert_eq!(html, Some(Action::ToggleHtmlView));
    assert_eq!(remote, Some(Action::ToggleRemoteContent));
}

#[test]
fn search_results_f_opens_full_message_view() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Results;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::ToggleFullscreen));

    app.apply(Action::ToggleFullscreen);

    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert!(app.search.page.result_selected);
    assert!(app.search.page.preview_fullscreen);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|message| message.id.clone()),
        Some(env.id)
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Showing full message view")
    );
}

#[test]
fn search_preview_f_toggles_back_to_split_view() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.search.page.result_selected = true;
    app.search.page.preview_fullscreen = true;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);

    app.apply(Action::ToggleFullscreen);

    assert!(!app.search.page.preview_fullscreen);
    assert_eq!(app.search.page.active_pane, SearchPane::Preview);
    assert_eq!(app.status_message.as_deref(), Some("Showing split view"));
}

#[test]
fn search_fullscreen_render_hides_results_pane() {
    let mut app = App::new();
    let results = make_test_envelopes(1);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.query = "deploy".into();
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.search.page.result_selected = true;
    app.search.page.preview_fullscreen = true;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env);
    app.mailbox.body_view_state = BodyViewState::ready(
        "hello".into(),
        "hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );

    let output = render_to_string(120, 20, |frame| app.draw(frame));

    assert!(output.contains("Search All Mail"));
    assert!(!output.contains("Search Results /"));
}

#[test]
fn search_preview_toggle_select_keeps_current_message_visible() {
    let mut app = App::new();
    let results = make_test_envelopes(2);
    let env = results[0].clone();
    app.screen = Screen::Search;
    app.search.page.results = results;
    app.search.page.session_active = true;
    app.search.page.active_pane = SearchPane::Preview;
    app.mailbox.viewed_thread_messages = vec![env.clone()];
    app.mailbox.viewing_envelope = Some(env.clone());

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert_eq!(action, Some(Action::ToggleSelect));

    app.apply(Action::ToggleSelect);

    assert_eq!(app.search.page.selected_index, 0);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|current| current.id.clone()),
        Some(env.id.clone())
    );
    assert!(app.mailbox.selected_set.contains(&env.id));
}

#[tokio::test]
async fn unchanged_editor_result_disables_send_actions() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-test-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: a@example.com\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let pending = pending_send_from_edited_draft(&ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: content.to_string(),
        invite_reply: None,
    })
    .await
    .unwrap();

    assert_eq!(pending.mode, PendingSendMode::Unchanged);

    let _ = std::fs::remove_file(temp);
}

#[test]
fn send_key_is_ignored_for_unchanged_draft_confirmation() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
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
            draft.id.clone()
        }
        other => panic!("Expected SaveDraft prelude, got {other:?}"),
    };
    match &queue[0].request {
        Request::ScheduleSend {
            draft_id: scheduled_id,
            send_at,
        } => {
            assert_eq!(scheduled_id, &draft_id);
            assert!(*send_at > chrono::Utc::now());
        }
        other => panic!("Expected ScheduleSend request, got {other:?}"),
    }
}

#[test]
fn remind_prompt_sends_draft_with_pending_reminder_time() {
    let mut app = App::new();
    let pending_account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: pending_account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "a@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Needs follow-up".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: vec![],
            thread_id: None,
            attach: vec![],
            signature: None,
        },
        body: "Body".into(),
        draft_path: std::path::PathBuf::from("/tmp/reminder-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    for c in "in 2h".chars() {
        let _ = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(app.compose.pending_remind_at_input.is_none());
    let queued = app
        .pending_mutation_queue
        .first()
        .expect("reminder send queues SendDraft");
    match &queued.request {
        Request::SendDraft { draft, .. } => {
            assert_eq!(draft.account_id, pending_account_id);
            assert_eq!(draft.subject, "Needs follow-up");
        }
        other => panic!("Expected SendDraft request, got {other:?}"),
    }
    match &queued.effect {
        MutationEffect::SentSuccess {
            remind_at,
            sent_message_id,
            ..
        } => {
            assert!(remind_at.is_some(), "reminder time is carried with send");
            assert!(
                sent_message_id.is_none(),
                "sent message id is not known until daemon SendReceipt"
            );
        }
        other => panic!("Expected SentSuccess effect, got {other:?}"),
    }
}

#[test]
fn cancel_reminder_action_queues_cancel_for_focused_message() {
    let mut app = App::new();
    let env = TestEnvelopeBuilder::new().build();
    app.mailbox.viewing_envelope = Some(env.clone());

    app.apply(Action::CancelAutoReminder);

    let queued = app
        .pending_mutation_queue
        .first()
        .expect("cancel reminder should queue daemon mutation");
    match &queued.request {
        Request::CancelAutoReminder { sent_message_id } => {
            assert_eq!(sent_message_id, &env.id);
        }
        other => panic!("Expected CancelAutoReminder request, got {other:?}"),
    }
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Cancelling reminder...")
    );
}

#[test]
fn reminder_triggered_event_marks_reply_queue_and_refreshes_open_modal() {
    let mut app = App::new();
    let message_id = MessageId::new();
    app.modals.reply_queue.open_loading();

    handle_daemon_event(
        &mut app,
        DaemonEvent::ReminderTriggered {
            sent_message_id: message_id.clone(),
        },
    );

    assert!(
        app.mailbox.reply_later_message_ids.contains(&message_id),
        "TUI should show reminder-triggered messages as reply-later nudges"
    );
    assert!(
        app.pending_reply_queue_refresh,
        "open reply queue should refresh when a reminder fires"
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Reminder due; added to reply queue")
    );
}

#[test]
fn reply_queue_enter_starts_reply_compose_for_selected_message() {
    let mut app = App::new();
    let messages = make_test_envelopes(2);
    let selected = messages[1].clone();
    app.modals.reply_queue.open_loading();
    app.modals.reply_queue.set_messages(messages);
    app.modals.reply_queue.select_next();

    let action = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, Some(Action::ReplyQueueModalReply));
    app.apply(action.unwrap());

    assert!(!app.modals.reply_queue.visible);
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::Reply {
            message_id: selected.id,
            account_id: selected.account_id,
            preloaded: None,
        })
    );
}

#[test]
fn compose_blank_recipient_advances_to_subject_modal() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);

    assert!(app.compose.compose_picker.visible);
    assert_eq!(
        app.compose.compose_picker.mode,
        crate::ui::compose_picker::ComposePickerMode::To
    );

    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.compose.compose_picker.visible);
    assert_eq!(
        app.compose.compose_picker.mode,
        crate::ui::compose_picker::ComposePickerMode::Subject
    );
}

#[test]
fn compose_blank_subject_starts_new_compose_with_empty_fields() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::New {
            to: String::new(),
            subject: String::new(),
        })
    );
    assert!(!app.compose.compose_picker.visible);
}

#[test]
fn escape_closes_recipient_modal_without_starting_compose() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!app.compose.compose_picker.visible);
    assert!(app.compose.pending_compose.is_none());
    assert!(app.compose.compose_picker.pending_to.is_empty());
}

#[test]
fn escape_closes_subject_modal_without_starting_compose() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.apply(Action::Compose);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!app.compose.compose_picker.visible);
    assert!(app.compose.pending_compose.is_none());
    assert!(app.compose.compose_picker.pending_to.is_empty());
}

#[tokio::test]
async fn blank_recipient_draft_opens_draft_only_confirmation() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-test-missing-to-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let content = "---\nto: \"\"\ncc: \"\"\nbcc: \"\"\nsubject: Hello\nfrom: me@example.com\nattach: []\n---\n\nBody\n";
    std::fs::write(&temp, content).unwrap();

    let pending = pending_send_from_edited_draft(&ComposeReadyData {
        account_id: AccountId::new(),
        intent: mxr_core::DraftIntent::New,
        draft_path: temp.clone(),
        cursor_line: 1,
        initial_content: String::new(),
        invite_reply: None,
    })
    .await
    .unwrap();

    assert_eq!(pending.mode, PendingSendMode::DraftOnlyNoRecipients);

    let _ = std::fs::remove_file(temp);
}

#[test]
fn send_key_is_ignored_for_missing_recipient_draft_confirmation() {
    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
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

#[test]
fn save_key_saves_missing_recipient_draft_to_server() {
    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(1);
    app.compose.pending_send_confirm = Some(PendingSend {
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
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(matches!(
        app.pending_mutation_queue
            .first()
            .map(|queued| &queued.request),
        Some(Request::SaveDraftToServer { .. })
    ));
}

#[test]
fn edit_key_reopens_missing_recipient_draft() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-edit-draft-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "draft").unwrap();

    let mut app = App::new();
    let account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: account_id.clone(),
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
        },
        body: "Body".into(),
        draft_path: temp.clone(),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::EditDraft {
            path: temp.clone(),
            account_id,
        })
    );

    let _ = std::fs::remove_file(temp);
}

#[test]
fn escape_discards_missing_recipient_draft_confirmation_and_queues_cleanup() {
    let temp = std::env::temp_dir().join(format!(
        "mxr-compose-discard-draft-{}-{}.md",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    std::fs::write(&temp, "draft").unwrap();

    let mut app = App::new();
    app.compose.pending_send_confirm = Some(PendingSend {
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
        },
        body: "Body".into(),
        draft_path: temp.clone(),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::DraftOnlyNoRecipients,
        safety_report: None,
        safety_check_failed: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(app.compose.pending_send_confirm.is_none());
    assert!(temp.exists());
    assert_eq!(app.compose.pending_draft_cleanup, vec![temp.clone()]);
    assert_eq!(app.status_message.as_deref(), Some("Discarded"));

    let _ = std::fs::remove_file(temp);
}

#[test]
fn mail_list_l_opens_label_picker_not_message() {
    let mut app = App::new();
    app.mailbox.active_pane = ActivePane::MailList;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::ApplyLabel));
}

#[test]
fn input_gc_opens_config_editor() {
    let mut h = InputHandler::new();

    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
        Some(Action::EditConfig)
    );
}

#[test]
fn input_g_shift_l_opens_logs() {
    let mut h = InputHandler::new();

    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        h.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
        Some(Action::OpenLogs)
    );
}

#[test]
fn input_m_marks_read_and_archives() {
    let mut app = App::new();

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::MarkReadAndArchive));
}

#[test]
fn reconnect_detection_treats_connection_refused_as_recoverable() {
    let result = Err(MxrError::Ipc(
        "IPC error: Connection refused (os error 61)".into(),
    ));

    assert!(crate::ipc::should_reconnect_ipc(&result));
}

#[test]
fn autostart_detection_handles_refused_and_missing_socket() {
    let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
    let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

    assert!(crate::ipc::should_autostart_daemon(&refused));
    assert!(crate::ipc::should_autostart_daemon(&missing));
    assert!(!crate::ipc::should_autostart_daemon(&other));
}

#[test]
fn diagnostics_shift_l_opens_logs() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT));

    assert_eq!(action, Some(Action::OpenLogs));
}

#[test]
fn diagnostics_uppercase_l_opens_logs_without_shift_modifier() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenLogs));
}

#[test]
fn diagnostics_tab_cycles_selected_pane() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;

    let action = app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));

    assert!(action.is_none());
    assert_eq!(
        app.diagnostics.page.selected_pane,
        crate::app::DiagnosticsPaneKind::Data
    );
}

#[test]
fn diagnostics_enter_toggles_fullscreen_for_selected_pane() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;
    app.diagnostics.page.selected_pane = crate::app::DiagnosticsPaneKind::Logs;

    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .is_none());
    assert_eq!(
        app.diagnostics.page.fullscreen_pane,
        Some(crate::app::DiagnosticsPaneKind::Logs)
    );
    assert!(app
        .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .is_none());
    assert_eq!(app.diagnostics.page.fullscreen_pane, None);
}

#[test]
fn diagnostics_d_opens_selected_pane_details() {
    let mut app = App::new();
    app.screen = Screen::Diagnostics;
    app.diagnostics.page.selected_pane = crate::app::DiagnosticsPaneKind::Events;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert_eq!(action, Some(Action::OpenDiagnosticsPaneDetails));
}

#[test]
fn back_clears_selection_before_other_mail_list_back_behavior() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox
        .selected_set
        .insert(app.mailbox.envelopes[0].id.clone());

    app.apply(Action::Back);

    assert!(app.mailbox.selected_set.is_empty());
    assert_eq!(app.status_message.as_deref(), Some("Selection cleared"));
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
fn confirming_bulk_archive_queues_mutation_and_clears_selection() {
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

    app.apply(Action::OpenSelected);

    assert!(app.modals.pending_bulk_confirm.is_none());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert!(app.mailbox.selected_set.is_empty());
}

#[test]
fn command_palette_includes_major_mail_actions() {
    let labels: Vec<String> = default_commands()
        .into_iter()
        .map(|cmd| cmd.label)
        .collect();
    assert!(labels.contains(&"Reply".to_string()));
    assert!(labels.contains(&"Reply All".to_string()));
    assert!(labels.contains(&"Archive".to_string()));
    assert!(labels.contains(&"Delete".to_string()));
    assert!(labels.contains(&"Apply Label".to_string()));
    assert!(labels.contains(&"Snooze".to_string()));
    assert!(labels.contains(&"Clear Selection".to_string()));
    assert!(labels.contains(&"Open Accounts Page".to_string()));
    assert!(labels.contains(&"New IMAP/SMTP Account".to_string()));
    assert!(labels.contains(&"Set Default Account".to_string()));
    assert!(labels.contains(&"Edit Config".to_string()));
}

#[test]
fn local_label_changes_update_open_message() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);

    let user_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "Work")
        .unwrap()
        .clone();
    let message_id = app.mailbox.envelopes[0].id.clone();

    app.apply_local_label_refs(
        std::slice::from_ref(&message_id),
        std::slice::from_ref(&user_label.name),
        &[],
    );

    assert!(app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .label_provider_ids
        .contains(&user_label.provider_id));
}

#[test]
fn snooze_action_opens_modal_then_queues_request() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::Snooze);
    assert!(app.modals.snooze_panel.visible);

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

#[test]
fn open_selected_cache_miss_enters_loading_with_snippet_preview() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 0")
    ));
    assert!(app.mailbox.queued_body_fetches.is_empty());
    assert_eq!(
        app.mailbox.priority_body_fetches,
        vec![app.mailbox.envelopes[0].id.clone()]
    );
    assert!(app
        .mailbox
        .in_flight_body_requests
        .contains(&app.mailbox.envelopes[0].id));
}

#[test]
fn cached_plain_body_resolves_ready_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();

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

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready {
            ref raw,
            ref rendered,
            source: BodySource::Plain,
            ..
        } if raw.as_str() == "Plain body" && rendered.as_str() == "Plain body"
    ));
}

#[test]
fn cached_html_only_body_resolves_ready_state() {
    let mut app = App::new();
    app.mailbox.html_view = true;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();

    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: Some("<p>Hello html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );

    app.apply(Action::OpenSelected);

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready {
            ref raw,
            ref rendered,
            source: BodySource::Html,
            ref metadata,
        } if raw.as_str() == "<p>Hello html</p>"
            && rendered.as_str() == raw.as_str()
            && metadata.mode == crate::app::BodyViewMode::Html
    ));
}

/// Regression: the Deliveries screen handler used to end in `_ => None`,
/// swallowing every key except its own (j/k/r/d/D/g). That trapped the user
/// — no tab switch, no quit, no command palette — which read as a hang.
/// Global keys must fall through to the shared input handler.
#[test]
fn deliveries_screen_does_not_trap_global_navigation() {
    let mut app = App::new();
    app.screen = Screen::Deliveries;

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    assert_eq!(
        action,
        Some(Action::OpenTab1),
        "a tab digit must still switch screens from Deliveries"
    );
}

/// `o` on a delivery row queues opening its source thread in the mailbox.
#[test]
fn deliveries_o_opens_source_thread() {
    let mut app = App::new();
    let thread_id = ThreadId::new();
    let now = chrono::Utc::now();
    app.deliveries.rows = vec![mxr_protocol::DeliveryData {
        id: DeliveryId::new(),
        account_id: AccountId::new(),
        merchant: Some("Bamboocut".into()),
        carrier: Some("dhl".into()),
        tracking_number: None,
        tracking_url: None,
        order_number: Some("AIPD-1512-KL10".into()),
        status: "in_transit".into(),
        eta_from: None,
        eta_until: None,
        delivered_at: None,
        items: vec![],
        confidence: 0.9,
        source: "heuristic".into(),
        thread_id: Some(thread_id.clone()),
        last_event_at: now,
        created_at: now,
        updated_at: now,
        resolved_at: None,
        dismissed_at: None,
        message_ids: vec![],
    }];
    app.deliveries.selected = 0;
    app.screen = Screen::Deliveries;

    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert_eq!(app.pending_delivery_open, Some(thread_id));
}

/// Opening a delivery's source thread shows it inline in the split preview
/// and stays on the Deliveries screen (not the mailbox).
#[test]
fn open_delivery_thread_previews_inline_without_leaving_screen() {
    let mut app = App::new();
    app.screen = Screen::Deliveries;

    app.open_delivery_thread(make_test_envelopes(1));

    assert!(app.deliveries.preview_active);
    assert_eq!(app.screen, Screen::Deliveries);
    assert!(app.mailbox.viewing_envelope.is_some());
}

/// Esc closes the inline preview and drops the loaded message.
#[test]
fn deliveries_esc_closes_inline_preview() {
    let mut app = App::new();
    app.screen = Screen::Deliveries;
    app.open_delivery_thread(make_test_envelopes(1));
    assert!(app.deliveries.preview_active);

    let _ = app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(!app.deliveries.preview_active);
    assert!(app.mailbox.viewing_envelope.is_none());
}

// ── auth-session poller resilience ──────────────────────────────────────────

/// Helper to build a minimal AuthSessionData for poll testing.
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

/// Poller receives Err then Ok(terminal): AuthSession result still delivered.
#[tokio::test(start_paused = true)]
async fn auth_session_poller_retries_after_transient_error() {
    use crate::async_result::AsyncResult;
    use tokio::sync::mpsc;

    let (ipc_tx, mut ipc_rx) = fake_ipc_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    // Fake account config.
    let account = mxr_protocol::AccountConfigData {
        key: "test".into(),
        name: "Test".into(),
        email: "test@example.com".into(),
        enabled: true,
        sync: None,
        send: None,
        is_default: false,
    };

    // Respond to StartAuthSession with a pending session.
    let start_resp = session_response(pending_session());
    tokio::spawn(async move {
        // StartAuthSession
        let req = ipc_rx.recv().await.unwrap();
        let _ = req.reply.send(Ok(start_resp));
        tokio::time::advance(std::time::Duration::from_secs(2)).await;

        // First GetAuthSession: transient error
        let req = ipc_rx.recv().await.unwrap();
        let _ = req
            .reply
            .send(Err(mxr_core::MxrError::Ipc("transient".into())));
        tokio::time::advance(std::time::Duration::from_secs(2)).await;

        // Second GetAuthSession: success with terminal state
        let req = ipc_rx.recv().await.unwrap();
        let _ = req.reply.send(Ok(session_response(authorized_session())));
    });

    super::super::spawn_outlook_auth_session(&ipc_tx, result_tx, account, false).await;

    // Collect results: should contain an AuthSession (not just an AccountOperation error).
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

/// Poller receives 5 consecutive errors: AccountOperation(Err) is delivered.
#[tokio::test(start_paused = true)]
async fn auth_session_poller_aborts_after_max_consecutive_failures() {
    use crate::async_result::AsyncResult;
    use tokio::sync::mpsc;

    let (ipc_tx, mut ipc_rx) = fake_ipc_channel();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<AsyncResult>();

    let account = mxr_protocol::AccountConfigData {
        key: "test".into(),
        name: "Test".into(),
        email: "test@example.com".into(),
        enabled: true,
        sync: None,
        send: None,
        is_default: false,
    };

    tokio::spawn(async move {
        // StartAuthSession
        let req = ipc_rx.recv().await.unwrap();
        let _ = req.reply.send(Ok(session_response(pending_session())));

        // 5 consecutive GetAuthSession errors
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
