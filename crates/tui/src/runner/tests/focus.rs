//! Focus & reply in the TUI: `g F` opens the web's queue (the desk's You
//! owe lane, then the reply-later queue), `j` skips, `e` is the desk's Done
//! with undo, `b` puts it off, `r` writes the reply and its own send moves
//! on, and Esc or `q` goes back to where it was opened.

use super::*;
use crate::app::{FocusSource, PendingUndo, UndoAction};
use mxr_protocol::{DeskDoneItemData, DeskLaneData, DeskLaneKind, DeskRowData, PeopleSplitData};

fn press(app: &mut App, code: KeyCode) {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn g_capital_f(app: &mut App) {
    press(app, KeyCode::Char('g'));
    press(app, KeyCode::Char('F'));
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

fn owed_row(name: &str) -> DeskRowData {
    DeskRowData {
        lane: DeskLaneKind::Owed,
        account_id: AccountId::new(),
        thread_id: ThreadId::new(),
        message_id: MessageId::new(),
        message_ids: vec![],
        counterparty_email: format!("{}@example.com", name.to_lowercase()),
        counterparty_name: Some(name.to_string()),
        subject: format!("From {name}"),
        reason: "replied to your message".into(),
        since: chrono::Utc::now(),
        age_seconds: 2 * 86_400,
        usual_seconds: Some(4 * 3_600),
        usual_samples: 3,
        overdue: true,
        unread: true,
        starred: false,
        commitment_id: None,
        back_at: None,
    }
}

/// Focus & reply open from Now on two owed conversations and two
/// reply-later messages, one of them a conversation already owed.
fn focus_app() -> (App, Vec<DeskRowData>, Vec<Envelope>) {
    let mut app = App::new();
    app.apply(Action::OpenNow);
    g_capital_f(&mut app);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Focus);
    assert!(
        app.mailbox.focus_page.pending_refresh,
        "opening it fetches both sources"
    );
    let owed = vec![owed_row("Nora"), owed_row("Sam")];
    let mut later = make_test_envelopes(2);
    later[1].thread_id = owed[1].thread_id.clone();
    app.set_focus_sources(
        Ok((
            DeskLaneData {
                rows: owed.clone(),
                total: 2,
            },
            PeopleSplitData {
                write_with: 2,
                others: 1,
            },
        )),
        Ok(later.clone()),
    );
    (app, owed, later)
}

fn current_thread(app: &App) -> Option<ThreadId> {
    app.mailbox
        .focus_page
        .current()
        .map(|item| item.thread_id.clone())
}

#[test]
fn g_capital_f_opens_the_webs_queue_in_its_order() {
    assert!(matches!(
        crate::app::focus_desk_request(),
        Request::GetDesk {
            account_id: None,
            lane_limit: 5000
        }
    ));
    let (app, owed, later) = focus_app();
    let page = &app.mailbox.focus_page;
    assert_eq!(
        page.queue,
        vec![
            owed[0].thread_id.clone(),
            owed[1].thread_id.clone(),
            later[0].thread_id.clone()
        ],
        "You owe first, then reply later, each conversation once"
    );
    let first = page.current().unwrap();
    assert_eq!(first.sender, "Nora");
    assert!(first
        .reason
        .starts_with("Replied to your message \u{b7} 2d"));
    assert_eq!(page.others, 1);
    // Its newest message is fetched to show.
    assert!(app
        .mailbox
        .queued_body_fetches
        .contains(&owed[0].message_id));
    assert_eq!(app.mode_key_context(), Some("focus"));
}

#[test]
fn j_skips_to_the_end_and_e_puts_it_away_with_undo() {
    let (mut app, owed, later) = focus_app();
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
    assert_eq!(
        app.mailbox.focus_page.queue.last(),
        Some(&owed[0].thread_id)
    );

    press(&mut app, KeyCode::Char('e'));
    // The desk's Done, with the web's lane.
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::ResolveDeskItems { items, dry_run: false }]
            if items == &[DeskDoneItemData {
                thread_id: owed[1].thread_id.clone(),
                account_id: None,
                lane: Some(DeskLaneKind::Owed),
                commitment_id: None,
            }]
    ));
    assert_eq!(current_thread(&app), Some(later[0].thread_id.clone()));
    assert_eq!(app.mailbox.focus_page.progress().done, 1);

    // `u` once the daemon offers the undo: back in front after it lands.
    app.pending_mutation_queue.clear();
    app.pending_undo = Some(PendingUndo {
        action: UndoAction::Mutations(vec!["done-1".into()]),
        verb_past: "Done".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });
    press(&mut app, KeyCode::Char('u'));
    let undo = app.pending_mutation_queue.pop().expect("undo queued");
    assert!(matches!(
        &undo.request,
        Request::UndoMutation { mutation_id } if mutation_id == "done-1"
    ));
    app.apply_mutation_completion(undo.effect, true);
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
    assert_eq!(app.mailbox.focus_page.progress().done, 0);
}

#[test]
fn a_reply_later_done_lets_the_daemon_pick_the_lane_and_a_failure_puts_it_back() {
    let (mut app, _owed, later) = focus_app();
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(
        app.mailbox.focus_page.current().map(|item| item.source),
        Some(FocusSource::ReplyLater)
    );
    press(&mut app, KeyCode::Char('e'));
    let done = app.pending_mutation_queue.pop().expect("done queued");
    assert!(matches!(
        &done.request,
        Request::ResolveDeskItems { items, dry_run: false }
            if items == &[DeskDoneItemData {
                thread_id: later[0].thread_id.clone(),
                account_id: None,
                lane: None,
                commitment_id: None,
            }]
    ));
    app.handle_mutation_failure_result(done.id, false, &MxrError::Ipc("offline".into()));
    assert_eq!(current_thread(&app), Some(later[0].thread_id.clone()));
}

#[test]
fn send_and_next_moves_on_only_for_the_reply_written_here() {
    let (mut app, owed, _later) = focus_app();
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(
        app.compose.pending_compose,
        Some(crate::app::ComposeAction::Reply {
            message_id: owed[0].message_id.clone(),
            account_id: owed[0].account_id.clone(),
            preloaded: None,
        })
    );
    // While the reply opens, j and e wait.
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(current_thread(&app), Some(owed[0].thread_id.clone()));
    // The runner opens the editor when ComposeReady arrives.
    app.compose.pending_compose = None;
    app.note_focus_compose(DraftIntent::Reply, std::path::Path::new("/tmp/focus-1.md"));

    let sent = |path: Option<&str>| MutationEffect::SentSuccess {
        status: "Sent!".into(),
        remind_at: None,
        sent_message_id: Some(MessageId::new()),
        draft_path: path.map(std::path::PathBuf::from),
    };
    // Another send, and one with no compose file, leave it on screen.
    app.apply_mutation_completion(sent(Some("/tmp/other.md")), true);
    app.apply_mutation_completion(sent(None), true);
    assert_eq!(current_thread(&app), Some(owed[0].thread_id.clone()));

    app.apply_mutation_completion(sent(Some("/tmp/focus-1.md")), true);
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
    assert_eq!(app.mailbox.focus_page.progress().done, 1);
    assert_eq!(
        app.compose.pending_compose, None,
        "the next reply waits for r"
    );
}

#[test]
fn remind_and_d_open_the_reply_with_the_reminder_or_the_writing_model_after() {
    let (mut app, owed, _later) = focus_app();
    // w is Wake now: it says where Send, remind went and runs nothing.
    let before = app.status_message.clone();
    press(&mut app, KeyCode::Char('w'));
    assert!(!app.compose.remind_after_editor);
    assert_eq!(
        app.status_message.as_deref(),
        notice_while_announced(
            "focus",
            "w",
            "Send, remind has no key now; find it in the palette",
            before.as_deref()
        )
    );
    app.apply(Action::FocusRemind);
    assert!(app.compose.remind_after_editor);
    assert!(matches!(
        &app.compose.pending_compose,
        Some(crate::app::ComposeAction::Reply { message_id, .. }) if *message_id == owed[0].message_id
    ));

    let (mut app, _owed, _later) = focus_app();
    press(&mut app, KeyCode::Char('d'));
    assert!(app.compose.assist_after_editor);
    assert!(app.compose.pending_compose.is_some());
}

#[test]
fn b_puts_off_the_conversation_on_screen() {
    let (mut app, owed, _later) = focus_app();
    press(&mut app, KeyCode::Char('b'));
    assert_eq!(
        app.modals.snooze_panel.target,
        Some(crate::app::NotNowTarget::Mail {
            message_id: owed[0].message_id.clone()
        })
    );
    // Enter on Later, the first row: reply later, and the next one is up.
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetReplyLater { message_id, flag: true }] if *message_id == owed[0].message_id
    ));
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
}

#[test]
fn esc_and_q_go_back_to_where_it_was_opened() {
    let (mut app, _owed, _later) = focus_app();
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Now);

    let mut app = App::new();
    app.apply(Action::OpenUpdates);
    g_capital_f(&mut app);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Focus);
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Updates);
    assert!(!app.should_quit, "q never quits from Focus & reply");
}

#[test]
fn f_in_the_reply_queue_opens_focus_at_the_selected_message() {
    let mut app = App::new();
    let messages = make_test_envelopes(3);
    app.modals.reply_queue.open_loading();
    app.modals.reply_queue.set_messages(messages.clone());
    app.modals.reply_queue.select_next();
    press(&mut app, KeyCode::Char('F'));
    assert!(!app.modals.reply_queue.visible);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Focus);
    app.set_focus_sources(
        Ok((
            DeskLaneData {
                rows: vec![owed_row("Nora")],
                total: 1,
            },
            PeopleSplitData::default(),
        )),
        Ok(messages.clone()),
    );
    assert_eq!(current_thread(&app), Some(messages[1].thread_id.clone()));
}
