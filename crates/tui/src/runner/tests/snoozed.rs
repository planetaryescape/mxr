//! Snoozed in the TUI (`g n`): the daemon's snoozed mail as a mail list,
//! waking soonest first, where the list's keys work and `w` wakes the lit
//! conversation now. `w` in the reply queue takes the lit message out of it.

use super::*;
use chrono::{Duration, Utc};

fn press(app: &mut App, c: char) {
    let modifiers = if c.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    if let Some(action) = app.handle_key(KeyEvent::new(KeyCode::Char(c), modifiers)) {
        app.apply(action);
    }
}

fn screen(app: &mut App) -> String {
    let backend = ratatui::backend::TestBackend::new(160, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

struct Snoozed {
    app: App,
    /// Two messages of one conversation, waking in two and in five days.
    thread: Vec<Envelope>,
    /// One message waking tomorrow, so it lists first.
    soonest: Envelope,
}

/// `g n` from the inbox, answered with three snoozed messages.
fn snoozed_app() -> Snoozed {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.envelopes = make_test_envelopes(3);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, 'n');
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Snoozed);
    assert!(
        app.mailbox.pending_snoozed_refresh,
        "opening Snoozed asks for what is snoozed"
    );

    let account_id = AccountId::new();
    let thread_id = ThreadId::new();
    let thread: Vec<Envelope> = ["Contract draft", "Re: Contract draft"]
        .into_iter()
        .map(|subject| {
            TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .thread_id(thread_id.clone())
                .subject(subject)
                .with_from_address("Maya", "maya@example.com")
                .build()
        })
        .collect();
    let soonest = TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .subject("Dentist on Friday")
        .with_from_address("Clinic", "clinic@example.com")
        .build();
    let now = Utc::now();
    let snooze = |envelope: &Envelope, days: i64| mxr_core::types::Snoozed {
        message_id: envelope.id.clone(),
        account_id: account_id.clone(),
        snoozed_at: now,
        wake_at: now + Duration::days(days),
        original_labels: Vec::new(),
    };
    let entries = vec![
        snooze(&thread[0], 2),
        snooze(&thread[1], 5),
        snooze(&soonest, 1),
    ];
    let mut envelopes = thread.clone();
    envelopes.push(soonest.clone());
    app.set_snoozed(entries, envelopes);
    Snoozed {
        app,
        thread,
        soonest,
    }
}

#[test]
fn g_n_lists_snoozed_mail_waking_soonest_first_with_its_wake_time() {
    let Snoozed {
        mut app, soonest, ..
    } = snoozed_app();
    let rows = app.mail_list_rows();
    assert_eq!(rows.len(), 2, "one row per conversation");
    assert_eq!(rows[0].representative.id, soonest.id);
    assert!(rows.iter().all(|row| row.wake_at.is_some()));
    let shown = screen(&mut app);
    assert!(shown.contains("Snoozed"), "{shown}");
    assert!(shown.contains("waking soonest first"), "{shown}");
    assert!(shown.contains("Dentist on Friday"), "{shown}");
    assert!(shown.contains("Clinic"), "{shown}");
    assert!(
        shown.contains("Back "),
        "the wake time is on the row: {shown}"
    );
    assert!(
        !shown.contains("Test Subject"),
        "the inbox behind it isn't shown: {shown}"
    );
}

#[test]
fn enter_opens_the_lit_snoozed_message() {
    let Snoozed {
        mut app, soonest, ..
    } = snoozed_app();
    if let Some(action) = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)) {
        app.apply(action);
    }
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(soonest.id)
    );
}

#[test]
fn w_wakes_every_snoozed_message_in_the_lit_conversation() {
    let Snoozed {
        mut app, thread, ..
    } = snoozed_app();
    app.mailbox.selected_index = 1;
    press(&mut app, 'w');
    let woken: Vec<MessageId> = queued(&app)
        .into_iter()
        .map(|request| match request {
            Request::Unsnooze { message_id } => message_id,
            other => panic!("expected unsnooze, got {other:?}"),
        })
        .collect();
    assert_eq!(
        woken,
        thread.iter().map(|env| env.id.clone()).collect::<Vec<_>>()
    );
    assert_eq!(app.mail_list_rows().len(), 1, "off the list at once");
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn the_lists_mail_keys_act_on_the_lit_snoozed_conversation() {
    let Snoozed {
        mut app, thread, ..
    } = snoozed_app();
    app.mailbox.selected_index = 1;
    press(&mut app, 'e');
    // Two messages, so the list's usual preview comes first.
    let confirm = app
        .modals
        .pending_bulk_confirm
        .as_ref()
        .expect("the archive preview");
    match &confirm.request {
        Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } => assert_eq!(
            message_ids,
            &thread.iter().map(|env| env.id.clone()).collect::<Vec<_>>()
        ),
        other => panic!("expected an archive, got {other:?}"),
    }
    assert_eq!(
        confirm.preview.rows.len(),
        2,
        "the preview lists the same two"
    );
}

#[test]
fn w_outside_snoozed_and_the_reply_queue_wakes_nothing() {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.envelopes = make_test_envelopes(2);
    press(&mut app, 'w');
    assert!(queued(&app).is_empty());
    assert!(app
        .status_message
        .as_deref()
        .is_some_and(|status| status.contains("g n opens Snoozed")));
}

#[test]
fn w_in_the_reply_queue_takes_the_lit_message_out_of_it() {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    let messages = make_test_envelopes(2);
    app.modals.reply_queue.open_loading();
    app.modals.reply_queue.set_messages(messages.clone());
    for message in &messages {
        app.mailbox
            .reply_later_message_ids
            .insert(message.id.clone());
    }
    app.modals.reply_queue.select_next();
    press(&mut app, 'w');
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetReplyLater { message_id, flag: false }] if *message_id == messages[1].id
    ));
    assert_eq!(app.modals.reply_queue.messages.len(), 1);
    assert_eq!(app.modals.reply_queue.messages[0].id, messages[0].id);
    assert!(app.modals.reply_queue.visible, "the queue stays open");
    assert!(!app
        .mailbox
        .reply_later_message_ids
        .contains(&messages[1].id));
}
