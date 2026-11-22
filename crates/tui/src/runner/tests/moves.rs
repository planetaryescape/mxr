use super::*;
use crate::app::NowRow;
use crate::ui::now_lens::tests::{page, populated};
use chrono::Utc;
use mxr_protocol::{
    ArrivalBucketData, ArrivalCountData, ArrivalsData, DaemonEvent, ModeKindData, MoveChoiceData,
    NotSureData,
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
        classification_report: None,
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
fn a_not_sure_question_answers_through_x_then_asks_about_the_sender_once() {
    let mut app = now_with_questions();
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
    press(&mut app, KeyCode::Char('X'));
    let menu = app.mailbox.trust.move_menu.as_ref().expect("the picker");
    assert!(menu.not_sure);
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
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        queued(&app).iter().filter_map(moved).next_back(),
        Some((first, ModeKindData::Updates, true, None))
    );
    assert!(app.mailbox.trust.sender_ask.is_none());

    let mut app = now_with_questions();
    app.mailbox.selected_index = 1;
    press(&mut app, KeyCode::Char('X'));
    press(&mut app, KeyCode::Char('m'));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(queued(&app).len(), 1);
    assert!(app.mailbox.trust.sender_ask.is_none());

    let mut app = now_with_questions();
    app.mailbox.selected_index = 1;
    press(&mut app, KeyCode::Char('X'));
    press(&mut app, KeyCode::Char('x'));
    assert!(app.mailbox.trust.sender_ask.is_none());
}

#[test]
fn an_account_address_change_drops_every_cached_reply_context() {
    let mut app = inbox_app();
    let message = app.mailbox.envelopes[0].id.clone();
    app.compose.reply_context_cache.insert(
        message.clone(),
        crate::app::ReplyContextPair {
            reply: None,
            reply_all: None,
        },
    );

    handle_daemon_event(
        &mut app,
        DaemonEvent::AccountAddressesChanged {
            account_id: mxr_core::AccountId::new(),
        },
    );
    assert!(
        !app.compose.reply_context_cache.contains_key(&message),
        "a reply cached before the alias existed would still offer the old To line"
    );
}
