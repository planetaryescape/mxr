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
fn messages_app(hints_seen: bool) -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('m'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::People);
    assert!(
        app.mailbox.messages_page.pending_refresh && app.mailbox.pending_rail_refresh,
        "opening Messages fetches it, its guide and the rail"
    );
    let loaded = page(populated(), hints_seen);
    app.set_messages(loaded.messages.unwrap(), loaded.guide);
    let (row_id, topic) = app
        .mailbox
        .messages_page
        .pending_person
        .take()
        .expect("the selected row's page is asked for");
    let samir = samir_page();
    assert_eq!(row_id, samir.row.id);
    app.set_person_page(row_id, topic, samir);
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
        html: "<p>Hi Samir,</p>\n<p>Got it, thanks.</p>\n<p>Alex</p>".into(),
        built_from: "Your usual greeting and sign-off with Samir.".into(),
        countdown_seconds: 5,
        dry_run: true,
        from: "alex@example.com".into(),
        preview_token: Some("1.test".into()),
        preview_expires_at: None,
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
            [Request::AckMessage { dry_run: false, expect_text: Some(text), thread_id, .. }]
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
fn got_it_explains_itself_in_the_status_line_and_pressing_it_dismisses_the_hint() {
    let mut app = messages_app(false);
    assert!(app.active_hint().is_none(), "not on arrival");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        app.active_hint().map(|h| h.id.as_str()),
        Some("messages.got_it")
    );
    press(&mut app, KeyCode::Char('.'));
    assert!(queued(&app).iter().any(|request| matches!(
        request,
        Request::SetHintSeen { hint, seen: true } if hint == "messages.got_it"
    )));
}

#[test]
fn esc_takes_the_hint_first_then_goes_back() {
    let mut app = messages_app(false);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert!(app.active_hint().is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.active_hint().is_none());
    assert_eq!(queued(&app).len(), 1);
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

/// `.` pressed and its preview shown: a Got it counting down from `start`.
fn counting_down(app: &mut App, start: std::time::Instant) -> AckPlanData {
    press(app, KeyCode::Char('.'));
    assert!(app
        .mailbox
        .messages_page
        .pending_ack_preview
        .take()
        .is_some());
    let preview = plan(app);
    app.show_messages_ack(preview.clone(), start);
    assert!(
        app.mailbox.messages_page.ack.is_some(),
        "the countdown runs"
    );
    preview
}

fn after_send_at(start: std::time::Instant) -> std::time::Instant {
    start + std::time::Duration::from_secs(10)
}

#[test]
fn the_send_carries_the_previews_token() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    let preview = counting_down(&mut app, start);
    app.tick_messages_ack(after_send_at(start));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::AckMessage { preview_token: Some(token), .. }]
            if Some(token) == preview.preview_token.as_ref()
    ));
}

#[test]
fn going_to_another_mode_mid_countdown_sends_nothing() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    counting_down(&mut app, start);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Now);
    assert!(
        app.mailbox.messages_page.ack.is_none(),
        "leaving cancels it"
    );
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
}

#[test]
fn the_sidebar_taking_focus_mid_countdown_sends_nothing() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    counting_down(&mut app, start);
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.mailbox.active_pane, ActivePane::Sidebar);
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
    assert!(app.mailbox.messages_page.ack.is_none());
}

#[test]
fn another_screen_mid_countdown_sends_nothing() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    counting_down(&mut app, start);
    app.apply(Action::OpenTab2);
    assert_ne!(app.screen, Screen::Mailbox);
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
}

#[test]
fn moving_to_another_person_mid_countdown_sends_nothing() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    counting_down(&mut app, start);
    press(&mut app, KeyCode::Char('j'));
    assert!(app.mailbox.messages_page.ack.is_none());
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
}

#[test]
fn switching_topic_mid_countdown_sends_nothing() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    counting_down(&mut app, start);
    press(&mut app, KeyCode::Char(']'));
    assert!(app.mailbox.messages_page.ack.is_none());
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
}

#[test]
fn quitting_mid_countdown_sends_nothing() {
    let mut app = messages_app(true);
    let start = std::time::Instant::now();
    counting_down(&mut app, start);
    app.should_quit = true;
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
}

#[test]
fn a_late_preview_after_leaving_registers_nothing() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('.'));
    assert!(app
        .mailbox
        .messages_page
        .pending_ack_preview
        .take()
        .is_some());
    let preview = plan(&app);
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('h'));
    let start = std::time::Instant::now();
    app.show_messages_ack(preview, start);
    assert!(
        app.mailbox.messages_page.ack.is_none(),
        "the late preview is dropped"
    );
    app.tick_messages_ack(after_send_at(start));
    assert!(queued(&app).is_empty());
}

#[test]
fn a_late_preview_after_undo_or_for_another_thread_registers_nothing() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('.'));
    app.mailbox.messages_page.pending_ack_preview = None;
    press(&mut app, KeyCode::Esc);
    app.show_messages_ack(plan(&app), std::time::Instant::now());
    assert!(
        app.mailbox.messages_page.ack.is_none(),
        "Esc before the preview came back cancels it"
    );

    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('.'));
    app.mailbox.messages_page.pending_ack_preview = None;
    let mut other = plan(&app);
    other.thread_id = ThreadId::new();
    app.show_messages_ack(other, std::time::Instant::now());
    assert!(
        app.mailbox.messages_page.ack.is_none(),
        "not the thread that asked"
    );
}

#[test]
fn a_preview_nobody_asked_for_registers_nothing() {
    let mut app = messages_app(true);
    app.show_messages_ack(plan(&app), std::time::Instant::now());
    assert!(app.mailbox.messages_page.ack.is_none());
}

/// Where the row with this id sits in the list.
fn at(app: &App, id: &str) -> usize {
    let page = &app.mailbox.messages_page;
    (0..page.item_count())
        .find(|&index| page.row_at(index).is_some_and(|row| row.id == id))
        .unwrap()
}

fn only_note(app: &App) -> crate::app::DoneNote {
    let notes = &app.mailbox.messages_page.done_notes;
    assert_eq!(notes.len(), 1, "one note, under the done's mutation");
    notes.values().next().cloned().unwrap()
}

/// The page the daemon would send for `row` on its first topic.
fn page_of(row: &mxr_protocol::MessagesRowData) -> mxr_protocol::PersonPageData {
    let mut page = samir_page();
    let topic = row.topics[0].clone();
    let conversation = page.conversation.as_mut().unwrap();
    conversation.thread_id = topic.thread_id.clone();
    conversation.subject = topic.subject.clone();
    page.topics = row.topics.clone();
    page.row = row.clone();
    page
}

/// Deliver the page the lens asked for last.
fn answer(app: &mut App, page: mxr_protocol::PersonPageData) {
    let (row_id, topic) = app.mailbox.messages_page.pending_person.take().unwrap();
    app.set_person_page(row_id, topic, page);
}

fn selected_row_id(app: &App) -> Option<String> {
    app.selected_messages_row().map(|row| row.id.clone())
}

fn subject_thread(app: &App, subject: &str) -> mxr_core::id::ThreadId {
    app.mailbox
        .messages_page
        .page
        .as_ref()
        .unwrap()
        .topics
        .iter()
        .find(|topic| topic.subject == subject)
        .unwrap()
        .thread_id
        .clone()
}

#[test]
fn done_with_topics_left_stays_on_the_person_and_opens_the_next_topic() {
    let mut app = messages_app(true);
    let samir = selected_row_id(&app).unwrap();
    let launch = subject_thread(&app, "Launch checklist");
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(selected_row_id(&app), Some(samir.clone()), "the row stays");
    assert_eq!(
        app.mailbox.messages_page.pending_person,
        Some((samir, Some(launch))),
        "the page asks for the next topic still in Messages, never the one done"
    );
    let note = only_note(&app);
    assert_eq!(
        note.line("Done. Archived in Gmail. u to undo"),
        "Done: Contract renewal. Next: Launch checklist. Archived in Gmail. u to undo"
    );
}

#[test]
fn done_on_a_persons_last_topic_moves_to_the_next_person() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(
        selected_row_id(&app).as_deref(),
        Some("person:jon@example.com")
    );
    // `e` waits for Jon's page; then it is done for his topic.
    press(&mut app, KeyCode::Char('e'));
    assert!(queued(&app).is_empty(), "nothing shown yet, nothing done");
    let jon = app.selected_messages_row().unwrap().clone();
    answer(&mut app, page_of(&jon));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        selected_row_id(&app).as_deref(),
        Some("person:maya@example.com"),
        "Jon has nothing left: the row after him opens"
    );
    let note = only_note(&app);
    assert_eq!(note.line("Done."), "Done with Jon Bell. Next: Maya Ortiz.");
}

#[test]
fn done_on_the_last_row_moves_to_the_previous_person() {
    let mut app = messages_app(true);
    app.mailbox.selected_index = at(&app, "person:iris@example.com");
    app.sync_messages_page();
    let iris = app.selected_messages_row().unwrap().clone();
    answer(&mut app, page_of(&iris));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        selected_row_id(&app).as_deref(),
        Some("person:maya@example.com")
    );
}

#[test]
fn a_second_e_before_the_next_page_arrives_sends_nothing_again() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('e'));
    press(&mut app, KeyCode::Char('e'));
    let dones = queued(&app)
        .into_iter()
        .filter(|request| matches!(request, Request::SetModeDone { .. }))
        .count();
    assert_eq!(dones, 1, "the page on screen is the one just done");
}

#[test]
fn a_refetch_keeps_the_selection_on_the_person_not_the_position() {
    let mut app = messages_app(true);
    let iris = "person:iris@example.com";
    app.mailbox.selected_index = at(&app, iris);
    let mut data = populated();
    let row = data.recent.remove(0);
    data.your_turn.insert(0, row);
    app.set_messages(data, None);
    assert_eq!(selected_row_id(&app).as_deref(), Some(iris));
}

#[test]
fn a_person_removed_by_sync_hands_the_selection_to_their_neighbour() {
    let mut app = messages_app(true);
    app.mailbox.selected_index = at(&app, "person:iris@example.com");
    let mut data = populated();
    data.recent.clear();
    data.your_turn.remove(0);
    app.set_messages(data, None);
    assert_eq!(
        selected_row_id(&app).as_deref(),
        Some("person:maya@example.com"),
        "the previous person, not the Quiet line that now sits at that position"
    );
}

#[test]
fn a_person_moved_into_quiet_with_topics_left_stays_in_view() {
    let mut app = messages_app(true);
    let iris = "person:iris@example.com";
    app.mailbox.selected_index = at(&app, iris);
    let mut data = populated();
    let row = data.recent.remove(0);
    data.quiet.insert(0, row);
    app.set_messages(data, None);
    assert!(app.mailbox.messages_page.quiet_open);
    assert_eq!(selected_row_id(&app).as_deref(), Some(iris));
}

fn dones(app: &App) -> usize {
    queued(app)
        .into_iter()
        .filter(|request| matches!(request, Request::SetModeDone { .. }))
        .count()
}

#[test]
fn e_waits_for_the_next_persons_page_before_doing_anything() {
    let mut app = messages_app(true);
    // Jon, then Iris: nobody pinned between them.
    let mut data = populated();
    data.pinned.clear();
    app.set_messages(data, None);
    press(&mut app, KeyCode::Char('j'));
    let jon = app.selected_messages_row().unwrap().clone();
    answer(&mut app, page_of(&jon));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        selected_row_id(&app).as_deref(),
        Some("person:iris@example.com")
    );
    // Iris's page hasn't come: `e` must not take her first topic unseen.
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(dones(&app), 1);
    let iris = app.selected_messages_row().unwrap().clone();
    answer(&mut app, page_of(&iris));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        dones(&app),
        2,
        "once her page is on screen, e is done for it"
    );
}

#[test]
fn a_late_answer_for_the_topic_just_done_never_replaces_the_next_one() {
    let mut app = messages_app(true);
    let samir = samir_page();
    let contract = samir.topics[0].thread_id.clone();
    let launch = subject_thread(&app, "Launch checklist");
    press(&mut app, KeyCode::Char('e'));
    // The done finishes before the next topic's page arrives: the refetch
    // asks again for the next topic, not the one still on screen.
    app.refresh_messages();
    assert_eq!(
        app.mailbox.messages_page.pending_person,
        Some((samir.row.id.clone(), Some(launch.clone())))
    );
    let mut next = samir.clone();
    next.conversation.as_mut().unwrap().thread_id = launch.clone();
    answer(&mut app, next);
    // An answer for the done topic, asked for earlier, lands last.
    app.set_person_page(samir.row.id.clone(), Some(contract), samir);
    let shown = app.mailbox.messages_page.page.as_ref().unwrap();
    assert_eq!(shown.conversation.as_ref().unwrap().thread_id, launch);
}

#[test]
fn the_same_address_in_two_accounts_keeps_its_own_row_across_a_refetch() {
    let mut app = messages_app(true);
    let mut data = populated();
    let mut other = data.your_turn[0].clone();
    other.account_id = mxr_core::AccountId::from_provider_id("fake", "work");
    data.your_turn.push(other.clone());
    app.set_messages(data.clone(), None);
    // The work account's Samir, last in Your turn.
    app.mailbox.selected_index = data.your_turn.len() - 1;
    assert_eq!(
        app.selected_messages_row().unwrap().account_id,
        other.account_id
    );
    // A refetch puts the work account's row first.
    let row = data.your_turn.pop().unwrap();
    data.your_turn.insert(0, row);
    app.set_messages(data, None);
    assert_eq!(app.mailbox.selected_index, 0);
    assert_eq!(
        app.selected_messages_row().unwrap().account_id,
        other.account_id
    );
}

#[test]
fn another_modes_done_never_borrows_messages_words() {
    let mut app = messages_app(true);
    press(&mut app, KeyCode::Char('e'));
    let messages_done = *app.mailbox.messages_page.done_notes.keys().next().unwrap();
    let other = app.mutation_id_generator.next_id();
    let copy = || crate::app::MutationEffect::ModeDone("Done. Archived in Gmail.".into());
    let plain = app.with_done_note(other, copy());
    assert!(
        matches!(plain, crate::app::MutationEffect::ModeDone(ref msg) if msg == "Done. Archived in Gmail."),
        "{plain:?}"
    );
    let named = app.with_done_note(messages_done, copy());
    assert!(
        matches!(named, crate::app::MutationEffect::ModeDone(ref msg)
            if msg.starts_with("Done: Contract renewal. Next: Launch checklist.")),
        "{named:?}"
    );
}
