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
    // INBOX should be first, then STARRED, SENT, etc.
    assert_eq!(system_names[0], "INBOX");
    assert_eq!(system_names[1], "STARRED");
    assert_eq!(system_names[2], "SENT");
    assert_eq!(system_names[3], "DRAFT");
    assert_eq!(system_names[4], "ARCHIVE");
}

#[test]
fn sidebar_items_put_inbox_before_all_mail() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();

    let items = app.sidebar_items();
    let all_mail_index = items
        .iter()
        .position(|item| matches!(item, SidebarItem::AllMail))
        .unwrap();

    assert!(matches!(
        items.first(),
        Some(SidebarItem::Label(label)) if label.name == "INBOX"
    ));
    assert!(all_mail_index > 0);
}

#[test]
fn sidebar_hidden_labels_not_shown() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
    assert!(
        !names.contains(&"CATEGORY_UPDATES"),
        "Gmail categories should be hidden"
    );
}

#[test]
fn sidebar_empty_system_labels_hidden_except_primary() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let names: Vec<&str> = ordered.iter().map(|l| l.name.as_str()).collect();
    // CHAT has 0 total, 0 unread — should be hidden
    assert!(
        !names.contains(&"CHAT"),
        "Empty non-primary system labels should be hidden"
    );
    // DRAFT has 0 total but is primary — should be shown
    assert!(
        names.contains(&"DRAFT"),
        "Primary system labels shown even if empty"
    );
    assert!(
        names.contains(&"ARCHIVE"),
        "Archive should be shown as a primary system label even if empty"
    );
    // IMPORTANT has 5 total — should be shown (non-primary but non-empty)
    assert!(
        names.contains(&"IMPORTANT"),
        "Non-empty system labels should be shown"
    );
}

#[test]
fn sidebar_user_labels_alphabetical() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let ordered = app.ordered_visible_labels();
    let user_names: Vec<&str> = ordered
        .iter()
        .filter(|l| l.kind == LabelKind::User)
        .map(|l| l.name.as_str())
        .collect();
    // Personal < Work alphabetically
    assert_eq!(user_names, vec!["Personal", "Work"]);
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
fn goto_inbox_without_labels_records_desired_mailbox() {
    let mut app = App::new();
    app.apply(Action::GoToInbox);
    assert_eq!(app.mailbox.desired_system_mailbox.as_deref(), Some("INBOX"));
    assert!(app.mailbox.pending_label_fetch.is_none());
    assert!(app.mailbox.pending_active_label.is_none());
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
    assert_eq!(
        app.status_message.as_deref(),
        Some("Mutation failed: provider rejected")
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
    assert_eq!(state.total_count, 10);
    assert_eq!(state.unread_count, 3);
    assert_eq!(state.starred_count, 2);
    assert_eq!(state.sync_status.as_deref(), Some("synced just now"));
}

#[test]
fn all_envelopes_refresh_updates_visible_all_mail() {
    let mut app = App::new();
    let envelopes = make_test_envelopes(4);
    app.mailbox.active_label = None;
    app.search.active = false;

    apply_all_envelopes_refresh(&mut app, envelopes.clone());

    assert_eq!(app.mailbox.all_envelopes.len(), 4);
    assert_eq!(app.mailbox.envelopes.len(), 4);
    assert_eq!(app.mailbox.selected_index, 0);
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
    app.mailbox.envelopes = vec![app.mailbox.envelopes[0].clone()]; // Simulate filtered
    app.mailbox.selected_index = 0;
    app.apply(Action::ClearFilter);
    assert!(app.mailbox.active_label.is_none());
    assert_eq!(app.mailbox.envelopes.len(), 10, "Should restore full list");
}

#[test]
fn mail_list_rows_include_open_commitment_counts() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    let envelope = app.mailbox.envelopes[0].clone();
    app.mailbox
        .open_commitment_counts
        .insert((envelope.account_id.clone(), envelope.thread_id.clone()), 2);

    let rows = app.mail_list_rows();

    assert_eq!(rows[0].open_commitment_count, 2);
}

#[test]
fn archive_removes_from_list() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let removed_id = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::Archive);
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.mailbox.envelopes.len(), 4);
    assert!(!app
        .mailbox
        .envelopes
        .iter()
        .any(|envelope| envelope.id == removed_id));
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
    // Threads mode is the default; sanity-check it.
    assert_eq!(app.mailbox.mail_list_mode, MailListMode::Threads);
    // Cursor is on the row representing the 3-message thread.
    app.mailbox.selected_index = 0;

    app.apply(Action::Archive);

    // 3 targets triggers the bulk-confirm modal before the mutation
    // is dispatched. Inspect the staged request there.
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
fn stale_label_refresh_does_not_revive_optimistically_archived_envelope() {
    // Reproduces the bounce-back bug: user presses `e` on a message,
    // it disappears optimistically, then a label-refresh response
    // (sync- or mutation-triggered) lands with the still-present
    // envelope because the daemon hasn't yet processed the archive.
    // Before the fix, the response unconditionally replaced
    // `mailbox.envelopes` and the row came back. With the fix, the
    // pending optimistic state masks the refresh until the mutation
    // acks.
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let before_archive = app.mailbox.envelopes.clone();
    let archived_id = app.mailbox.envelopes[0].id.clone();

    app.apply(Action::Archive);
    assert_eq!(app.mailbox.envelopes.len(), 2, "optimistic remove fired");
    assert!(app.pending_optimistic.is_removed(&archived_id));

    // Stale refresh from the daemon: it hasn't processed the archive
    // yet, so it returns every envelope including the archived one.
    let mut refresh = before_archive.clone();
    app.pending_optimistic.apply(&mut refresh);
    assert_eq!(
        refresh.len(),
        2,
        "stale refresh must be filtered to honor the pending archive"
    );
    assert!(!refresh.iter().any(|env| env.id == archived_id));

    // Once the daemon acks, future refreshes are unmasked again.
    let mutation_id = app.pending_mutation_queue[0].id;
    app.pending_optimistic.clear(mutation_id);
    let mut after_ack = before_archive;
    app.pending_optimistic.apply(&mut after_ack);
    assert_eq!(
        after_ack.len(),
        3,
        "after mutation ack, refresh is no longer masked"
    );
}

#[test]
fn archived_message_stays_hidden_while_transient_failure_retries() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let archived_id = app.mailbox.envelopes[0].id.clone();

    app.apply(Action::Archive);

    assert_eq!(app.mailbox.envelopes.len(), 2);
    assert!(app.pending_optimistic.is_removed(&archived_id));
    let queued = app.pending_mutation_queue.remove(0);
    assert!(!queued.best_effort);

    let error = MxrError::Ipc("mutation skipped 1 message(s): pool timed out".into());
    app.finish_pending_mutation();
    app.schedule_mutation_retry(queued, &error);

    assert!(app.modals.error.is_none());
    assert_eq!(
        app.mailbox.envelopes.len(),
        2,
        "retrying a transient failure must not bounce the archived row back into view"
    );
    assert!(app.pending_optimistic.is_removed(&archived_id));
    assert_eq!(app.pending_mutation_queue.len(), 1);
    assert_eq!(app.pending_mutation_queue[0].attempts, 1);
    assert_eq!(app.pending_mutation_count, 1);
}

#[test]
fn archive_outside_inbox_does_not_remove_optimistically() {
    let mut app = App::new();
    // Active label = STARRED (not INBOX). Archive removes INBOX, so the
    // message still belongs in the Starred view and should NOT vanish.
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
    // First envelope is READ (even index), not starred
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

/// `Action::RespondInvite` no longer opens a modal — it arms
/// `pending_invite_send` with a 1s hold window. The tick loop later
/// drains it into `pending_mutation_queue`. Pressing `u` within the
/// window cancels without ever talking to the daemon, so no email is
/// sent on a mistaken keystroke.
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

    // Tick past the dispatch deadline and confirm the request drains
    // into the mutation queue.
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

/// Pressing `u` while `pending_invite_send` is armed cancels the RSVP
/// before any daemon RPC fires — no email goes out.
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

/// `format_mutation_failure` is what the runtime surfaces to the
/// user when the daemon returns 0 succeeded with skipped > 0.
/// Locks down two behaviors:
///  - when no per-account error is set, it falls back to the
///    skipped-count summary (the previous all-the-time behavior);
///  - when per-account errors are present, they are joined onto
///    the summary so the user sees the real cause (e.g. pool
///    timeout) instead of a meaningless "skipped 1 message(s)".
#[test]
fn format_mutation_failure_joins_per_account_errors() {
    use super::super::format_mutation_failure;
    use mxr_core::id::AccountId;
    use mxr_protocol::{AccountMutationResultData, MutationResultData};

    let bare = MutationResultData {
        requested: 1,
        succeeded: 0,
        skipped: 1,
        failed: 0,
        accounts: vec![AccountMutationResultData {
            account_id: AccountId::new(),
            account_name: "primary".into(),
            succeeded: 0,
            skipped: 1,
            failed: 0,
            error: None,
        }],
        mutation_id: None,
        undo_unavailable: false,
    };
    assert_eq!(
        format_mutation_failure(&bare),
        "mutation skipped 1 message(s)"
    );

    let with_error = MutationResultData {
        accounts: vec![
            AccountMutationResultData {
                account_id: AccountId::new(),
                account_name: "primary".into(),
                succeeded: 0,
                skipped: 1,
                failed: 0,
                error: Some("pool timed out while waiting for an open connection".into()),
            },
            AccountMutationResultData {
                account_id: AccountId::new(),
                account_name: "secondary".into(),
                succeeded: 0,
                skipped: 1,
                failed: 0,
                error: Some("disk I/O error".into()),
            },
        ],
        ..bare
    };
    let formatted = format_mutation_failure(&with_error);
    assert!(formatted.starts_with("mutation skipped 1 message(s):"));
    assert!(formatted.contains("pool timed out"));
    assert!(formatted.contains("disk I/O error"));
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
    // Open first message
    app.apply(Action::OpenSelected);
    let viewing_id = app
        .mailbox
        .viewing_envelope
        .as_ref()
        .expect("open selected should populate viewing envelope")
        .id
        .clone();
    // The pending_mutation_queue is empty — Archive wasn't pressed yet
    // Press archive while viewing
    app.apply(Action::Archive);
    let effect = app.pending_mutation_queue.remove(0).effect;
    // Verify the effect targets the viewing envelope
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
fn mail_list_title_shows_message_count() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let title = app.mail_list_title();
    assert!(title.contains("5"), "Title should show message count");
    assert!(
        title.contains("Threads"),
        "Default title should say Threads"
    );
}

#[test]
fn mail_list_title_shows_label_name() {
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
    app.mailbox.active_label = Some(inbox_id);
    let title = app.mail_list_title();
    assert!(
        title.contains("Inbox"),
        "Title should show humanized label name"
    );
}

#[test]
fn mail_list_title_shows_search_query() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(5);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.search.active = true;
    app.search.bar.query = "deployment".to_string();
    let title = app.mail_list_title();
    assert!(
        title.contains("deployment"),
        "Title should show search query"
    );
    assert!(title.contains("Search"), "Title should indicate search");
}

#[test]
fn message_view_body_display() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenMessageView);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::ThreePane);
    app.mailbox.body_view_state = BodyViewState::ready(
        "Hello".into(),
        "Hello".into(),
        BodySource::Plain,
        BodyViewMetadata::default(),
    );
    assert_eq!(app.mailbox.body_view_state.display_text(), Some("Hello"));
    app.apply(Action::CloseMessageView);
    assert!(matches!(
        app.mailbox.body_view_state,
        BodyViewState::Empty { .. }
    ));
}

#[test]
fn close_message_view_preserves_reader_mode() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenMessageView);

    app.apply(Action::CloseMessageView);

    assert!(app.mailbox.reader_mode);
    assert!(app.mailbox.html_view);
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
    assert_eq!(
        app.status_message.as_deref(),
        Some("Mailbox refreshing to reconcile state")
    );
}

#[test]
fn open_selected_on_read_message_does_not_queue_read_mutation() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.envelopes[0].flags = MessageFlags::READ;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    app.apply(Action::OpenSelected);
    app.expire_pending_preview_read_for_tests();
    app.tick();

    assert!(app.pending_mutation_queue.is_empty());
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
