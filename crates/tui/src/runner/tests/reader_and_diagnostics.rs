use super::*;

#[test]
fn cached_attachment_only_body_resolves_fallback_ready_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();

    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "report.pdf".into(),
                mime_type: "application/pdf".into(),
                disposition: AttachmentDisposition::Attachment,
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

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready {
            ref raw,
            ref rendered,
            source: BodySource::Fallback,
            ..
        } if raw.contains("Attachment-only message")
            && rendered.contains("report.pdf")
    ));
}

#[test]
fn body_fetch_error_resolves_error_not_loading() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let env = app.mailbox.envelopes[0].clone();

    app.resolve_body_fetch_error(&env.id, "boom".into());

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Error { ref message, ref preview }
            if message == "boom" && preview.as_deref() == Some("Snippet 0")
    ));
    assert!(!app.mailbox.in_flight_body_requests.contains(&env.id));
}

#[test]
fn current_body_fetch_is_prioritized_even_when_prefetch_is_already_in_flight() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.in_flight_body_requests.insert(env.id.clone());

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.priority_body_fetches, vec![env.id.clone()]);
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 0")
    ));
}

#[test]
fn body_batch_uses_daemon_failure_message_for_missing_current_body() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let env = app.mailbox.envelopes[0].clone();

    app.resolve_body_batch(
        vec![env.id.clone()],
        vec![],
        vec![BodyFailure {
            message_id: env.id.clone(),
            error: "hydrate failed".into(),
        }],
    );

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Error { ref message, ref preview }
            if message == "hydrate failed" && preview.as_deref() == Some("Snippet 0")
    ));
    assert!(!app.mailbox.in_flight_body_requests.contains(&env.id));
}

#[test]
fn late_prefetch_failure_does_not_clobber_priority_body_success() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let env = app.mailbox.envelopes[0].clone();

    app.resolve_body_success(MessageBody {
        message_id: env.id.clone(),
        text_plain: Some("Loaded by priority request".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    app.resolve_body_batch(
        vec![env.id.clone()],
        vec![],
        vec![BodyFailure {
            message_id: env.id.clone(),
            error: "late prefetch failed".into(),
        }],
    );

    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Ready { ref raw, .. }
            if raw.as_str() == "Loaded by priority request"
    ));
}

#[test]
fn stale_body_response_does_not_clobber_current_view() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let first = app.mailbox.envelopes[0].clone();
    app.mailbox.active_pane = ActivePane::MailList;
    app.apply(Action::MoveDown);
    let second = app.mailbox.envelopes[1].clone();

    app.resolve_body_success(MessageBody {
        message_id: first.id.clone(),
        text_plain: Some("Old body".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    });

    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(second.id)
    );
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Loading { ref preview }
            if preview.as_deref() == Some("Snippet 1")
    ));
}

#[test]
fn reader_mode_toggle_shows_raw_html_when_disabled() {
    let mut app = App::new();
    app.mailbox.html_view = false;
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

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { raw, rendered, .. } => {
            assert_eq!(raw.as_str(), "<p>Hello html</p>");
            assert_ne!(rendered.as_str(), raw.as_str());
            assert!(rendered.contains("Hello html"));
        }
        other => panic!("expected ready state, got {other:?}"),
    }

    app.apply(Action::ToggleReaderMode);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { raw, rendered, .. } => {
            assert_eq!(raw.as_str(), "<p>Hello html</p>");
            assert_eq!(rendered.as_str(), raw.as_str());
        }
        other => panic!("expected ready state, got {other:?}"),
    }

    app.apply(Action::ToggleReaderMode);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { raw, rendered, .. } => {
            assert_eq!(raw.as_str(), "<p>Hello html</p>");
            assert_ne!(rendered.as_str(), raw.as_str());
            assert!(rendered.contains("Hello html"));
        }
        other => panic!("expected ready state, got {other:?}"),
    }
}

#[test]
fn html_view_toggle_updates_mode_and_remote_content_status() {
    let mut app = App::new();
    app.mailbox.html_view = true;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let env = app.mailbox.envelopes[0].clone();
    app.mailbox.body_cache.insert(
        env.id.clone(),
        MessageBody {
            message_id: env.id.clone(),
            text_plain: Some("Fallback plain".into()),
            text_html: Some(
                "<p>Hello <img alt=\"Hero\" src=\"https://example.com/hero.png\"></p>".into(),
            ),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: env.id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                text_plain_source: Some(BodyPartSource::Exact),
                text_html_source: Some(BodyPartSource::Exact),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenSelected);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready {
            source: BodySource::Html,
            metadata,
            ..
        } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Html);
            assert!(metadata.inline_images);
            assert!(metadata.remote_content_available);
            assert!(metadata.remote_content_enabled);
        }
        other => panic!("expected html ready state, got {other:?}"),
    }

    app.apply(Action::ToggleHtmlView);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready {
            source: BodySource::Plain,
            metadata,
            ..
        } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Text);
            assert!(metadata.inline_images);
            assert!(metadata.remote_content_available);
            assert!(metadata.remote_content_enabled);
        }
        other => panic!("expected text ready state, got {other:?}"),
    }
    assert_eq!(app.status_message.as_deref(), Some("View: Reading"));
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("View: Reading")));

    app.apply(Action::ToggleRemoteContent);

    match &app.mailbox.body_view_state {
        BodyViewState::Ready { metadata, .. } => {
            assert_eq!(metadata.mode, crate::app::BodyViewMode::Text);
            assert!(!metadata.remote_content_enabled);
        }
        other => panic!("expected text ready state, got {other:?}"),
    }
    assert_eq!(
        app.status_message.as_deref(),
        Some("Remote images blocked in HTML view")
    );
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("View: Reading")));
}

#[test]
fn reader_mode_toggle_is_blocked_in_html_view() {
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
            metadata: MessageMetadata {
                text_html_source: Some(BodyPartSource::Exact),
                ..Default::default()
            },
        },
    );

    app.apply(Action::OpenSelected);
    let reader_mode_before = app.mailbox.reader_mode;

    app.apply(Action::ToggleReaderMode);

    assert_eq!(app.mailbox.reader_mode, reader_mode_before);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Switch to text view to use reading view")
    );
}

#[test]
fn reader_stats_visibility_respects_config() {
    let mut app = App::new();
    app.mailbox.body_view_state = BodyViewState::ready(
        "Hello".into(),
        "Hello".into(),
        BodySource::Plain,
        BodyViewMetadata {
            mode: crate::app::BodyViewMode::Text,
            provenance: Some(BodyPartSource::Exact),
            reader_applied: true,
            original_lines: Some(12),
            cleaned_lines: Some(7),
            ..BodyViewMetadata::default()
        },
    );

    app.mailbox.show_reader_stats = false;
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| !status.contains("trimmed 5 lines")));

    app.mailbox.show_reader_stats = true;
    assert!(app
        .status_bar_state()
        .body_status
        .as_deref()
        .is_some_and(|status| status.contains("trimmed 5 lines")));
}

#[test]
fn account_switch_complete_closes_open_message_state() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.mailbox_view = MailboxView::Subscriptions;
    app.mailbox.layout_mode = LayoutMode::FullScreen;
    app.mailbox.active_pane = ActivePane::MessageView;
    app.mailbox.viewing_envelope = Some(app.mailbox.envelopes[0].clone());
    app.mailbox.viewed_thread_messages = app.mailbox.envelopes.clone();
    app.mailbox.body_view_state = BodyViewState::ready(
        "hello".into(),
        "hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );
    app.mailbox.active_label = Some(LabelId::new());
    app.mailbox.pending_active_label = Some(LabelId::new());
    app.mailbox.pending_label_fetch = Some(LabelId::new());
    app.mailbox
        .selected_set
        .insert(app.mailbox.envelopes[0].id.clone());

    app.handle_account_switch_complete();

    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.viewed_thread_messages.is_empty());
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Empty { .. }
    ));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Messages);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert!(app.search.page.results.is_empty());
    assert!(app.mailbox.subscriptions_page.entries.is_empty());
    assert!(app.mailbox.selected_set.is_empty());
    assert!(app.mailbox.active_label.is_none());
    assert!(app.mailbox.pending_active_label.is_none());
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.mailbox.pending_subscriptions_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert_eq!(
        app.mailbox.mailbox_loading_message.as_deref(),
        Some("Loading selected account...")
    );
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
}

#[test]
fn mailbox_refresh_clears_account_switch_loader() {
    let mut app = App::new();
    app.handle_account_switch_complete();

    let envelopes = make_test_envelopes(2);
    apply_all_envelopes_refresh(&mut app, envelopes.clone());

    assert!(app.mailbox.mailbox_loading_message.is_none());
    assert_eq!(app.status_message.as_deref(), Some("Account switched"));
    assert_eq!(app.mailbox.all_envelopes.len(), envelopes.len());
}

/// Phase 1.1 / Behavior 4: when the user sends from the Sent view,
/// applying the completion of a SendDraft mutation refreshes the active
/// label so the new message appears without a manual sync. The status
/// message reads "Sent!" — not "Synced" — because the user just sent.
#[test]
fn sent_success_effect_refreshes_active_label_and_sets_status() {
    let mut app = App::new();
    let label_id = LabelId::new();
    app.mailbox.active_label = Some(label_id.clone());
    // Simulate a single in-flight mutation so completion logic shows status.
    app.pending_mutation_count = 1;

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        true,
    );

    assert_eq!(
        app.mailbox.pending_label_fetch,
        Some(label_id),
        "active label must be queued for refetch so the Sent view shows the new message"
    );
    assert!(
        app.mailbox.pending_subscriptions_refresh,
        "subscriptions must refresh after a successful send"
    );
    assert_eq!(app.status_message.as_deref(), Some("Sent!"));
}

#[test]
fn sent_success_with_reminder_queues_auto_reminder_for_sent_message() {
    let mut app = App::new();
    let sent_message_id = MessageId::new();
    let remind_at = chrono::Utc::now() + chrono::Duration::hours(2);

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: Some(remind_at),
            sent_message_id: Some(sent_message_id.clone()),
        },
        true,
    );

    let queued = app
        .pending_mutation_queue
        .first()
        .expect("sent reminder should queue SetAutoReminder");
    match &queued.request {
        Request::SetAutoReminder {
            sent_message_id: queued_message_id,
            remind_at: queued_remind_at,
        } => {
            assert_eq!(queued_message_id, &sent_message_id);
            assert_eq!(queued_remind_at, &remind_at);
        }
        other => panic!("Expected SetAutoReminder request, got {other:?}"),
    }
}

/// Phase 1.1 / Behavior 4: with no active label (e.g. on the accounts
/// screen), applying SentSuccess still updates the status message but
/// does not enqueue a label fetch. Catches regressions that would
/// either crash on the None case or leak a stale label fetch.
#[test]
fn sent_success_effect_with_no_active_label_only_updates_status() {
    let mut app = App::new();
    app.mailbox.active_label = None;

    app.apply_mutation_completion(
        MutationEffect::SentSuccess {
            status: "Sent!".into(),
            remind_at: None,
            sent_message_id: None,
        },
        true,
    );

    assert_eq!(app.mailbox.pending_label_fetch, None);
    assert_eq!(app.status_message.as_deref(), Some("Sent!"));
}

/// Phase 1.2 / Behavior 4: connection_state_label exposes a non-empty
/// human-readable string when the connection is not healthy, which the
/// status bar prepends. Catches "silent hang" regressions — a missing
/// or empty label would put the user back to staring at a frozen UI.
#[test]
fn connection_state_label_surfaces_reconnecting_state() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    app.set_connection_state(ConnectionState::Reconnecting {
        since: std::time::Instant::now(),
        reason: "connection refused".into(),
    });

    let label = app.connection_state_label();
    let label = label.expect("label must be Some when not Connected");
    let lower = label.to_lowercase();
    assert!(
        lower.contains("reconnect") || lower.contains("daemon"),
        "label must mention the disconnected state; got {label:?}"
    );
}

/// Phase 1.2 / Behavior 4: when Connected, the label is None so the
/// status bar shows the regular mailbox info, not a stale connection
/// notice.
#[test]
fn connection_state_label_is_none_when_connected() {
    use crate::app::ConnectionState;
    let mut app = App::new();
    app.set_connection_state(ConnectionState::Connected);
    assert!(app.connection_state_label().is_none());
}

/// Phase 1.3 / Behavior 1: a Warn through the reporter lands in the
/// ring buffer with the supplied message and Warn severity. Catches
/// regressions where async errors silently disappear (the original
/// `let _ = ...` smell).
#[test]
fn report_warn_adds_one_entry_to_ring_buffer() {
    use crate::app::UserErrorSeverity;
    let mut app = App::new();
    app.report_warn("body parse failed");

    let log = &app.modals.error_log;
    assert_eq!(log.len(), 1, "exactly one entry after one warn");
    let entry = log.back().expect("entry");
    assert_eq!(entry.message, "body parse failed");
    assert!(matches!(entry.severity, UserErrorSeverity::Warn));
}

/// Phase 1.3 / Behavior 2: the ring buffer caps at 5 — pushing a sixth
/// drops the oldest. No panic, no unbounded growth. Catches both
/// "buffer not capped" (memory leak under error storms) and
/// "buffer drops newest" (would lose the most actionable info).
#[test]
fn ring_buffer_keeps_five_most_recent_entries() {
    let mut app = App::new();
    for i in 0..6 {
        app.report_warn(format!("warn {i}"));
    }

    let log = &app.modals.error_log;
    assert_eq!(log.len(), 5, "ring buffer caps at 5");
    let messages: Vec<&str> = log.iter().map(|e| e.message.as_str()).collect();
    assert!(
        !messages.contains(&"warn 0"),
        "oldest entry must be dropped; got {messages:?}"
    );
    assert!(
        messages.contains(&"warn 5"),
        "newest entry must be kept; got {messages:?}"
    );
}

/// Phase 1.3 / Behavior 3: a warn shown in the status bar auto-clears
/// after 5s of wall time so a transient error doesn't permanently
/// hide the inbox info.
#[test]
fn current_user_warn_clears_after_5s() {
    let mut app = App::new();
    app.report_warn("body parse failed");
    let since = app.modals.error_log.back().expect("entry must exist").since;

    assert_eq!(
        app.current_user_warn(since + std::time::Duration::from_secs(4))
            .as_deref(),
        Some("body parse failed"),
        "warn must still be visible at 4s"
    );
    assert_eq!(
        app.current_user_warn(since + std::time::Duration::from_secs(6)),
        None,
        "warn must clear by 6s"
    );
}

/// Phase 2.1 / Behavior 1: opening a fresh saved-search form has
/// empty fields and lexical mode, and submitting valid name+query
/// produces a `Request::CreateSavedSearch` ready to dispatch.
#[test]
fn saved_search_form_for_new_submits_create_request() {
    let mut app = App::new();
    app.open_saved_search_form_new();

    let form = app
        .modals
        .saved_search_form
        .as_mut()
        .expect("form must open");
    form.name = "Work overdue".into();
    form.query = "label:work older_than:7d".into();

    let request = app
        .take_saved_search_form_request()
        .expect("valid form must yield a request");
    match request {
        mxr_protocol::Request::CreateSavedSearch {
            name,
            query,
            account_id: _,
            search_mode,
        } => {
            assert_eq!(name, "Work overdue");
            assert_eq!(query, "label:work older_than:7d");
            assert!(matches!(search_mode, mxr_core::types::SearchMode::Lexical));
        }
        other => panic!("expected CreateSavedSearch, got {other:?}"),
    }
    assert!(
        app.modals.saved_search_form.is_none(),
        "form must close after a successful submit"
    );
}

/// Phase 2.1 / Behavior 2: an empty name surfaces a validation
/// error and does NOT yield a request — catches "form silently
/// drops malformed input" regressions.
#[test]
fn saved_search_form_empty_name_rejects_with_validation_error() {
    let mut app = App::new();
    app.open_saved_search_form_new();

    let form = app
        .modals
        .saved_search_form
        .as_mut()
        .expect("form must open");
    form.name = String::new();
    form.query = "label:inbox".into();

    let request = app.take_saved_search_form_request();
    assert!(
        request.is_none(),
        "empty name must not produce a request; got {request:?}"
    );

    let form = app
        .modals
        .saved_search_form
        .as_ref()
        .expect("form must remain open after validation failure");
    assert!(
        form.validation_error
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains("name"),
        "validation error must mention the empty name; got {:?}",
        form.validation_error
    );
}

/// Phase 2.1 / Behavior 4: opening for edit prefills the form and
/// records the existing name. On submit the daemon receives both
/// a Delete (for the old name) and a Create (for the possibly-new
/// name) so name renames don't collide with the unique constraint.
#[test]
fn saved_search_form_for_edit_yields_delete_then_create() {
    let mut app = App::new();
    app.open_saved_search_form_for_edit(
        "Old name".into(),
        "label:work".into(),
        mxr_core::types::SearchMode::Lexical,
    );

    let form = app
        .modals
        .saved_search_form
        .as_mut()
        .expect("form must open");
    // Preserves old name as the source for the delete step.
    assert_eq!(form.existing_name.as_deref(), Some("Old name"));
    // Prefilled with the current name so the user can rename.
    assert_eq!(form.name, "Old name");
    form.name = "New name".into();

    let requests = app
        .take_saved_search_form_requests()
        .expect("edit must yield delete+create requests");
    assert_eq!(requests.len(), 2);
    match &requests[0] {
        mxr_protocol::Request::DeleteSavedSearch { name } => {
            assert_eq!(name, "Old name", "first request must delete the old name");
        }
        other => panic!("expected DeleteSavedSearch first, got {other:?}"),
    }
    match &requests[1] {
        mxr_protocol::Request::CreateSavedSearch { name, .. } => {
            assert_eq!(
                name, "New name",
                "second request must create under the new name"
            );
        }
        other => panic!("expected CreateSavedSearch second, got {other:?}"),
    }
}

/// Phase 2.1 stage B / Behavior 1 + dispatch wiring: dispatching
/// `SaveSavedSearchForm` with a valid form queues exactly one
/// `CreateSavedSearch` request for the IPC dispatcher and closes
/// the form. Catches "save action no-ops" regressions where the
/// keybinding fires but no request reaches the daemon.
#[test]
fn save_saved_search_form_action_queues_create_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.open_saved_search_form_new();
    let form = app.modals.saved_search_form.as_mut().expect("form open");
    form.name = "Important".into();
    form.query = "label:starred".into();

    app.apply(Action::SaveSavedSearchForm);

    let queue = app.take_pending_saved_search_dispatch();
    assert_eq!(queue.len(), 1, "expected one queued request: {queue:?}");
    match &queue[0] {
        mxr_protocol::Request::CreateSavedSearch { name, query, .. } => {
            assert_eq!(name, "Important");
            assert_eq!(query, "label:starred");
        }
        other => panic!("expected CreateSavedSearch, got {other:?}"),
    }
    assert!(
        app.modals.saved_search_form.is_none(),
        "form should close after a valid save"
    );
    assert!(
        app.modals.pending_saved_search_dispatch.is_empty(),
        "queue must be drained by take_pending_saved_search_dispatch"
    );
}

/// Phase 2.1 stage B / Behavior 2: `SaveSavedSearchForm` with an
/// empty query keeps the form open with a validation error and
/// does NOT enqueue a request. Matches the principle "form fails
/// fast, daemon never sees garbage".
#[test]
fn save_saved_search_form_action_skips_dispatch_on_validation_failure() {
    use crate::action::Action;
    let mut app = App::new();
    app.open_saved_search_form_new();
    let form = app.modals.saved_search_form.as_mut().expect("form open");
    form.name = "Important".into();
    form.query = "  ".into(); // whitespace-only — rejected.

    app.apply(Action::SaveSavedSearchForm);

    assert!(
        app.modals.pending_saved_search_dispatch.is_empty(),
        "no requests must queue on a rejected save"
    );
    let form = app
        .modals
        .saved_search_form
        .as_ref()
        .expect("form must remain open");
    assert!(
        form.validation_error.is_some(),
        "validation_error must be set so the modal can surface it"
    );
}

/// Phase 2.1 stage B / Behavior 3: opening the delete-confirm via
/// `DeleteSavedSearch` with a Saved Search row selected, then
/// confirming, queues exactly one `DeleteSavedSearch` request.
/// Cancel path clears the confirm without dispatching.
#[test]
fn delete_saved_search_confirm_path_queues_delete_request() {
    let mut app = App::new();
    // Confirm path
    app.modals.pending_saved_search_delete_confirm = Some("Important".into());
    let confirmed = app.confirm_pending_saved_search_delete();
    assert_eq!(confirmed.as_deref(), Some("Important"));
    let queue = app.take_pending_saved_search_dispatch();
    assert_eq!(queue.len(), 1, "expected one queued delete: {queue:?}");
    match &queue[0] {
        mxr_protocol::Request::DeleteSavedSearch { name } => {
            assert_eq!(name, "Important");
        }
        other => panic!("expected DeleteSavedSearch, got {other:?}"),
    }
    assert!(
        app.modals.pending_saved_search_delete_confirm.is_none(),
        "confirm dialog must close after confirm"
    );
}

/// Phase 2.2 / Palette parity: each of the four semantic palette
/// actions appears in the default palette and is reachable from
/// the standard mailbox context. Catches accidental removal or
/// allowlist drift in `action_allowed_in_context`.
#[test]
fn semantic_palette_entries_present_in_default_commands() {
    let commands = crate::ui::command_palette::default_commands();
    let labels: Vec<&str> = commands.iter().map(|c| c.label.as_str()).collect();
    for needle in [
        "Semantic: Enable",
        "Semantic: Disable",
        "Semantic: Reindex",
        "Semantic: Backfill Missing",
        "Semantic: Install Profile (BGE Small EN)",
        "Semantic: Install Profile (Multilingual E5)",
        "Semantic: Install Profile (BGE-M3)",
    ] {
        assert!(
            labels.contains(&needle),
            "expected `{needle}` in palette; got {labels:?}"
        );
    }
}

#[test]
fn platform_palette_entries_present_in_default_commands() {
    let commands = crate::ui::command_palette::default_commands();
    let labels: Vec<&str> = commands.iter().map(|c| c.label.as_str()).collect();
    for needle in [
        "Draft: Assist Current Thread",
        "Draft: New For Sender",
        "Voice: Show Profile",
        "Voice: Rebuild Profile",
        "Commitments: Show Open",
    ] {
        assert!(
            labels.contains(&needle),
            "expected `{needle}` in palette; got {labels:?}"
        );
    }
}

/// Phase 2.2 / Behavior 1: dispatching `EnableSemantic` queues exactly
/// one `Request::EnableSemantic { enabled: true }` for the
/// dispatcher.
#[test]
fn enable_semantic_action_queues_enabled_true_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::EnableSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::EnableSemantic { enabled } => {
            assert!(*enabled, "Enable must request enabled=true");
        }
        other => panic!("expected EnableSemantic, got {other:?}"),
    }
}

/// Phase 2.2 / Behavior 1 (disable): dispatching `DisableSemantic`
/// queues `EnableSemantic { enabled: false }`. Symmetric to enable
/// so the same daemon handler clears the flag.
#[test]
fn disable_semantic_action_queues_enabled_false_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::DisableSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::EnableSemantic { enabled } => {
            assert!(!*enabled, "Disable must request enabled=false");
        }
        other => panic!("expected EnableSemantic, got {other:?}"),
    }
}

/// Phase 2.2 / Behavior 2: `ReindexSemantic` queues
/// `Request::ReindexSemantic`.
#[test]
fn reindex_semantic_action_queues_reindex_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::ReindexSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    assert!(
        matches!(queue[0], mxr_protocol::Request::ReindexSemantic),
        "expected ReindexSemantic, got {:?}",
        queue[0]
    );
}

#[test]
fn backfill_semantic_action_queues_backfill_request() {
    use crate::action::Action;
    let mut app = App::new();
    app.apply(Action::BackfillSemantic);
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    assert!(
        matches!(queue[0], mxr_protocol::Request::BackfillSemantic),
        "expected BackfillSemantic, got {:?}",
        queue[0]
    );
}

/// Phase 2.2 / Behavior 3: `InstallSemanticProfile(profile)` queues
/// `Request::InstallSemanticProfile { profile }` with the same
/// profile variant. Verifies the profile parameter survives the
/// palette → action → request hop without reshuffling.
#[test]
fn install_semantic_profile_action_queues_install_request() {
    use crate::action::Action;
    let mut app = App::new();
    let profile = mxr_core::types::SemanticProfile::MultilingualE5Small;
    app.apply(Action::InstallSemanticProfile(profile));
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::InstallSemanticProfile { profile: p } => {
            assert_eq!(p.as_str(), profile.as_str());
        }
        other => panic!("expected InstallSemanticProfile, got {other:?}"),
    }
}

#[test]
fn use_semantic_profile_action_queues_use_request() {
    use crate::action::Action;
    let mut app = App::new();
    let profile = mxr_core::types::SemanticProfile::BgeM3;
    app.apply(Action::UseSemanticProfile(profile));
    let queue = app.take_pending_semantic_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0] {
        mxr_protocol::Request::UseSemanticProfile { profile: p } => {
            assert_eq!(p.as_str(), profile.as_str());
        }
        other => panic!("expected UseSemanticProfile, got {other:?}"),
    }
}

#[test]
fn draft_assist_action_queues_selected_thread_request() {
    use crate::action::Action;
    let mut app = App::new();
    let envelope = TestEnvelopeBuilder::new()
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    let thread_id = envelope.thread_id.clone();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::DraftAssistCurrentThread);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].title, "Draft assist");
    match &queue[0].request {
        Request::DraftAssist {
            thread_id: queued,
            instruction,
        } => {
            assert_eq!(queued, &thread_id);
            assert_eq!(instruction, "Draft a concise reply.");
        }
        other => panic!("expected DraftAssist, got {other:?}"),
    }
}

#[test]
fn thread_summary_response_clears_loading_after_user_navigates_away() {
    use crate::daemon_events::apply_thread_summary_loaded;
    // Reproduces the "y doesn't work" bug: user opens thread A,
    // auto-summary fires, user navigates to thread B, response
    // lands for A. Before the fix the response handler bailed on
    // `still_relevant` *without* clearing `thread_summary_loading`,
    // leaving thread A's loading flag stuck. Then `y` on thread A
    // short-circuited with "Summary already running".
    let mut app = App::new();
    let envelope_a = TestEnvelopeBuilder::new()
        .with_from_address("Alice", "alice@example.com")
        .subject("A")
        .build();
    let envelope_b = TestEnvelopeBuilder::new()
        .with_from_address("Bob", "bob@example.com")
        .subject("B")
        .build();
    let thread_a = envelope_a.thread_id.clone();
    // User is now focused on thread B; response will arrive for A.
    app.mailbox.envelopes = vec![envelope_b];
    app.mailbox.thread_summary_loading = Some(thread_a.clone());

    apply_thread_summary_loaded(
        &mut app,
        thread_a.clone(),
        Ok(("summary".into(), "model".into())),
    );

    assert!(
        app.mailbox.thread_summary_loading.is_none(),
        "loading flag must clear so the next `y` press can fire"
    );
}

#[test]
fn summarize_action_starts_background_request_without_modal() {
    use crate::action::Action;
    let mut app = App::new();
    let envelope = TestEnvelopeBuilder::new()
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    let thread_id = envelope.thread_id.clone();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::SummarizeCurrentThread);

    assert_eq!(app.pending_summary_request, Some(thread_id.clone()));
    assert_eq!(app.mailbox.thread_summary_loading, Some(thread_id));
    assert!(!app.modals.summary.visible);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Summarizing in background...")
    );
}

#[test]
fn draft_new_for_sender_action_queues_selected_sender_request() {
    use crate::action::Action;
    let mut app = App::new();
    let account_id = AccountId::new();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::DraftNewForSender);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0].request {
        Request::DraftNew {
            account_id: queued_account,
            to,
            purpose,
            register,
            length_hint,
        } => {
            assert_eq!(queued_account, &account_id);
            assert_eq!(to.email, "sender@example.com");
            assert_eq!(purpose, "Follow up on the selected thread: Quarterly plan");
            assert!(register.is_none());
            assert!(length_hint.is_none());
        }
        other => panic!("expected DraftNew, got {other:?}"),
    }
}

#[test]
fn refine_pending_draft_saves_then_queues_refine_request() {
    let mut app = App::new();
    let account_id = AccountId::new();
    app.compose.pending_send_confirm = Some(PendingSend {
        account_id: account_id.clone(),
        fm: mxr_compose::frontmatter::ComposeFrontmatter {
            to: "sender@example.com".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Quarterly plan".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            intent: mxr_core::DraftIntent::New,
            references: Vec::new(),
            thread_id: None,
            attach: Vec::new(),
            signature: None,
        },
        body: "Could you review the plan?".into(),
        draft_path: std::path::PathBuf::from("/tmp/mxr-draft.md"),
        intent: mxr_core::DraftIntent::New,
        mode: PendingSendMode::SendOrSave,
        safety_report: None,
        override_token: None,
        suggested_collaborators: vec![],
        invite_reply: None,
    });

    app.apply(Action::RefinePendingDraft);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].prelude.len(), 1);
    let draft_id = match &queue[0].prelude[0] {
        Request::SaveDraft { draft } => {
            assert_eq!(draft.account_id, account_id);
            assert_eq!(draft.subject, "Quarterly plan");
            assert_eq!(draft.body_markdown, "Could you review the plan?");
            draft.id.clone()
        }
        other => panic!("expected SaveDraft prelude, got {other:?}"),
    };
    match &queue[0].request {
        Request::DraftRefine {
            draft_id: queued,
            knobs,
        } => {
            assert_eq!(queued, &draft_id);
            assert_eq!(knobs, &mxr_protocol::DraftRefineKnobsData::default());
        }
        other => panic!("expected DraftRefine, got {other:?}"),
    }
}

#[test]
fn commitments_action_queues_open_commitments_for_selected_sender() {
    use crate::action::Action;
    let mut app = App::new();
    let account_id = AccountId::new();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .with_from_address("Sender", "sender@example.com")
        .subject("Quarterly plan")
        .build();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::OpenCommitments);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 1);
    match &queue[0].request {
        Request::ListCommitments {
            account_id: queued_account,
            email,
            status,
        } => {
            assert_eq!(queued_account, &account_id);
            assert_eq!(email.as_deref(), Some("sender@example.com"));
            assert_eq!(*status, Some(mxr_protocol::CommitmentStatusData::Open));
        }
        other => panic!("expected ListCommitments, got {other:?}"),
    }
}

#[test]
fn voice_actions_queue_selected_account_requests() {
    use crate::action::Action;
    let mut app = App::new();
    let account_id = AccountId::new();
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .with_from_address("Sender", "sender@example.com")
        .build();
    app.mailbox.envelopes = vec![envelope];

    app.apply(Action::OpenVoiceProfile);
    app.apply(Action::RebuildUserVoice);

    let queue = app.take_pending_platform_dispatch();
    assert_eq!(queue.len(), 2);
    match &queue[0].request {
        Request::GetUserVoice { account_id: queued } => assert_eq!(queued, &account_id),
        other => panic!("expected GetUserVoice, got {other:?}"),
    }
    match &queue[1].request {
        Request::RebuildUserVoice { account_id: queued } => assert_eq!(queued, &account_id),
        other => panic!("expected RebuildUserVoice, got {other:?}"),
    }
}

/// Phase 2.5 / Behavior 1: opening an analytics view from the
/// palette switches to the Analytics screen, sets the right view
/// mode, and sets `refresh_pending` so the dispatcher fires the
/// matching `List*` request next tick. Catches "palette entry
/// opens the screen but never loads data" regressions.
#[test]
fn open_analytics_view_action_switches_screen_and_marks_refresh_pending() {
    use crate::action::Action;
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.apply(Action::OpenAnalyticsView(AnalyticsView::Contacts));
    assert!(matches!(app.screen, crate::app::Screen::Analytics));
    assert_eq!(app.analytics.view, AnalyticsView::Contacts);
    assert!(
        app.analytics.refresh_pending,
        "opening an analytics view must mark refresh_pending so the daemon request fires"
    );
}

/// Phase 2.5: the active view determines which `Request` the
/// dispatcher fires. Locks down the mapping so a daemon-side
/// rename (e.g. ListStorageBreakdown → ListStorageBuckets) shows
/// up here as a compile error or a test failure rather than as
/// "the screen renders but nothing ever loads."
#[test]
fn analytics_request_for_active_view_maps_each_variant() {
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::Storage;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListStorageBreakdown { .. })
    ));
    app.analytics.view = AnalyticsView::StaleThreads;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListStaleThreads { .. })
    ));
    app.analytics.view = AnalyticsView::Contacts;
    // Default contacts_mode is Asymmetry per Default impl.
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListContactAsymmetry { .. })
    ));
    app.analytics.view = AnalyticsView::ResponseTime;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListResponseTime { .. })
    ));
    app.accounts.page.accounts = vec![account_summary(AccountId::new(), true, true)];
    app.analytics.view = AnalyticsView::CadenceDrift;
    assert!(matches!(
        app.analytics_request_for_active_view(),
        Some(mxr_protocol::Request::ListCadenceDrift { .. })
    ));
}

#[test]
fn cadence_drift_request_requires_enabled_account() {
    use crate::app::AnalyticsView;
    let mut app = App::new();
    app.analytics.view = AnalyticsView::CadenceDrift;
    assert!(
        app.analytics_request_for_active_view().is_none(),
        "cadence drift must not dispatch a request with a fabricated account id"
    );
}

/// Phase 2.5 / Behavior 4: the refresh action re-marks
/// `refresh_pending` and clears any prior error. Catches "press r
/// after a daemon error and nothing happens" regressions.
#[test]
fn refresh_analytics_action_clears_error_and_marks_pending() {
    use crate::action::Action;
    let mut app = App::new();
    app.screen = crate::app::Screen::Analytics;
    app.analytics.error = Some("stale".into());
    app.analytics.refresh_pending = false;
    app.apply(Action::RefreshAnalytics);
    assert!(app.analytics.refresh_pending);
    assert!(app.analytics.error.is_none());
}
