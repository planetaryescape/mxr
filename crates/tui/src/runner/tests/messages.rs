//! Messages in the TUI: `g m` opens the people list and its page follows
//! the cursor, `[` and `]` step through topics, `.` shows Got it's exact
//! text and sends only after its countdown unless `u` undoes it, `e` is
//! done here through `SetModeDone`, and `s` pins.

use super::*;
use crate::app::{ComposeAction, MessagesFocus};
use crate::ui::messages_lens::tests::{page, populated, samir_page};
use mxr_protocol::{AckPlanData, ModeKindData};

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

/// Messages open on the fixture people, with Samir's page loaded.
fn messages_app(card_seen: bool) -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('m'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::People);
    assert!(
        app.mailbox.messages_page.pending_refresh && app.mailbox.pending_rail_refresh,
        "opening Messages fetches it, its guide and the rail"
    );
    let loaded = page(populated(), card_seen);
    app.set_messages(loaded.messages.unwrap(), loaded.guide);
    let (row_id, _) = app
        .mailbox
        .messages_page
        .pending_person
        .take()
        .expect("the selected row's page is asked for");
    let samir = samir_page();
    assert_eq!(row_id, samir.row.id);
    app.set_person_page(row_id, samir);
    app
}

fn plan(app: &App) -> AckPlanData {
    let conversation = app
        .mailbox
        .messages_page
        .page
        .as_ref()
        .unwrap()
        .conversation
        .clone()
        .unwrap();
    AckPlanData {
        account_id: conversation.account_id,
        thread_id: conversation.thread_id,
        reply_to_message_id: conversation.composer.reply_to_message_id,
        to: vec![mxr_core::types::Address {
            name: Some("Samir Patel".into()),
            email: "samir@example.com".into(),
        }],
        subject: "Re: Contract renewal".into(),
        text: "Hi Samir,\n\nGot it, thanks.\n\nAlex".into(),
        built_from: "Your usual greeting and sign-off with Samir.".into(),
        countdown_seconds: 5,
        dry_run: true,
        sent_message_id: None,
    }
}

#[test]
fn the_page_follows_the_cursor() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('j'));
    let (row_id, topic) = app
        .mailbox
        .messages_page
        .pending_person
        .clone()
        .expect("moving asks for the next person's page");
    assert_eq!(row_id, "person:jon@example.com");
    assert!(topic.is_none());
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mailbox.messages_page.focus, MessagesFocus::Person);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mailbox.messages_page.focus, MessagesFocus::List);
}

#[test]
fn brackets_step_through_the_persons_topics() {
    let mut app = messages_app(true);
    let topics = app
        .mailbox
        .messages_page
        .page
        .as_ref()
        .unwrap()
        .topics
        .clone();
    press(&mut app, KeyCode::Char(']'));
    let (_, topic) = app.mailbox.messages_page.pending_person.take().unwrap();
    assert_eq!(topic, Some(topics[1].thread_id.clone()));
    press(&mut app, KeyCode::Char('['));
    let (_, topic) = app.mailbox.messages_page.pending_person.take().unwrap();
    assert_eq!(
        topic,
        Some(topics[2].thread_id.clone()),
        "wraps to the last"
    );
}

#[test]
fn got_it_previews_counts_down_and_sends_exactly_that_text() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('.'));
    let thread = app
        .mailbox
        .messages_page
        .pending_ack_preview
        .take()
        .expect("Got it asks for the preview first");
    let preview = plan(&app);
    assert_eq!(thread, preview.thread_id);
    let start = std::time::Instant::now();
    app.show_messages_ack(preview.clone(), start);
    assert!(
        queued(&app).is_empty(),
        "nothing is sent during the countdown"
    );
    app.tick_messages_ack(start + std::time::Duration::from_secs(4));
    assert!(queued(&app).is_empty());
    app.tick_messages_ack(start + std::time::Duration::from_secs(5));
    let sent = queued(&app);
    assert!(
        matches!(
            sent.as_slice(),
            [Request::AckMessage { dry_run: false, expect_text: Some(text), thread_id }]
                if *text == preview.text && *thread_id == preview.thread_id
        ),
        "{sent:?}"
    );
    assert!(app.mailbox.messages_page.ack.is_none());
    assert!(
        app.mailbox
            .messages_page
            .messages
            .as_ref()
            .unwrap()
            .your_turn
            .iter()
            .all(|row| row.id != "person:samir@example.com"),
        "Samir leaves Your turn at once"
    );
}

#[test]
fn u_during_the_countdown_sends_nothing() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('.'));
    app.mailbox.messages_page.pending_ack_preview = None;
    let start = std::time::Instant::now();
    app.show_messages_ack(plan(&app), start);
    press(&mut app, KeyCode::Char('u'));
    assert!(app.mailbox.messages_page.ack.is_none());
    app.tick_messages_ack(start + std::time::Duration::from_secs(10));
    assert!(queued(&app).is_empty());
}

#[test]
fn e_is_done_in_messages_for_the_open_topic() {
    let mut app = messages_app(true);
    let thread = app
        .mailbox
        .messages_page
        .page
        .as_ref()
        .unwrap()
        .conversation
        .as_ref()
        .unwrap()
        .thread_id
        .clone();
    press(&mut app, KeyCode::Char('e'));
    let sent = queued(&app);
    assert!(
        matches!(
            sent.as_slice(),
            [Request::SetModeDone { thread_ids, mode: ModeKindData::Messages, dry_run: false, .. }]
                if *thread_ids == vec![thread.clone()]
        ),
        "{sent:?}"
    );
}

#[test]
fn s_pins_the_person() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('s'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::WatchCadence { email, .. }] if email == "samir@example.com"
    ));
}

#[test]
fn the_card_retires_on_esc_and_on_got_it() {
    let mut app = messages_app(false);
    assert!(app.mailbox.messages_page.card_visible());
    press(&mut app, KeyCode::Esc);
    assert!(!app.mailbox.messages_page.card_visible());
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeGuideSeen { mode, seen: true }] if mode == "messages"
    ));
    let mut app = messages_app(false);
    press(&mut app, KeyCode::Char('.'));
    assert!(!app.mailbox.messages_page.card_visible());
}

#[test]
fn r_replies_to_the_open_topic_and_help_leads_with_messages() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('r'));
    let expected = app
        .mailbox
        .messages_page
        .page
        .as_ref()
        .unwrap()
        .conversation
        .as_ref()
        .unwrap()
        .composer
        .reply_to_message_id
        .clone();
    assert!(matches!(
        &app.compose.pending_compose,
        Some(ComposeAction::Reply { message_id, .. }) if *message_id == expected
    ));
    assert_eq!(
        app.help_mode_guide().map(|g| g.mode.as_str()),
        Some("messages")
    );
}
