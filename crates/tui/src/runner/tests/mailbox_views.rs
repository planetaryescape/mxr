use super::*;

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
