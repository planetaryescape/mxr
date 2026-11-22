use super::*;
use crate::app::TodoPanel;
use crate::ui::todo_lens::tests::{council_tax, page, runway, todo};
use mxr_protocol::{ResponseData, TodoCatchupDecisionData, TodoChangeData, TodoStateActionData};

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

fn attach_resolution(app: &mut App, status: &str) {
    let mut row = app.mailbox.todo_page.runway.as_ref().unwrap().now[0].clone();
    row.resolution = Some(mxr_protocol::TodoResolutionData {
        id: "resolution-1".into(),
        message_id: mxr_core::id::MessageId::new(),
        thread_id: mxr_core::id::ThreadId::new(),
        source_date: chrono::Utc::now(),
        quote: "Your council tax payment has been received.".into(),
        quote_location: "body".into(),
        source_method: "rule".into(),
        action: "complete".into(),
        proposed_due_at: None,
        proposed_due_words: None,
        status: status.into(),
    });
    app.mailbox.todo_page.runway.as_mut().unwrap().now[0] = row;
}

#[test]
fn microsoft_sync_preview_must_be_rendered_on_the_runway_before_confirmation() {
    let mut app = todo_app(true);
    app.microsoft_todo_answer(ResponseData::MicrosoftTodoStatus {
        status: mxr_protocol::MicrosoftTodoStatusData {
            installed: true,
            install_url: "https://example.test".into(),
            hint: "installed".into(),
        },
    });
    let id = app.selected_todo().unwrap().id.clone();
    app.apply(Action::TodoSyncMicrosoft);
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SyncMicrosoftTodos { dry_run: true, .. })
    ));
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(app.mailbox.todo_page.panel, TodoPanel::Expired);
    app.mailbox.todo_page.expired = vec![council_tax()];
    let fields = mxr_protocol::MicrosoftTodoFieldsData {
        title: "Pay changed title".into(),
        due: None,
        scheduled: None,
        completed: false,
    };
    let answer = ResponseData::MicrosoftTodoSync {
        sync: mxr_protocol::MicrosoftTodoSyncData {
            dry_run: true,
            results: vec![mxr_protocol::MicrosoftTodoSyncItemData {
                todo_id: id.clone(),
                state: "changes".into(),
                summary: "Review pending changes".into(),
                mixer: Some(fields.clone()),
                microsoft: Some(fields.clone()),
                merged: Some(fields),
            }],
        },
    };
    app.microsoft_todo_answer(answer.clone());
    app.apply(Action::TodoSyncMicrosoft);
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SyncMicrosoftTodos { dry_run: true, .. })
    ));
    app.mailbox.todo_page.panel = TodoPanel::Runway;
    app.microsoft_todo_answer(answer.clone());
    let render_review = |app: &App, height| {
        mxr_test_support::render_to_string(120, height, |frame| {
            crate::ui::todo_lens::draw(
                frame,
                ratatui::layout::Rect::new(0, 0, 120, height),
                &crate::ui::todo_lens::TodoView {
                    zone: crate::ui::received_label::DisplayZone::Fixed(
                        chrono::FixedOffset::east_opt(3600).unwrap(),
                    ),
                    now: chrono::Utc::now(),
                    page: &app.mailbox.todo_page,
                    selected_index: app.mailbox.selected_index,
                    active_pane: &crate::app::ActivePane::MailList,
                    account_marks: crate::ui::account_mark::AccountMarks::default(),
                    peek_body: None,
                },
                &crate::theme::Theme::default(),
            );
        })
    };
    render_review(&app, 7);
    app.apply(Action::TodoSyncMicrosoft);
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SyncMicrosoftTodos { dry_run: true, .. })
    ));
    app.microsoft_todo_answer(answer);
    let visible = render_review(&app, 16);
    assert!(visible.contains("Review pending changes"));
    assert!(visible.contains("After sync: Pay changed title"));
    press(&mut app, KeyCode::Enter);
    assert!(
        matches!(app.mailbox.todo_page.pending_microsoft.take(), Some(Request::SyncMicrosoftTodos { todo_id: Some(actual), dry_run: false, .. }) if actual == id)
    );
}

#[test]
fn resolution_requires_preview_and_explicit_accept_and_opens_exact_source() {
    let mut app = todo_app(true);
    let source_id = mxr_core::id::MessageId::new();
    let mut row = app.mailbox.todo_page.runway.as_ref().unwrap().now[0].clone();
    row.resolution = Some(mxr_protocol::TodoResolutionData {
        id: "resolution-1".into(),
        message_id: source_id.clone(),
        thread_id: mxr_core::id::ThreadId::new(),
        source_date: chrono::Utc::now(),
        quote: "Your council tax payment has been received.".into(),
        quote_location: "body".into(),
        source_method: "rule".into(),
        action: "complete".into(),
        proposed_due_at: None,
        proposed_due_words: None,
        status: "pending".into(),
    });
    app.mailbox.todo_page.runway.as_mut().unwrap().now[0] = row;
    press(&mut app, KeyCode::Char(','));
    assert!(app.mailbox.todo_page.pending_resolution.is_some());
    assert!(app.mailbox.todo_page.prompt.is_none());
    press(&mut app, KeyCode::Char('o'));
    assert_eq!(
        app.mailbox
            .todo_page
            .pending_open
            .as_ref()
            .unwrap()
            .message_id,
        source_id
    );
    app.show_resolution_preview(
        "todo_1".into(),
        "resolution-1".into(),
        TodoChangeData {
            correction_undo: None,
            dry_run: true,
            action: "resolution_preview".into(),
            changed: vec![app.selected_todo().unwrap().clone()],
            unchanged: vec![],
            summary: "Would complete the to-do. Nothing changed yet.".into(),
        },
    );
    app.mailbox.todo_page.pending_open = None;
    press(&mut app, KeyCode::Char('o'));
    assert_eq!(
        app.mailbox
            .todo_page
            .pending_open
            .as_ref()
            .unwrap()
            .message_id,
        source_id,
        "opening the cited source remains available during preview"
    );
    assert!(app.mailbox.todo_page.resolution_preview.is_some());
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::ResolveTodoSuggestion {
            action: mxr_protocol::TodoResolutionActionData::Accept,
            dry_run: false,
            ..
        }]
    ));
}

#[test]
fn resolution_preview_is_discarded_after_selection_moves() {
    let mut app = todo_app(true);
    attach_resolution(&mut app, "pending");
    let first_todo = app.selected_todo().unwrap().clone();
    press(&mut app, KeyCode::Char('j'));
    let selected_source = app
        .selected_todo()
        .unwrap()
        .source_message_id
        .clone()
        .unwrap();
    assert_eq!(app.selected_todo().unwrap().id, "todo_9");

    app.show_resolution_preview(
        "todo_1".into(),
        "resolution-1".into(),
        TodoChangeData {
            correction_undo: None,
            dry_run: true,
            action: "resolution_preview".into(),
            changed: vec![first_todo],
            unchanged: vec![],
            summary: "Would complete the to-do. Nothing changed yet.".into(),
        },
    );

    assert!(app.mailbox.todo_page.resolution_preview.is_none());
    press(&mut app, KeyCode::Enter);
    assert!(
        queued(&app).is_empty(),
        "Enter must not accept the unseen offer"
    );
    assert_eq!(
        app.mailbox.todo_page.pending_open.unwrap().message_id,
        selected_source,
        "Enter acts on the currently selected row"
    );
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
    assert_eq!(app.status_message.as_deref(), Some("Ticking off..."));
    app.finish_pending_mutation();
    app.apply_mutation_completion(MutationEffect::Todo("Ticked off 1 to-do".into()), true);
    assert_eq!(app.pending_mutation_count, 0);
    assert_eq!(app.status_message, None);
    assert_eq!(
        app.mailbox.todo_page.runway_rows()[0].title,
        "Send the form"
    );
    assert!(app
        .toasts
        .visible(std::time::Instant::now())
        .iter()
        .any(|toast| toast.text == "Ticked off 1 to-do"));

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
fn x_chooses_a_durable_correction_before_changing_the_row() {
    let mut app = todo_app(true);
    press_shifted(&mut app, 'X');
    assert!(queued(&app).is_empty());
    assert!(app.mailbox.todo_page.prompt.is_some());
    press(&mut app, KeyCode::Char('2'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::CorrectTodo {
            reason: mxr_protocol::TodoCorrectionData::NotMine,
            dry_run: false,
            ..
        }]
    ));
}

#[test]
fn not_now_schedules_a_typed_time_as_previewed() {
    let mut app = todo_app(true);
    press(&mut app, KeyCode::Char('b'));
    for _ in 1..crate::ui::snooze_modal::rows_count(&app.modals.snooze_panel) {
        press(&mut app, KeyCode::Char('j'));
    }
    press(&mut app, KeyCode::Enter);
    for c in "in 3d".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let previewed = app.modals.snooze_panel.custom_time.chosen().unwrap();
    press(&mut app, KeyCode::Enter);
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
        [Request::CorrectTodo {
            todo_id,
            reason: mxr_protocol::TodoCorrectionData::WrongDetails,
            edits,
            dry_run: false,
            ..
        }] if todo_id == "todo_1" && edits.len() == 1
            && edits[0].field == "due" && edits[0].value == "fri"
    ));
    assert_eq!(
        app.mailbox.todo_page.prompt.as_ref().unwrap().input,
        "due=fri"
    );
    let saved = app.pending_mutation_queue[0].clone();
    let effect = app.with_done_note(saved.id, saved.effect);
    app.apply_mutation_completion(effect, true);
    assert!(app.mailbox.todo_page.prompt.is_none());
    let change = TodoChangeData {
        correction_undo: Some("correction-1".into()),
        dry_run: false,
        action: "correct".into(),
        changed: vec![todo("todo_1", "Check the agreement")],
        unchanged: vec![],
        summary: "Correction saved".into(),
    };
    app.pending_mutation_queue.clear();
    app.set_pending_undo(
        crate::runner::todo_undo(&change).expect("durable correction offers undo"),
    );
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::UndoTodoCorrection { todo_id, correction_id, dry_run: false }]
            if todo_id == "todo_1" && correction_id == "correction-1"
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
    press(&mut app, KeyCode::Char('2'));
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

    press_shifted(&mut app, 'E');
    assert!(
        app.mailbox.todo_page.pending_catchup_preview,
        "E asks for a dry run first"
    );
    assert!(queued(&app).is_empty());
    app.mailbox.todo_page.pending_catchup_preview = false;
    app.show_catchup_preview(TodoChangeData {
        correction_undo: None,
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
