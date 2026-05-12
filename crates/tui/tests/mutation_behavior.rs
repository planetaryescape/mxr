//! Behavior tests for TUI mutation actions.
//!
//! These tests construct an `App` directly and drive `Action` dispatches
//! through the public `App::apply` entry point. No daemon, no IPC — they
//! verify what the user observes in the TUI immediately after a keystroke,
//! before any provider round-trip resolves.
//!
//! Test names describe behavior, not implementation. Each test should
//! survive an internal refactor of mutation infrastructure (snapshot
//! buffer, reconciliation routing, etc.) as long as the observable
//! outcome remains the same.

use chrono::{TimeZone, Utc};
use mxr_core::id::{AccountId, LabelId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, Label, LabelKind, MessageFlags, UnsubscribeMethod};
use mxr_protocol::{MutationCommand, Request};
use mxr_tui::action::Action;
use mxr_tui::app::mutation_snapshot::MUTATION_SNAPSHOT_CAPACITY;
use mxr_tui::app::{App, MutationId};
use mxr_tui::ui::label_picker::LabelPickerMode;

fn set_active_inbox_for_tests(app: &mut App) {
    let account_id = app
        .mailbox
        .envelopes
        .first()
        .map(|e| e.account_id.clone())
        .unwrap_or_else(AccountId::new);
    let inbox_id = LabelId::from_provider_id("test", "INBOX");
    app.mailbox.labels = vec![Label {
        id: inbox_id.clone(),
        account_id,
        name: "INBOX".into(),
        kind: LabelKind::System,
        color: None,
        provider_id: "INBOX".into(),
        unread_count: 1,
        total_count: 1,
    }];
    app.mailbox.active_label = Some(inbox_id);
}

fn unstarred_inbox_envelope() -> Envelope {
    Envelope {
        id: MessageId::new(),
        account_id: AccountId::new(),
        provider_id: "msg-1".into(),
        thread_id: ThreadId::new(),
        message_id_header: Some("<msg-1@example.com>".into()),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some("Alice Example".into()),
            email: "alice@example.com".into(),
        },
        to: vec![Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Test message".into(),
        date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
        flags: MessageFlags::READ,
        snippet: "fixture".into(),
        has_attachments: false,
        size_bytes: 100,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: vec!["INBOX".into()],
    }
}

#[test]
fn star_applies_optimistically_before_daemon_response() {
    // Given: a TUI with one unstarred message visible in the inbox.
    let mut app = App::new();
    let envelope = unstarred_inbox_envelope();
    let message_id = envelope.id.clone();
    app.mailbox.envelopes.push(envelope);
    app.mailbox.selected_index = 0;

    assert!(
        !app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "precondition: envelope starts unstarred"
    );
    assert_eq!(
        app.pending_mutation_queue.len(),
        0,
        "precondition: no mutations queued"
    );

    // When: the user dispatches Star. No daemon connection exists in the
    // test, so the mutation cannot have been confirmed by any provider.
    app.apply(Action::Star);

    // Then: the visible envelope is already starred — the user observes
    // the change before any network round-trip.
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "envelope flags reflect optimistic star application"
    );

    // And: exactly one Star mutation is queued for the daemon to apply.
    assert_eq!(
        app.pending_mutation_queue.len(),
        1,
        "exactly one mutation queued"
    );
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation:
                MutationCommand::Star {
                    message_ids,
                    starred,
                },
            ..
        } => {
            assert_eq!(
                message_ids,
                &vec![message_id],
                "queued mutation targets the selected message"
            );
            assert!(*starred, "queued mutation requests starred = true");
        }
        other => panic!("expected queued Star mutation, got {other:?}"),
    }
}

#[test]
fn snapshot_buffer_evicts_oldest_when_full() {
    // The store is bounded so a runaway burst of mutations cannot grow
    // memory without limit. When capacity is reached, the oldest snapshot
    // is evicted FIFO — its mutation can no longer be rolled back. The
    // newest snapshots stay rollback-eligible.
    let mut app = App::new();

    // Seed one more envelope than the snapshot store can retain.
    let count = MUTATION_SNAPSHOT_CAPACITY + 1;
    for _ in 0..count {
        app.mailbox.envelopes.push(unstarred_inbox_envelope());
    }

    // Star each in sequence. Each Star captures a snapshot at queue time;
    // after `count` stars, the very first snapshot is evicted because the
    // store is full when the last one is inserted.
    let mut mutation_ids = Vec::with_capacity(count);
    for index in 0..count {
        app.mailbox.selected_index = index;
        app.apply(Action::Star);
        assert_eq!(
            app.pending_mutation_queue.len(),
            index + 1,
            "queued one mutation per Star"
        );
        mutation_ids.push(app.pending_mutation_queue[index].id);
    }

    // Attempt to roll back the OLDEST mutation. Its snapshot was evicted
    // when the newest was inserted, so the rollback is a no-op and the
    // optimistic star persists.
    app.handle_mutation_reconciliation_failed(mutation_ids[0]);
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "oldest mutation's effect persists once its snapshot is evicted"
    );

    // Attempt to roll back the NEWEST. Its snapshot is still retained, so
    // the rollback reverts the envelope to unstarred.
    app.handle_mutation_reconciliation_failed(mutation_ids[count - 1]);
    assert!(
        !app.mailbox.envelopes[count - 1]
            .flags
            .contains(MessageFlags::STARRED),
        "newest mutation's effect reverts when its snapshot is still retained"
    );
}

#[test]
fn star_reverts_when_reconciliation_fails() {
    // Given: an unstarred message and an optimistic star applied through Action::Star.
    let mut app = App::new();
    let envelope = unstarred_inbox_envelope();
    app.mailbox.envelopes.push(envelope);
    app.mailbox.selected_index = 0;

    app.apply(Action::Star);
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "precondition: optimistic star is applied to local state"
    );

    // Simulate the IPC worker draining the queue (this is what `lib.rs`
    // does when sending the mutation to the daemon). Capture the
    // mutation's ID — in production the daemon echoes this ID back when
    // it cannot reconcile the change with the provider.
    let drained: Vec<_> = app.pending_mutation_queue.drain(..).collect();
    assert_eq!(drained.len(), 1, "exactly one mutation was queued");
    let mutation_id: MutationId = drained[0].id;

    // When: the daemon notifies us that the provider rejected this mutation.
    app.handle_mutation_reconciliation_failed(mutation_id);

    // Then: the user sees the optimistic star reverted.
    assert!(
        !app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "envelope flags revert to pre-mutation state when reconciliation fails"
    );
}

#[test]
fn concurrent_mutations_on_same_message_compose_under_out_of_order_success() {
    // Two mutations on the same message — Star then ApplyLabel. Both
    // succeed, but the daemon acknowledges them in reverse order. The
    // final state must reflect both optimistic effects.
    let mut app = App::new();
    app.mailbox.envelopes.push(unstarred_inbox_envelope());
    app.mailbox.selected_index = 0;

    app.apply(Action::Star);
    let star_id = app.pending_mutation_queue[0].id;

    app.modals.pending_label_action = Some((LabelPickerMode::Apply, "Project X".into()));
    app.apply(Action::ApplyLabel);
    assert_eq!(
        app.pending_mutation_queue.len(),
        2,
        "two mutations queued for the same message"
    );
    let label_id = app.pending_mutation_queue[1].id;

    // Daemon acks the LABEL first, then the STAR. Each ack discards the
    // mutation's rollback snapshot — the optimistic local state stands.
    let _ = app.mutation_snapshots.take(label_id);
    let _ = app.mutation_snapshots.take(star_id);

    // Final state composes both effects.
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "star effect retained"
    );
    assert!(
        app.mailbox.envelopes[0]
            .label_provider_ids
            .contains(&"Project X".to_string()),
        "label effect retained"
    );

    // Snapshot store empties out — nothing left to roll back.
    assert!(
        app.mutation_snapshots.is_empty(),
        "no orphan snapshots remain after both mutations are acknowledged"
    );
}

#[test]
fn concurrent_mutations_on_same_message_partial_failure() {
    // Two mutations on the same message — Star then ApplyLabel. Star
    // succeeds at the provider; ApplyLabel fails. Only the Label effect
    // should be reverted; the Star effect must persist.
    let mut app = App::new();
    app.mailbox.envelopes.push(unstarred_inbox_envelope());
    app.mailbox.selected_index = 0;

    app.apply(Action::Star);
    let star_id = app.pending_mutation_queue[0].id;
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "precondition: star applied optimistically"
    );

    app.modals.pending_label_action = Some((LabelPickerMode::Apply, "Project X".into()));
    app.apply(Action::ApplyLabel);
    let label_id = app.pending_mutation_queue[1].id;
    assert!(
        app.mailbox.envelopes[0]
            .label_provider_ids
            .contains(&"Project X".to_string()),
        "precondition: label applied optimistically"
    );

    // Star succeeded → discard its rollback snapshot.
    let _ = app.mutation_snapshots.take(star_id);

    // Label failed → replay its snapshot, reverting only the label.
    app.handle_mutation_reconciliation_failed(label_id);

    // Final state: starred (Star kept), no label (Label reverted).
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "star effect persists when only the label mutation failed"
    );
    assert!(
        !app.mailbox.envelopes[0]
            .label_provider_ids
            .contains(&"Project X".to_string()),
        "label effect reverts because label mutation failed"
    );
}

#[test]
fn flag_reply_later_queues_set_reply_later_request() {
    // Pressing `b` on a message queues a SetReplyLater request to the
    // daemon and marks the row locally so the user gets immediate
    // feedback before daemon reconciliation.
    let mut app = App::new();
    let envelope = unstarred_inbox_envelope();
    let message_id = envelope.id.clone();
    app.mailbox.envelopes.push(envelope);
    app.mailbox.selected_index = 0;

    app.apply(Action::FlagReplyLater);

    assert_eq!(
        app.pending_mutation_queue.len(),
        1,
        "exactly one SetReplyLater request queued"
    );
    match &app.pending_mutation_queue[0].request {
        Request::SetReplyLater {
            message_id: queued_id,
            flag,
        } => {
            assert_eq!(queued_id, &message_id);
            assert!(*flag, "flag is true on initial bookmark");
        }
        other => panic!("expected SetReplyLater request, got {other:?}"),
    }
    assert!(
        app.mailbox.reply_later_message_ids.contains(&message_id),
        "reply-later flag should be visible optimistically"
    );
    assert!(
        app.mail_list_rows()[0].reply_later,
        "mail list row should expose the optimistic reply-later marker"
    );
}

#[test]
fn flag_reply_later_with_no_selection_shows_status_not_queued() {
    // No message selected → action is a no-op with a clear status.
    let mut app = App::new();
    app.apply(Action::FlagReplyLater);

    assert_eq!(
        app.pending_mutation_queue.len(),
        0,
        "no request queued without a target"
    );
    assert_eq!(app.status_message.as_deref(), Some("No message selected"));
}

#[test]
fn move_to_label_removes_message_optimistically() {
    // Moving a message to a different label removes it from the
    // current view immediately — the user shouldn't have to wait for
    // a provider round-trip to see the action take effect.
    let mut app = App::new();
    let envelope = unstarred_inbox_envelope();
    let message_id = envelope.id.clone();
    app.mailbox.envelopes.push(envelope);
    app.mailbox.selected_index = 0;

    app.modals.pending_label_action = Some((LabelPickerMode::Move, "Project X".into()));
    app.apply(Action::MoveToLabel);

    assert!(
        !app.mailbox.envelopes.iter().any(|env| env.id == message_id),
        "envelope removed from current view optimistically"
    );
    assert_eq!(
        app.pending_mutation_queue.len(),
        1,
        "exactly one Move mutation queued"
    );
    match &app.pending_mutation_queue[0].request {
        Request::Mutation {
            mutation:
                MutationCommand::Move {
                    message_ids,
                    target_label,
                },
            ..
        } => {
            assert_eq!(message_ids, &vec![message_id]);
            assert_eq!(target_label, "Project X");
        }
        other => panic!("expected Move mutation, got {other:?}"),
    }
}

#[test]
fn bulk_star_reverts_all_messages_when_reconciliation_fails() {
    // A bulk mutation goes through the confirmation modal before queuing.
    // When the daemon rejects the batch, every message in it must revert
    // to pre-mutation state — not just the cursor message.
    let mut app = App::new();
    let env_a = unstarred_inbox_envelope();
    let env_b = unstarred_inbox_envelope();
    let id_a = env_a.id.clone();
    let id_b = env_b.id.clone();
    app.mailbox.envelopes.push(env_a);
    app.mailbox.envelopes.push(env_b);
    app.mailbox.selected_index = 0;
    app.mailbox.selected_set.insert(id_a);
    app.mailbox.selected_set.insert(id_b);

    // Multi-target Star opens the bulk-confirm modal — no mutation queued
    // until the user confirms.
    app.apply(Action::Star);
    assert!(
        app.modals.pending_bulk_confirm.is_some(),
        "bulk-confirm modal opens for multi-selection star"
    );
    assert_eq!(
        app.pending_mutation_queue.len(),
        0,
        "no mutation queued before confirmation"
    );

    // Confirm: optimistic apply runs and the mutation is queued.
    app.apply(Action::OpenSelected);
    assert!(
        app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "first message starred optimistically after confirm"
    );
    assert!(
        app.mailbox.envelopes[1]
            .flags
            .contains(MessageFlags::STARRED),
        "second message starred optimistically after confirm"
    );
    assert_eq!(
        app.pending_mutation_queue.len(),
        1,
        "exactly one bulk mutation queued"
    );
    let mutation_id = app.pending_mutation_queue[0].id;

    // Daemon rejects the batch. All messages in the batch revert.
    app.handle_mutation_reconciliation_failed(mutation_id);
    assert!(
        !app.mailbox.envelopes[0]
            .flags
            .contains(MessageFlags::STARRED),
        "first message reverts on bulk failure"
    );
    assert!(
        !app.mailbox.envelopes[1]
            .flags
            .contains(MessageFlags::STARRED),
        "second message reverts on bulk failure"
    );
}

#[test]
fn archive_inbox_optimistic_removal_restores_row_on_reconciliation_failure() {
    let mut app = App::new();
    let envelope = unstarred_inbox_envelope();
    let eid = envelope.id.clone();
    app.mailbox.envelopes.push(envelope.clone());
    app.mailbox.all_envelopes.push(envelope);
    app.mailbox.selected_index = 0;
    set_active_inbox_for_tests(&mut app);

    app.apply(Action::Archive);

    assert!(
        app.mailbox.envelopes.is_empty(),
        "row removed optimistically while viewing INBOX"
    );

    let mutation_id = app.pending_mutation_queue[0].id;
    app.handle_mutation_reconciliation_failed(mutation_id);

    assert_eq!(app.mailbox.envelopes.len(), 1);
    assert_eq!(app.mailbox.envelopes[0].id, eid);
    assert_eq!(app.mailbox.all_envelopes.len(), 1);
}
