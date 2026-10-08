//! Sorting shows its work in the TUI (D119): `X` moves one email and `K`
//! its sender through `MoveMessage`, `u` undoes a move with its correction
//! id, a Not-sure question on Now takes one key and then asks "Always for
//! this sender? y/n" once, Enter on the arrivals line lists the emails
//! behind it, and Inbox rows carry their mode.

use super::*;
use crate::app::NowRow;
use crate::ui::now_lens::tests::{page, populated};
use chrono::Utc;
use mxr_protocol::{
    ArrivalBucketData, ArrivalCountData, ArrivalItemData, ArrivalsData, DaemonEvent, ModeKindData,
    MoveChoiceData, MoveOutcomeData, NotSureData,
};

fn press(app: &mut App, code: KeyCode) {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

/// The mail list with three emails, the first selected.
fn inbox_app() -> App {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = MailboxView::Messages;
    app.mailbox.active_pane = ActivePane::MailList;
    app.mailbox.mail_list_mode = MailListMode::Messages;
    app.mailbox.envelopes = make_test_envelopes(3);
    app
}

fn moved(request: &Request) -> Option<(MessageId, ModeKindData, bool, Option<&str>)> {
    match request {
        Request::MoveMessage {
            message_id,
            mode,
            sender,
            dry_run: false,
            source,
        } => Some((message_id.clone(), *mode, *sender, source.as_deref())),
        _ => None,
    }
}

fn question(subject: &str) -> NotSureData {
    NotSureData {
        account_id: mxr_core::AccountId::new(),
        message_id: MessageId::new(),
        thread_id: ThreadId::new(),
        sender_email: "maya@example.com".into(),
        sender_name: Some("Maya Ortiz".into()),
        subject: subject.into(),
        mode: ModeKindData::Updates,
        line: format!("Maya Ortiz copied you on \"{subject}\". Updates for now."),
        choices: MoveChoiceData::all(),
    }
}

pub(crate) fn arrivals(not_sure: Vec<NotSureData>) -> ArrivalsData {
    let now = Utc::now();
    ArrivalsData {
        generated_at: now,
        since: now - chrono::Duration::hours(3),
        until: now,
        since_label: "08:12".into(),
        total: 50,
        counts: vec![
            ArrivalCountData {
                bucket: ArrivalBucketData::Messages,
                count: 8,
                label: "8 Messages".into(),
            },
            ArrivalCountData {
                bucket: ArrivalBucketData::Updates,
                count: 10,
                label: "10 Updates".into(),
            },
            ArrivalCountData {
                bucket: ArrivalBucketData::Reading,
                count: 31,
                label: "31 Reading".into(),
            },
            ArrivalCountData {
                bucket: ArrivalBucketData::Spam,
                count: 1,
                label: "1 spam".into(),
            },
        ],
        also: vec![ArrivalCountData {
            bucket: ArrivalBucketData::Todo,
            count: 2,
            label: "2 in To do".into(),
        }],
        line: "Since 08:12: 50 arrived. 8 Messages · 10 Updates · 31 Reading · 1 spam. Also 2 in To do."
            .into(),
        clear_line: Some("Clear. All 50 emails since 08:12 are accounted for.".into()),
        latest_at: Some(now),
        not_sure_line: (!not_sure.is_empty())
            .then(|| format!("{} emails I wasn't sure about. Where should these go?", not_sure.len())),
        not_sure_hint: (!not_sure.is_empty()).then(|| {
            mxr_protocol::arrivals_copy::NOT_SURE_HINT.to_string()
        }),
        not_sure,
        track_record: Some("Last week mxr sorted 310 emails; you moved 2.".into()),
        never_bury: mxr_protocol::arrivals_copy::NEVER_BURY.into(),
    }
}

/// Now open with its line and two Not-sure questions loaded.
fn now_with_questions() -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('h'));
    assert!(
        app.mailbox.now_page.pending_mark_seen,
        "opening Now starts a visit"
    );
    let loaded = page(populated(), true);
    app.set_now(loaded.now.unwrap(), loaded.guide);
    app.set_arrivals(arrivals(vec![question("Q4 plan"), question("Offsite")]));
    app
}

fn outcome(message_id: MessageId, to: ModeKindData, ask: bool) -> MoveOutcomeData {
    MoveOutcomeData {
        account_id: mxr_core::AccountId::new(),
        message_id,
        thread_id: ThreadId::new(),
        sender_email: "maya@example.com".into(),
        from: ArrivalBucketData::Updates,
        to,
        sender: false,
        dry_run: false,
        copy: format!("Moved to {}.", to.name()),
        ask_sender: ask.then(|| "Always for this sender? (K)".into()),
        hint: ask.then(|| mxr_protocol::arrivals_copy::MOVE_HINT.into()),
        correction_id: Some(7),
        aspect_id: None,
    }
}

#[test]
fn x_opens_the_move_menu_and_one_key_moves_this_email() {
    for (key, mode) in [
        ('m', ModeKindData::Messages),
        ('x', ModeKindData::Todo),
        ('u', ModeKindData::Updates),
        ('r', ModeKindData::Reading),
        ('e', ModeKindData::Archive),
    ] {
        let mut app = inbox_app();
        let selected = app.selected_envelope().unwrap().id.clone();
        press(&mut app, KeyCode::Char('X'));
        let menu = app
            .mailbox
            .trust
            .move_menu
            .as_ref()
            .expect("X opens the menu");
        assert_eq!(menu.message_id, selected);
        assert!(!menu.sender_only);
        press(&mut app, KeyCode::Char(key));
        assert!(app.mailbox.trust.move_menu.is_none(), "{key} closes it");
        assert_eq!(
            queued(&app).iter().filter_map(moved).collect::<Vec<_>>(),
            vec![(selected, mode, false, None)],
            "{key}"
        );
    }
}

#[test]
fn shift_in_the_menu_and_k_move_the_sender_to_a_mode_a_sender_can_live_in() {
    let mut app = inbox_app();
    let selected = app.selected_envelope().unwrap().id.clone();
    press(&mut app, KeyCode::Char('X'));
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(
        queued(&app).iter().filter_map(moved).collect::<Vec<_>>(),
        vec![(selected.clone(), ModeKindData::Reading, true, None)]
    );

    let mut app = inbox_app();
    let selected = app.selected_envelope().unwrap().id.clone();
    press(&mut app, KeyCode::Char('K'));
    let menu = app
        .mailbox
        .trust
        .move_menu
        .as_ref()
        .expect("K opens the sender menu");
    assert!(menu.sender_only);
    // To do and Archive hold one email, never a sender.
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('e'));
    assert!(queued(&app).is_empty());
    assert!(app.mailbox.trust.move_menu.is_some(), "still open");
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(
        queued(&app).iter().filter_map(moved).collect::<Vec<_>>(),
        vec![(selected, ModeKindData::Updates, true, None)]
    );

    // Esc closes without a move.
    let mut app = inbox_app();
    press(&mut app, KeyCode::Char('X'));
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.trust.move_menu.is_none());
    assert!(queued(&app).is_empty());
}

#[test]
fn x_works_in_the_reader_and_on_now_rows() {
    let mut app = inbox_app();
    app.mailbox.active_pane = ActivePane::MessageView;
    app.mailbox.viewed_thread_messages = make_test_envelopes(2);
    app.mailbox.thread_selected_index = 1;
    let focused = app.mailbox.viewed_thread_messages[1].id.clone();
    press(&mut app, KeyCode::Char('X'));
    assert_eq!(
        app.mailbox
            .trust
            .move_menu
            .as_ref()
            .map(|m| m.message_id.clone()),
        Some(focused),
        "the reader moves the message in focus"
    );

    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('h'));
    let loaded = page(populated(), true);
    app.set_now(loaded.now.unwrap(), loaded.guide);
    let person = app.mailbox.now_page.now.as_ref().unwrap().people.rows[0]
        .row
        .message_id
        .clone();
    press(&mut app, KeyCode::Char('X'));
    assert_eq!(
        app.mailbox
            .trust
            .move_menu
            .as_ref()
            .map(|m| m.message_id.clone()),
        Some(person)
    );
}

#[test]
fn u_after_a_move_undoes_it_with_its_correction_id() {
    let mut app = inbox_app();
    app.set_pending_undo(crate::app::PendingUndo {
        action: crate::app::UndoAction::Move(7),
        verb_past: "Moved".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::UndoMove { correction_id: 7 }]
    ));
}

#[test]
fn after_a_move_k_says_always_for_this_sender_at_once() {
    let mut app = inbox_app();
    let message = MessageId::new();
    app.after_move(
        &outcome(message.clone(), ModeKindData::Reading, true),
        false,
    );
    assert!(
        app.mailbox.trust.move_hint_shown,
        "the first move teaches X and K"
    );
    press(&mut app, KeyCode::Char('K'));
    assert!(
        app.mailbox.trust.move_menu.is_none(),
        "no menu: K answered yes"
    );
    assert_eq!(
        queued(&app).iter().filter_map(moved).collect::<Vec<_>>(),
        vec![(message, ModeKindData::Reading, true, None)]
    );

    // Any other key lets the question go; a later K opens the menu.
    let mut app = inbox_app();
    app.after_move(
        &outcome(MessageId::new(), ModeKindData::Reading, true),
        false,
    );
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('K'));
    assert!(app.mailbox.trust.move_menu.is_some());
    assert!(queued(&app).is_empty());
}

#[test]
fn the_move_toast_is_the_daemons_copy_and_its_question() {
    let message = MessageId::new();
    assert_eq!(
        crate::app::move_status(
            &outcome(message.clone(), ModeKindData::Reading, true),
            false
        ),
        "Moved to Reading.  Always for this sender? (K)"
    );
    assert_eq!(
        crate::app::move_status(&outcome(message, ModeKindData::Messages, false), true),
        "Moved to Messages.  Always for this sender? y/n"
    );
}

#[test]
fn a_not_sure_question_takes_one_key_then_asks_about_the_sender_once() {
    let mut app = now_with_questions();
    // The line, its two questions, then Now's own rows.
    assert!(matches!(
        app.mailbox.now_page.rows()[..3],
        [NowRow::Arrivals(_), NowRow::NotSure(_), NowRow::NotSure(_)]
    ));
    assert_eq!(
        app.status_message.as_deref(),
        Some(mxr_protocol::arrivals_copy::NOT_SURE_HINT),
        "the first question carries the hint"
    );
    app.mailbox.selected_index = 1;
    let first = match app.selected_now_row() {
        Some(NowRow::NotSure(question)) => question.message_id.clone(),
        other => panic!("expected a question, got {other:?}"),
    };
    // u is Updates here, not undo; r is Reading, not reply.
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(
        queued(&app).iter().filter_map(moved).collect::<Vec<_>>(),
        vec![(
            first.clone(),
            ModeKindData::Updates,
            false,
            Some("not_sure")
        )]
    );
    assert_eq!(
        app.mailbox
            .now_page
            .arrivals
            .as_ref()
            .unwrap()
            .not_sure
            .len(),
        1,
        "the answer leaves Now at once"
    );
    let ask = app.mailbox.trust.sender_ask.clone().expect("asks once");
    assert!(ask.yes_no);
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        queued(&app).iter().filter_map(moved).next_back(),
        Some((first, ModeKindData::Updates, true, None))
    );
    assert!(app.mailbox.trust.sender_ask.is_none());

    // n answers no: nothing more is sent.
    let mut app = now_with_questions();
    app.mailbox.selected_index = 1;
    press(&mut app, KeyCode::Char('m'));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(queued(&app).len(), 1);
    assert!(app.mailbox.trust.sender_ask.is_none());

    // To do and Archive never ask about the sender.
    let mut app = now_with_questions();
    app.mailbox.selected_index = 1;
    press(&mut app, KeyCode::Char('x'));
    assert!(app.mailbox.trust.sender_ask.is_none());
}

#[test]
fn enter_on_the_arrivals_line_lists_the_emails_behind_it() {
    let mut app = now_with_questions();
    app.mailbox.selected_index = 0;
    press(&mut app, KeyCode::Enter);
    let line = app.mailbox.now_page.arrivals.clone().unwrap();
    let fetch = app
        .mailbox
        .trust
        .pending_list
        .clone()
        .expect("asks for the list");
    assert_eq!(
        (fetch.since, fetch.until, fetch.bucket),
        (line.since, line.until, None)
    );
    assert!(app.mailbox.trust.arrivals_list.is_some());

    // Tab walks the line's counts, then its "also"s.
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.mailbox.trust.pending_list.as_ref().unwrap().bucket,
        Some(ArrivalBucketData::Messages)
    );
    let fetch = app.mailbox.trust.pending_list.take().unwrap();
    let item = ArrivalItemData {
        account_id: mxr_core::AccountId::new(),
        message_id: MessageId::new(),
        thread_id: ThreadId::new(),
        sender_email: "maya@example.com".into(),
        sender_name: None,
        subject: "Q4 plan".into(),
        date: Utc::now(),
        first_seen_at: Utc::now(),
        arrived_in: ArrivalBucketData::Messages,
        bucket: ArrivalBucketData::Messages,
        reason: None,
        chip: "→ Messages · from a person".into(),
        moved: false,
        not_sure: false,
        unread: true,
        also_todo: false,
        also_archive: false,
    };
    app.set_arrivals_list(
        &fetch,
        mxr_protocol::ArrivalListData {
            since: fetch.since,
            until: fetch.until,
            bucket: fetch.bucket,
            total: 1,
            items: vec![item.clone()],
        },
    );
    // X there moves the listed email.
    press(&mut app, KeyCode::Char('X'));
    assert_eq!(
        app.mailbox
            .trust
            .move_menu
            .as_ref()
            .map(|m| m.message_id.clone()),
        Some(item.message_id)
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert!(
        app.mailbox.trust.arrivals_list.is_none(),
        "Esc closes the list"
    );
}

#[test]
fn inbox_rows_carry_their_mode_and_a_move_anywhere_refreshes_them() {
    let mut app = inbox_app();
    app.request_visible_chips();
    let asked = app
        .mailbox
        .trust
        .pending_chips
        .clone()
        .expect("asks for the chips");
    assert_eq!(asked.len(), 3);
    let first = asked[0].clone();
    app.mailbox.trust.set_chips(vec![ArrivalItemData {
        account_id: mxr_core::AccountId::new(),
        message_id: first.clone(),
        thread_id: ThreadId::new(),
        sender_email: "news@weekly.example".into(),
        sender_name: None,
        subject: "Issue 41".into(),
        date: Utc::now(),
        first_seen_at: Utc::now(),
        arrived_in: ArrivalBucketData::Reading,
        bucket: ArrivalBucketData::Reading,
        reason: Some("has List-Unsubscribe".into()),
        chip: "→ Reading · has List-Unsubscribe".into(),
        moved: false,
        not_sure: false,
        unread: true,
        also_todo: false,
        also_archive: false,
    }]);
    let rows = app.mail_list_rows();
    let row = rows
        .iter()
        .find(|row| row.representative.id == first)
        .unwrap();
    assert_eq!(row.mode_chip, Some("Reading"));

    handle_daemon_event(&mut app, DaemonEvent::ModesChanged { account_id: None });
    assert!(
        app.mailbox.trust.chips.is_empty(),
        "another client moved mail"
    );
    assert!(app.mailbox.now_page.pending_refresh);
}
