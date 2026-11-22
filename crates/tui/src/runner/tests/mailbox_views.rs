use super::*;

#[test]
fn sidebar_system_labels_in_correct_order() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let system_names: Vec<&str> = ordered
        .iter()
        .filter(|l| l.kind == LabelKind::System)
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(system_names[0], "INBOX");
    assert_eq!(system_names[1], "STARRED");
    assert_eq!(system_names[2], "SENT");
    assert_eq!(system_names[3], "DRAFT");
    assert_eq!(system_names[4], "ARCHIVE");
}

#[test]
fn sidebar_empty_system_labels_hidden_except_primary() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
    assert!(
        !names.contains(&"CHAT"),
        "Empty non-primary system labels should be hidden"
    );
    assert!(
        names.contains(&"DRAFT"),
        "Primary system labels shown even if empty"
    );
    assert!(
        names.contains(&"ARCHIVE"),
        "Archive should be shown as a primary system label even if empty"
    );
    assert!(
        names.contains(&"IMPORTANT"),
        "Non-empty system labels should be shown"
    );
}

#[test]
fn goto_inbox_sets_active_label() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.labels = make_test_labels();
    app.apply(Action::GoToInbox);
    let label = app
        .mailbox
        .labels
        .iter()
        .find(|l| l.name == "INBOX")
        .unwrap();
    assert!(
        app.mailbox.active_label.is_none(),
        "GoToInbox should wait for fetch success before swapping active label"
    );
    assert_eq!(
        app.mailbox.pending_active_label.as_ref().unwrap(),
        &label.id
    );
    assert!(
        app.mailbox.pending_label_fetch.is_some(),
        "Should trigger label fetch"
    );
}

#[test]
fn g_hash_and_g_bang_open_the_trash_and_spam_labels() {
    for (key, name) in [('#', "TRASH"), ('!', "SPAM")] {
        let mut app = App::new();
        app.mailbox.labels = make_test_labels();
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            None
        );
        let action = app.handle_key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::SHIFT));
        app.apply(action.expect("g then a symbol goes somewhere"));
        let label = app.mailbox.labels.iter().find(|l| l.name == name).unwrap();
        assert_eq!(app.mailbox.pending_active_label.as_ref(), Some(&label.id));
    }
}

#[test]
fn labels_refresh_resolves_desired_inbox() {
    let mut app = App::new();
    app.mailbox.desired_system_mailbox = Some("INBOX".into());
    app.mailbox.labels = make_test_labels();

    app.resolve_desired_system_mailbox();

    let inbox_id = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap()
        .id
        .clone();
    assert_eq!(app.mailbox.pending_active_label.as_ref(), Some(&inbox_id));
    assert_eq!(app.mailbox.pending_label_fetch.as_ref(), Some(&inbox_id));
    assert!(app.mailbox.active_label.is_none());
}

#[test]
fn sync_completed_requests_live_refresh_even_without_active_label() {
    let mut app = App::new();

    handle_daemon_event(
        &mut app,
        DaemonEvent::SyncCompleted {
            account_id: AccountId::new(),
            messages_synced: 5,
        },
    );

    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert_eq!(app.status_message.as_deref(), Some("Synced 5 messages"));
}

#[test]
fn mutation_reconciliation_failed_event_replays_optimistic_snapshot() {
    let mut app = App::new();
    let envelopes = make_test_envelopes(1);
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    app.mailbox.selected_index = 0;

    app.apply(Action::Star);
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "star applies optimistically"
    );
    let mid = app.pending_mutation_queue[0].id;

    handle_daemon_event(
        &mut app,
        DaemonEvent::MutationReconciliationFailed {
            client_correlation_id: mid.raw().to_string(),
            error_summary: "provider rejected".into(),
        },
    );

    assert!(
        !app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "daemon failure event rolls back starred state"
    );
    let now = std::time::Instant::now();
    let toasts = app.toasts.visible(now);
    assert!(
        toasts
            .iter()
            .any(|toast| toast.text == "Mutation failed: provider rejected"),
        "failure must surface as an error toast; got {:?}",
        toasts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn status_bar_uses_label_counts_instead_of_loaded_window() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(5);
    if let Some(first) = envelopes.first_mut() {
        first.flags.remove(MessageFlags::READ);
        first.flags.insert(MessageFlags::STARRED);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    app.mailbox.labels = make_test_labels();
    let inbox = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap()
        .id
        .clone();
    app.mailbox.active_label = Some(inbox);
    app.last_sync_status = Some("synced just now".into());

    let state = app.status_bar_state();

    assert_eq!(state.mailbox_name, "INBOX");
    assert_eq!(state.total_count, Some(10));
    assert_eq!(state.unread_count, 3);
    assert_eq!(state.starred_count, 2);
    assert_eq!(state.sync_status.as_deref(), Some("synced just now"));
}

#[test]
fn all_envelopes_refresh_preserves_selection_when_possible() {
    let mut app = App::new();
    app.visible_height = 3;
    app.mailbox.mail_list_mode = MailListMode::Messages;
    let initial = make_test_envelopes(4);
    app.mailbox.all_envelopes = initial.clone();
    app.mailbox.envelopes = initial.clone();
    app.mailbox.selected_index = 2;
    app.mailbox.scroll_offset = 1;

    let mut refreshed = initial.clone();
    refreshed.push(make_test_envelopes(1).remove(0));

    apply_all_envelopes_refresh(&mut app, refreshed);

    assert_eq!(app.mailbox.selected_index, 2);
    assert_eq!(
        app.mailbox.envelopes[app.mailbox.selected_index].id,
        initial[2].id
    );
    assert_eq!(app.mailbox.scroll_offset, 1);
}

#[test]
fn all_envelopes_refresh_preserves_selected_message_when_rows_shift() {
    let mut app = App::new();
    app.mailbox.mail_list_mode = MailListMode::Messages;
    let initial = make_test_envelopes(4);
    let selected_id = initial[2].id.clone();
    app.mailbox.all_envelopes = initial.clone();
    app.mailbox.envelopes = initial;
    app.mailbox.selected_index = 2;

    let mut refreshed = make_test_envelopes(1);
    refreshed.extend(app.mailbox.envelopes.clone());

    apply_all_envelopes_refresh(&mut app, refreshed);

    assert_eq!(
        app.mailbox.envelopes[app.mailbox.selected_index].id,
        selected_id
    );
}

#[test]
fn all_envelopes_refresh_preserves_pending_label_view() {
    let mut app = App::new();
    let labels = make_test_labels();
    let inbox_id = labels
        .iter()
        .find(|label| label.name == "INBOX")
        .unwrap()
        .id
        .clone();
    let initial = make_test_envelopes(2);
    let refreshed = make_test_envelopes(5);
    app.mailbox.labels = labels;
    app.mailbox.envelopes = initial.clone();
    app.mailbox.all_envelopes = initial;
    app.mailbox.pending_active_label = Some(inbox_id);

    apply_all_envelopes_refresh(&mut app, refreshed.clone());

    assert_eq!(app.mailbox.all_envelopes.len(), refreshed.len());
    assert_eq!(app.mailbox.all_envelopes[0].id, refreshed[0].id);
    assert_eq!(app.mailbox.envelopes.len(), 2);
}

#[test]
fn label_counts_refresh_can_follow_empty_boot() {
    let mut app = App::new();
    app.mailbox.desired_system_mailbox = Some("INBOX".into());

    handle_daemon_event(
        &mut app,
        DaemonEvent::SyncCompleted {
            account_id: AccountId::new(),
            messages_synced: 0,
        },
    );

    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
}

#[test]
fn clear_filter_restores_all_envelopes() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(10);
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
    app.mailbox.active_label = Some(inbox_id);
    app.mailbox.envelopes = vec![app.mailbox.envelopes[0].clone()];
    app.mailbox.selected_index = 0;
    app.apply(Action::ClearFilter);
    assert!(app.mailbox.active_label.is_none());
    assert_eq!(app.mailbox.envelopes.len(), 10, "Should restore full list");
}

#[test]
fn archive_keeps_row_until_confirmed_completion() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let removed_id = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::Archive);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.mailbox.envelopes.len(), 5);
    assert!(!app.pending_optimistic.is_removed(&removed_id));
    assert!(matches!(
        app.pending_mutation_queue[0].effect,
        MutationEffect::RemoveFromList(_)
    ));
}

#[test]
fn archive_in_threads_mode_targets_every_message_in_thread() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let mut envelopes = make_test_envelopes(5);
    let shared_thread = ThreadId::new();
    envelopes[0].thread_id = shared_thread.clone();
    envelopes[2].thread_id = shared_thread.clone();
    envelopes[4].thread_id = shared_thread.clone();
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    assert_eq!(app.mailbox.mail_list_mode, MailListMode::Threads);
    app.mailbox.selected_index = 0;

    app.apply(Action::Archive);

    let pending = app
        .modals
        .pending_bulk_confirm
        .as_ref()
        .expect("expected bulk confirm for multi-target archive");
    match &pending.request {
        Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } => {
            assert_eq!(message_ids.len(), 3, "all thread members archived");
        }
        other => panic!("expected Archive mutation, got {other:?}"),
    }
}

#[test]
fn archive_row_remains_visible_during_a_stale_refresh() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let archived_id = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::Archive);
    let mut refresh = app.mailbox.all_envelopes.clone();
    app.pending_optimistic.apply(&mut refresh);
    assert_eq!(refresh.len(), 3);
    assert!(refresh.iter().any(|envelope| envelope.id == archived_id));
    assert!(!app.pending_optimistic.is_removed(&archived_id));
}

#[test]
fn archive_response_loss_keeps_the_row_without_a_fresh_identity_retry() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let archived_id = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::Archive);
    let queued = app.pending_mutation_queue.remove(0);
    assert!(!super::super::mutation_can_auto_retry(&queued.request));
    app.finish_pending_mutation();
    assert_eq!(app.mailbox.envelopes.len(), 3);
    assert!(!app.pending_optimistic.is_removed(&archived_id));
    assert!(app.pending_mutation_queue.is_empty());
}

#[test]
fn archive_outside_inbox_does_not_remove_optimistically() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    app.mailbox.active_label = app
        .mailbox
        .labels
        .iter()
        .find(|label| label.name.eq_ignore_ascii_case("STARRED"))
        .map(|label| label.id.clone());
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::Archive);

    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(
        app.mailbox.envelopes.len(),
        3,
        "archive outside inbox should not strip the row before the daemon responds"
    );
}

#[test]
fn star_updates_flags_in_place() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    assert!(!app.mailbox.envelopes[0]
        .flags
        .contains(MessageFlags::STARRED));
    app.apply(Action::Star);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_count, 1);
    assert!(app.mailbox.envelopes[0]
        .flags
        .contains(MessageFlags::STARRED));
}

#[test]
fn bulk_mark_read_applies_flags_when_confirmed() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(3);
    for envelope in &mut envelopes {
        envelope.flags.remove(MessageFlags::READ);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes.clone();
    app.mailbox.selected_set = envelopes
        .iter()
        .map(|envelope| envelope.id.clone())
        .collect();

    app.apply(Action::MarkRead);
    assert!(app.pending_mutation_queue.is_empty());
    match app.modals.pending_bulk_confirm.as_ref() {
        Some(confirm) => match &confirm.request {
            Request::Mutation {
                mutation: MutationCommand::SetRead { message_ids, read },
                ..
            } => {
                assert!(*read);
                assert_eq!(message_ids.len(), 3);
            }
            other => panic!("Expected SetRead bulk request, got {other:?}"),
        },
        None => panic!("Expected pending bulk confirmation"),
    }
    assert!(app
        .mailbox
        .envelopes
        .iter()
        .all(|envelope| !envelope.flags.contains(MessageFlags::READ)));

    app.apply(Action::OpenSelected);

    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_count, 1);
    assert!(app.modals.pending_bulk_confirm.is_none());
    assert!(app
        .mailbox
        .envelopes
        .iter()
        .all(|envelope| envelope.flags.contains(MessageFlags::READ)));
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Marking 3 messages as read...")
    );
}

#[test]
fn status_bar_shows_pending_mutation_indicator_after_other_actions() {
    let mut app = App::new();
    let mut envelopes = make_test_envelopes(2);
    for envelope in &mut envelopes {
        envelope.flags.remove(MessageFlags::READ);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;

    app.apply(Action::MarkRead);
    app.apply(Action::MoveDown);

    let state = app.status_bar_state();
    assert_eq!(state.pending_mutation_count, 1);
    assert_eq!(
        state.pending_mutation_status.as_deref(),
        Some("Marking 1 message as read...")
    );
}

#[test]
fn mark_read_and_archive_removes_message_optimistically_and_queues_mutation() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let mut envelopes = make_test_envelopes(1);
    envelopes[0].flags.remove(MessageFlags::READ);
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    let message_id = app.mailbox.envelopes[0].id.clone();

    app.apply(Action::MarkReadAndArchive);

    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation: MutationCommand::ReadAndArchive { message_ids },
            ..
        } => {
            assert_eq!(message_ids, &vec![message_id]);
        }
        other => panic!("expected read-and-archive mutation, got {other:?}"),
    }
}

#[test]
fn bulk_mark_read_and_archive_removes_messages_when_confirmed() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let mut envelopes = make_test_envelopes(3);
    for envelope in &mut envelopes {
        envelope.flags.remove(MessageFlags::READ);
    }
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes.clone();
    app.mailbox.selected_set = envelopes
        .iter()
        .map(|envelope| envelope.id.clone())
        .collect();

    app.apply(Action::MarkReadAndArchive);
    match app.modals.pending_bulk_confirm.as_ref() {
        Some(confirm) => match &confirm.request {
            Request::Mutation {
                mutation: MutationCommand::ReadAndArchive { message_ids },
                ..
            } => {
                assert_eq!(message_ids.len(), 3);
            }
            other => panic!("Expected ReadAndArchive bulk request, got {other:?}"),
        },
        None => panic!("Expected pending bulk confirmation"),
    }
    assert_eq!(app.mailbox.envelopes.len(), 3);

    app.apply(Action::OpenSelected);

    assert!(app.modals.pending_bulk_confirm.is_none());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_count, 1);
    assert!(app.mailbox.envelopes.is_empty());
    assert!(app.mailbox.all_envelopes.is_empty());
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Marking 3 messages as read and archiving...")
    );
}

#[test]
fn invite_response_action_requires_calendar_metadata() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::RespondInvite(
        mxr_protocol::CalendarInviteActionData::Accept,
    ));

    assert!(app.pending_mutation_queue.is_empty());
    assert!(app.modals.pending_bulk_confirm.is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("No calendar invite found for this message")
    );
}

#[test]
fn invite_response_action_arms_pending_send_with_undo_window() {
    let mut app = App::new();
    let envelope = make_test_envelopes(1).remove(0);
    app.mailbox.envelopes = vec![envelope.clone()];
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.body_cache.insert(
        envelope.id.clone(),
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("Join us".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session".into()),
                    starts_at: Some("2026-05-20T15:00:00Z".into()),
                    organizer: Some(CalendarPerson {
                        email: "organizer@example.com".into(),
                        name: Some("Organizer".into()),
                        uri: Some("mailto:organizer@example.com".into()),
                    }),
                    attendees: vec![CalendarAttendee {
                        email: "user@example.com".into(),
                        name: Some("User".into()),
                        uri: Some("mailto:user@example.com".into()),
                        partstat: Some("NEEDS-ACTION".into()),
                        role: Some("REQ-PARTICIPANT".into()),
                        rsvp: Some(true),
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    );

    app.apply(Action::RespondInvite(
        mxr_protocol::CalendarInviteActionData::Accept,
    ));

    assert!(
        app.modals.pending_bulk_confirm.is_none(),
        "auto-confirm flow must not open the bulk confirm modal"
    );
    let pending = app
        .pending_invite_send
        .as_ref()
        .expect("RSVP must arm pending_invite_send slot");
    assert_eq!(pending.message_id, envelope.id);
    assert_eq!(
        pending.action,
        mxr_protocol::CalendarInviteActionData::Accept
    );
    assert!(
        app.pending_mutation_queue.is_empty(),
        "the daemon RPC must not fire until the 1s window elapses"
    );

    let future = pending.dispatch_at + std::time::Duration::from_millis(1);
    app.tick_pending_invite_send(future);

    assert!(app.pending_invite_send.is_none());
    assert_eq!(app.pending_mutation_queue.len(), 1);
    match &app.pending_mutation_queue[0].request {
        Request::RespondInvite {
            message_id,
            action,
            dry_run,
        } => {
            assert_eq!(message_id, &envelope.id);
            assert_eq!(*action, mxr_protocol::CalendarInviteActionData::Accept);
            assert!(!dry_run);
        }
        other => panic!("expected queued RespondInvite request, got {other:?}"),
    }
}

#[test]
fn invite_response_undo_within_window_prevents_send() {
    let mut app = App::new();
    let envelope = make_test_envelopes(1).remove(0);
    app.mailbox.envelopes = vec![envelope.clone()];
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.body_cache.insert(
        envelope.id.clone(),
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("Join us".into()),
            text_html: None,
            attachments: Vec::new(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Planning session".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    );

    app.apply(Action::RespondInvite(
        mxr_protocol::CalendarInviteActionData::Accept,
    ));
    assert!(app.pending_invite_send.is_some());

    app.apply(Action::UndoLastMutation);

    assert!(
        app.pending_invite_send.is_none(),
        "undo must clear the pending invite send"
    );
    assert!(
        app.pending_mutation_queue.is_empty(),
        "undo must prevent the daemon RPC entirely"
    );
}

#[test]
fn mutation_failure_opens_error_modal_and_refreshes_mailbox() {
    let mut app = App::new();

    app.show_mutation_failure(&MxrError::Ipc("boom".into()));
    app.refresh_mailbox_after_mutation_failure();

    assert_eq!(
        app.modals.error.as_ref().map(|modal| modal.title.as_str()),
        Some("Mutation Failed")
    );
    assert_eq!(
        app.modals
            .error
            .as_ref()
            .map(|modal| modal.detail.contains("boom")),
        Some(true)
    );
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    assert!(app.mailbox.pending_subscriptions_refresh);
}

#[test]
fn mutation_failure_reloads_pending_label_fetch() {
    let mut app = App::new();
    let inbox_id = LabelId::new();
    app.mailbox.pending_active_label = Some(inbox_id.clone());

    app.refresh_mailbox_after_mutation_failure();

    assert_eq!(app.mailbox.pending_label_fetch.as_ref(), Some(&inbox_id));
}

#[test]
fn archive_viewing_message_effect() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let viewing_id = app
        .mailbox
        .viewing_envelope
        .as_ref()
        .expect("open selected should populate viewing envelope")
        .id
        .clone();
    app.apply(Action::Archive);
    let effect = app.pending_mutation_queue.remove(0).effect;
    match &effect {
        MutationEffect::RemoveFromList(id) => {
            assert_eq!(*id, viewing_id);
        }
        _ => panic!("Expected RemoveFromList"),
    }
}

#[test]
fn archive_keeps_reader_open_and_selects_next_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let removed_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    let next_id = app.mailbox.envelopes[1].id.clone();

    app.apply_removed_message_ids(&[removed_id]);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    assert_eq!(app.mailbox.selected_index, 0);
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|envelope| envelope.id.clone()),
        Some(next_id)
    );
}

#[test]
fn archive_keeps_mail_list_focus_when_reader_was_visible() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.mailbox.active_pane = ActivePane::MailList;
    let removed_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();
    let next_id = app.mailbox.envelopes[1].id.clone();

    app.apply_removed_message_ids(&[removed_id]);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|envelope| envelope.id.clone()),
        Some(next_id)
    );
}

#[test]
fn archive_last_visible_message_closes_reader() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    let removed_id = app.mailbox.viewing_envelope.as_ref().unwrap().id.clone();

    app.apply_removed_message_ids(&[removed_id]);

    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert!(app.mailbox.viewing_envelope.is_none());
    assert!(app.mailbox.envelopes.is_empty());
}

#[test]
fn open_selected_populates_visible_thread_messages() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(5);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.viewed_thread_messages.len(), 2);
    assert_eq!(
        app.mailbox.viewed_thread_messages[0].id,
        app.mailbox.envelopes[0].id
    );
    assert_eq!(
        app.mailbox.viewed_thread_messages[1].id,
        app.mailbox.envelopes[1].id
    );
}

#[test]
fn mail_list_defaults_to_threads() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    assert_eq!(app.mail_list_rows().len(), 2);
    assert_eq!(
        app.selected_mail_row().map(|row| row.message_count),
        Some(2)
    );
}

#[test]
fn open_thread_focuses_latest_unread_message() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    let shared_thread = ThreadId::new();
    app.mailbox.envelopes[0].thread_id = shared_thread.clone();
    app.mailbox.envelopes[1].thread_id = shared_thread;
    app.mailbox.envelopes[0].date = chrono::Utc::now() - chrono::Duration::minutes(10);
    app.mailbox.envelopes[1].date = chrono::Utc::now();
    app.mailbox.envelopes[0].flags = MessageFlags::READ;
    app.mailbox.envelopes[1].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert_eq!(app.mailbox.thread_selected_index, 1);
    assert_eq!(
        app.focused_thread_envelope().map(|env| env.id.clone()),
        Some(app.mailbox.envelopes[1].id.clone())
    );
}

#[test]
fn open_selected_marks_unread_message_read_after_dwell() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);

    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert!(app.pending_mutation_queue.is_empty());

    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
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
    assert!(app.pending_mutation_queue[0].best_effort);
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
fn preview_read_transient_failure_retries_without_error_modal_or_rollback() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    let queued = app.pending_mutation_queue.remove(0);
    assert!(queued.best_effort);
    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));

    let error = MxrError::Ipc("mutation skipped 1 message(s): pool timed out".into());
    app.finish_pending_mutation();
    assert!(app.should_retry_mutation_failure(&error));
    app.schedule_mutation_retry(queued, &error);

    assert!(app.modals.error.is_none());
    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_queue[0].attempts, 1);
    assert!(app.pending_mutation_queue[0].run_after.is_some());
    assert_eq!(app.pending_mutation_count, 1);
}

#[test]
fn preview_read_exhausted_failure_reconciles_without_error_modal() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    let queued = app.pending_mutation_queue.remove(0);
    assert!(queued.best_effort);
    assert!(app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));

    app.finish_pending_mutation();
    app.handle_mutation_failure_result(
        queued.id,
        queued.best_effort,
        &MxrError::Ipc("pool timed out".into()),
    );

    assert!(app.modals.error.is_none());
    assert!(!app.mailbox.envelopes[0].flags.contains(MessageFlags::READ));
    assert!(!app.mailbox.all_envelopes[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app.mailbox.viewed_thread_messages[0]
        .flags
        .contains(MessageFlags::READ));
    assert!(!app
        .mailbox
        .viewing_envelope
        .as_ref()
        .unwrap()
        .flags
        .contains(MessageFlags::READ));
    assert!(app.mailbox.pending_labels_refresh);
    assert!(app.mailbox.pending_all_envelopes_refresh);
    assert!(app.diagnostics.pending_status_refresh);
    let now = std::time::Instant::now();
    let toasts = app.toasts.visible(now);
    assert!(
        toasts
            .iter()
            .any(|toast| toast.text == "Mailbox refreshing to reconcile state"),
        "best-effort failure must surface as a warn toast; got {:?}",
        toasts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn reopening_same_message_does_not_queue_duplicate_read_mutation() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::empty();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.apply(Action::OpenSelected);

    assert!(app.pending_mutation_queue.is_empty());
    app.expire_pending_preview_read_for_tests();
    app.tick();
    assert_eq!(app.pending_mutation_queue.len(), 1);
}

#[test]
fn tiny_terminal_renders_too_small_placeholder() {
    let mut app = App::new();
    let output = mxr_test_support::render_to_string(60, 12, |frame| app.draw(frame));
    assert!(
        output.contains("Terminal too small"),
        "placeholder must render below 80x20; got:\n{output}"
    );
    assert!(output.contains("80x20"));

    let output = mxr_test_support::render_to_string(80, 20, |frame| app.draw(frame));
    assert!(
        !output.contains("Terminal too small"),
        "80x20 exactly must render the full layout"
    );
}

#[test]
fn a_degraded_first_status_snapshot_leaves_the_total_unknown() {
    use crate::async_result::StatusSnapshot;

    let mut app = App::new();
    app.mailbox.all_envelopes = make_test_envelopes(10);

    app.apply_status_snapshot(StatusSnapshot {
        uptime_secs: 5,
        daemon_pid: Some(42),
        accounts: Vec::new(),
        total_messages: 0,
        sync_statuses: Vec::new(),
        degraded: true,
        freshness: None,
    });

    assert!(
        app.status_bar_state().total_count.is_none(),
        "a degraded first snapshot must leave the total unknown, not fall back to the loaded window"
    );

    app.apply_status_snapshot(StatusSnapshot {
        uptime_secs: 6,
        daemon_pid: Some(42),
        accounts: vec!["personal".into()],
        total_messages: 4_242,
        sync_statuses: Vec::new(),
        degraded: false,
        freshness: None,
    });
    assert_eq!(app.status_bar_state().total_count, Some(4_242));

    app.apply_status_snapshot(StatusSnapshot {
        uptime_secs: 7,
        daemon_pid: Some(42),
        accounts: Vec::new(),
        total_messages: 0,
        sync_statuses: Vec::new(),
        degraded: true,
        freshness: None,
    });
    assert_eq!(
        app.status_bar_state().total_count,
        Some(4_242),
        "a degraded snapshot must not overwrite a reading that was real"
    );
}

fn titlecase_labels() -> Vec<Label> {
    let mut labels = make_test_labels();
    for label in &mut labels {
        let lower = label.name.to_lowercase();
        let mut chars = lower.chars();
        label.name = chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default();
    }
    labels
}

#[test]
fn g_keys_find_system_mailboxes_whatever_their_case() {
    for (action, name) in [
        (Action::GoToInbox, "Inbox"),
        (Action::GoToSent, "Sent"),
        (Action::GoToTrash, "Trash"),
        (Action::GoToSpam, "Spam"),
        (Action::GoToStarred, "Starred"),
    ] {
        let mut app = App::new();
        app.mailbox.labels = titlecase_labels();
        app.apply(action.clone());
        let label = app
            .mailbox
            .labels
            .iter()
            .find(|l| l.name == name)
            .unwrap_or_else(|| panic!("a {name} label"));
        assert_eq!(
            app.mailbox.pending_active_label.as_ref(),
            Some(&label.id),
            "{action:?}"
        );
    }
}

#[test]
fn starred_by_flag_is_titled_starred() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.apply(Action::GoToStarred);
    assert_eq!(app.mailbox.active_role, Some(mxr_core::Role::Starred));
    let title = app.mail_list_title();
    assert!(title.starts_with("Starred "), "{title}");
    app.apply(Action::GoToAllMail);
    assert!(app.mail_list_title().starts_with("All Mail "));
}

#[test]
fn search_results_keep_room_for_the_subject() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.results = vec![TestEnvelopeBuilder::new()
        .subject("Contract renewal for next year")
        .with_from_address("Samir Patel", "samir@example.com")
        .build()];
    app.search.page.session_active = true;
    app.search.page.editing = false;
    let backend = ratatui::backend::TestBackend::new(160, 20);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    assert!(
        rows.iter().any(|row| row.contains("Contract renewal")),
        "{}",
        rows.join("\n")
    );
}
