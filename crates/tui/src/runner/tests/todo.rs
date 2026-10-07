//! To do in the TUI: `g x` opens the runway and its guide, the row's verbs
//! become the daemon requests they name, hints show in the status line at
//! their row, and `t` makes a to-do from a conversation.

use super::*;
use crate::app::{TodoPanel, TodoPromptKind};
use crate::ui::todo_lens::tests::{council_tax, page, runway, todo};
use mxr_protocol::{TodoCatchupDecisionData, TodoChangeData, TodoStateActionData};

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn press_shifted(app: &mut App, c: char) {
    if let Some(action) = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)) {
        app.apply(action);
    }
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

/// To do open on a runway of the bill and one undated row.
fn todo_app(hints_seen: bool) -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Todo);
    assert!(
        app.mailbox.todo_page.pending_refresh,
        "opening To do fetches the runway and its guide"
    );
    let loaded = page(
        runway(vec![council_tax(), todo("todo_9", "Send the form")], vec![]),
        hints_seen,
    );
    app.set_todo_runway(loaded.runway.unwrap(), loaded.guide);
    app
}

#[test]
fn g_x_opens_to_do_and_enter_opens_the_email_never_the_link() {
    let mut app = todo_app(true);
    let message_id = app.mailbox.todo_page.runway.as_ref().unwrap().now[0]
        .source_message_id
        .clone();
    assert!(message_id.is_some());
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.mailbox
            .todo_page
            .pending_open
            .as_ref()
            .map(|open| open.message_id.clone()),
        message_id,
        "Open email to pay opens the email"
    );
    assert!(app
        .status_message
        .as_deref()
        .is_some_and(|status| status.contains("camden.gov.uk")));
}

#[test]
fn e_ticks_off_at_once_and_u_puts_it_back() {
    let mut app = todo_app(true);
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoState { todo_ids, action: TodoStateActionData::Done, dry_run: false }]
            if todo_ids == &["todo_1".to_string()]
    ));
    assert_eq!(
        app.mailbox.todo_page.runway_rows()[0].title,
        "Send the form",
        "the row folds away and the next takes the cursor"
    );
    // The daemon's answer offers undo; `u` sends it.
    app.set_pending_undo(crate::app::PendingUndo {
        action: crate::app::UndoAction::Todos(vec!["todo_1".into()]),
        verb_past: "Ticked off".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });
    app.pending_mutation_queue.clear();
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoState {
            action: TodoStateActionData::Undo,
            ..
        }]
    ));
}

#[test]
fn x_marks_it_not_a_to_do() {
    let mut app = todo_app(true);
    press_shifted(&mut app, 'X');
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoState {
            action: TodoStateActionData::Dismiss,
            ..
        }]
    ));
}

#[test]
fn the_runway_hint_shows_at_the_first_bar_after_a_key_and_esc_dismisses_it_once() {
    let mut app = todo_app(false);
    assert!(app.active_hint().is_none(), "not on arrival");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        app.active_hint().map(|h| h.id.as_str()),
        Some("todo.runway")
    );
    assert!(app
        .status_bar_state()
        .status_message
        .is_some_and(|line| line.contains("The bar fills from when this showed up")));
    press(&mut app, KeyCode::Esc);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetHintSeen { hint, seen: true }] if hint == "todo.runway"
    ));
    assert!(app.active_hint().is_none());
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        queued(&app).len(),
        1,
        "a dismissed hint isn't dismissed twice"
    );
}

#[test]
fn a_failed_dismissal_brings_the_hint_back() {
    let mut app = todo_app(false);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Esc);
    let failed = app.pending_mutation_queue[0].id;
    app.handle_mutation_failure_result(failed, true, &mxr_core::MxrError::Ipc("down".into()));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        app.active_hint().map(|h| h.id.as_str()),
        Some("todo.runway")
    );
}

#[test]
fn schedule_sends_the_previewed_time() {
    let mut app = todo_app(true);
    press_shifted(&mut app, 'Z');
    assert!(matches!(
        app.mailbox.todo_page.prompt.as_ref().map(|p| &p.kind),
        Some(TodoPromptKind::Schedule { todo_id }) if todo_id == "todo_1"
    ));
    for c in "in 3d".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let previewed = app
        .mailbox
        .todo_page
        .prompt
        .as_ref()
        .unwrap()
        .time
        .chosen()
        .unwrap();
    press(&mut app, KeyCode::Enter);
    assert!(app.mailbox.todo_page.prompt.is_none());
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::ScheduleTodo { when: Some(when), .. }] if *when == previewed.to_rfc3339()
    ));
}

#[test]
fn comma_edits_a_field_and_refuses_a_line_without_equals() {
    let mut app = todo_app(true);
    press(&mut app, KeyCode::Char(','));
    for c in "due fri".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Enter);
    assert!(app
        .mailbox
        .todo_page
        .prompt
        .as_ref()
        .unwrap()
        .error
        .is_some());
    assert!(queued(&app).is_empty());
    app.mailbox.todo_page.prompt.as_mut().unwrap().input = "due=fri".into();
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::UpdateTodo { edits, .. }] if edits[0].field == "due" && edits[0].value == "fri"
    ));
}

#[test]
fn t_on_a_conversation_makes_a_to_do_titled_from_the_sender() {
    let mut app = App::new();
    let envelopes = make_test_envelopes(2);
    let target = envelopes[0].clone();
    app.mailbox.envelopes = envelopes.clone();
    app.mailbox.all_envelopes = envelopes;
    app.mailbox.viewing_envelope = Some(target.clone());
    app.mailbox.active_pane = ActivePane::MessageView;
    press(&mut app, KeyCode::Char('t'));
    let prompt = app.mailbox.todo_page.prompt.as_ref().expect("a prompt");
    let sender = target
        .from
        .name
        .clone()
        .unwrap_or_else(|| target.from.email.clone());
    assert_eq!(prompt.input, format!("Reply to {sender}"));
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::CreateTodo { message_id, title, dry_run: false, .. }]
            if *message_id == target.id && title.starts_with("Reply to")
    ));
}

#[test]
fn the_catch_up_keeps_lets_go_and_lets_go_of_exactly_the_preview() {
    let mut app = todo_app(true);
    press_shifted(&mut app, 'C');
    assert_eq!(app.mailbox.todo_page.panel, TodoPanel::Catchup);
    assert_eq!(
        app.mailbox.todo_page.pending_list,
        Some(crate::app::TodoListFetch::Catchup)
    );
    let rows = vec![
        todo("todo_a", "Sign Tenancy renewal"),
        todo("todo_b", "Pay water bill"),
    ];
    app.set_todo_catchup(mxr_protocol::TodoCatchupData {
        title: "Catch up: 2 things".into(),
        why: String::new(),
        window_days: 14,
        todos: rows.clone(),
        overflow_count: 0,
        already_over: Vec::new(),
        already_over_line: None,
        first_run: mxr_protocol::TodoFirstRunData {
            complete: true,
            scanned: 1,
            reached: None,
        },
    });
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoCatchup { decision: TodoCatchupDecisionData::Keep { todo_ids }, dry_run: false, .. }]
            if todo_ids == &["todo_a".to_string()]
    ));
    app.pending_mutation_queue.clear();

    press_shifted(&mut app, 'A');
    assert!(
        app.mailbox.todo_page.pending_catchup_preview,
        "A asks for a dry run first"
    );
    assert!(queued(&app).is_empty());
    app.mailbox.todo_page.pending_catchup_preview = false;
    app.show_catchup_preview(TodoChangeData {
        dry_run: true,
        action: "let_go".into(),
        changed: vec![rows[1].clone()],
        unchanged: Vec::new(),
        summary: "Would let go of 1.".into(),
    });
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoCatchup { decision: TodoCatchupDecisionData::LetGo { todo_ids }, dry_run: false, .. }]
            if todo_ids == &["todo_b".to_string()]
    ));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mailbox.todo_page.panel, TodoPanel::Runway);
}

#[test]
fn the_expired_list_restores_with_enter() {
    let mut app = todo_app(true);
    press_shifted(&mut app, 'E');
    assert_eq!(app.mailbox.todo_page.panel, TodoPanel::Expired);
    app.set_todo_expired(vec![todo("todo_old", "Renew passport")]);
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoState { todo_ids, action: TodoStateActionData::Undo, .. }]
            if todo_ids == &["todo_old".to_string()]
    ));
}

#[test]
fn help_leads_with_the_mode_on_to_do() {
    let app = todo_app(true);
    let guide = app.help_mode_guide().expect("To do's guide");
    assert_eq!(guide.mode, "todo");
    assert!(App::new().help_mode_guide().is_none());
}

fn catchup_change(action: &str, ids: &[&str]) -> TodoChangeData {
    TodoChangeData {
        dry_run: false,
        action: action.into(),
        changed: ids.iter().map(|id| todo(id, "Pay water bill")).collect(),
        unchanged: Vec::new(),
        summary: String::new(),
    }
}

#[test]
fn undoing_a_catch_up_decision_puts_the_rows_back_undecided() {
    for action in ["keep", "let_go"] {
        let undo = crate::runner::todo_undo(&catchup_change(action, &["todo_a", "todo_b"]))
            .unwrap_or_else(|| panic!("{action} offers undo"));
        let mut app = todo_app(true);
        app.set_pending_undo(undo);
        app.pending_mutation_queue.clear();
        press(&mut app, KeyCode::Char('u'));
        assert!(
            matches!(
                queued(&app).as_slice(),
                [Request::SetTodoCatchup {
                    decision: TodoCatchupDecisionData::Undecide { todo_ids },
                    dry_run: false,
                    ..
                }] if todo_ids == &["todo_a".to_string(), "todo_b".to_string()]
            ),
            "{action}: {:?}",
            queued(&app)
        );
    }
}

#[test]
fn enter_opens_the_source_message_with_its_link_to_mark() {
    let mut app = todo_app(true);
    let bill = app.mailbox.todo_page.runway.as_ref().unwrap().now[0].clone();
    press(&mut app, KeyCode::Enter);
    let open = app
        .mailbox
        .todo_page
        .pending_open
        .clone()
        .expect("Enter asks for the source message");
    assert_eq!(Some(open.message_id), bill.source_message_id);
    assert_eq!(open.link.as_deref(), Some("https://www.camden.gov.uk/pay"));

    // Once the envelope lands, that message (not the newest) is the one shown,
    // and its link is the one marked.
    let thread = mxr_core::id::ThreadId::new();
    let mut source = crate::test_fixtures::TestEnvelopeBuilder::new()
        .subject("Your council tax bill")
        .thread_id(thread.clone())
        .date(chrono::Utc::now() - chrono::Duration::days(2))
        .flags(mxr_core::types::MessageFlags::READ)
        .build();
    source.id = bill.source_message_id.clone().unwrap();
    let newer = crate::test_fixtures::TestEnvelopeBuilder::new()
        .subject("Re: Your council tax bill")
        .thread_id(thread)
        .build();
    app.mailbox.all_envelopes = vec![source.clone(), newer];
    app.open_todo_envelope(source.clone(), open.link);
    assert_eq!(
        app.mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone()),
        Some(source.id.clone())
    );
    assert_eq!(
        app.mailbox.todo_link,
        Some((source.id, "https://www.camden.gov.uk/pay".to_string()))
    );
}

#[test]
fn ticking_off_the_last_row_puts_the_cursor_on_the_one_before() {
    let mut app = todo_app(true);
    app.mailbox.selected_index = 1;
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(app.mailbox.selected_index, 0);
    assert_eq!(
        app.selected_todo().map(|todo| todo.id.as_str()),
        Some("todo_1")
    );
}
