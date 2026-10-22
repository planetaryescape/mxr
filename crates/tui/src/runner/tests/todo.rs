//! To do in the TUI: `g x` opens the runway and its guide, the row's verbs
//! become the daemon requests they name, hints show in the status line at
//! their row, and `t` makes a to-do from a conversation.

use super::*;
use crate::app::{TodoPanel, TodoPromptKind};
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
    press_shifted(&mut app, 'S');
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SyncMicrosoftTodos { dry_run: true, .. })
    ));
    press_shifted(&mut app, 'E');
    assert_eq!(app.mailbox.todo_page.panel, TodoPanel::Expired);
    // The same task can appear on another panel while a request is in flight.
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
    press_shifted(&mut app, 'S');
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
                },
                &crate::theme::Theme::default(),
            );
        })
    };
    render_review(&app, 7);
    press_shifted(&mut app, 'S');
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SyncMicrosoftTodos { dry_run: true, .. })
    ));
    app.microsoft_todo_answer(answer);
    let visible = render_review(&app, 16);
    assert!(visible.contains("Review pending changes"));
    assert!(visible.contains("After sync: Pay changed title"));
    press_shifted(&mut app, 'S');
    assert!(
        matches!(app.mailbox.todo_page.pending_microsoft.take(), Some(Request::SyncMicrosoftTodos { todo_id: Some(actual), dry_run: false, .. }) if actual == id)
    );
}

#[test]
fn microsoft_link_is_installation_gated_and_requires_a_second_explicit_key() {
    let mut app = todo_app(true);
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::GetMicrosoftTodoStatus)
    ));
    app.microsoft_todo_answer(ResponseData::MicrosoftTodoStatus {
        status: mxr_protocol::MicrosoftTodoStatusData {
            installed: false,
            install_url: "https://example.test/install".into(),
            hint: "Install ms-todo".into(),
        },
    });
    press_shifted(&mut app, 'M');
    assert!(app.mailbox.todo_page.pending_microsoft.is_none());
    app.mailbox
        .todo_page
        .microsoft_status
        .as_mut()
        .unwrap()
        .installed = true;
    press_shifted(&mut app, 'M');
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SendTodoToMicrosoft {
            dry_run: true,
            fingerprint: None,
            ..
        })
    ));
    // No second request may escape while the preview is still loading.
    press_shifted(&mut app, 'M');
    assert!(app.mailbox.todo_page.pending_microsoft.is_none());
    let id = app.selected_todo().unwrap().id.clone();
    app.microsoft_todo_answer(ResponseData::MicrosoftTodoSend {
        result: mxr_protocol::MicrosoftTodoSendData {
            dry_run: true,
            todo_id: id.clone(),
            title: "Pay council tax".into(),
            due: None,
            scheduled: None,
            notes: "Exact context".into(),
            fingerprint: "exact-preview".into(),
            state: "preview".into(),
            task_id: None,
            summary: "Shared fields sync both ways".into(),
        },
    });
    // A second key before the review has actually been rendered cannot commit.
    press_shifted(&mut app, 'M');
    assert!(app.mailbox.todo_page.pending_microsoft.is_none());
    mxr_test_support::render_to_string(20, 5, |frame| {
        crate::ui::todo_lens::draw(
            frame,
            ratatui::layout::Rect::new(0, 0, 20, 5),
            &crate::ui::todo_lens::TodoView {
                zone: crate::ui::received_label::DisplayZone::Fixed(
                    chrono::FixedOffset::east_opt(3600).unwrap(),
                ),
                now: chrono::Utc::now(),
                page: &app.mailbox.todo_page,
                selected_index: app.mailbox.selected_index,
                active_pane: &crate::app::ActivePane::MailList,
            },
            &crate::theme::Theme::default(),
        );
    });
    press_shifted(&mut app, 'M');
    assert!(app.mailbox.todo_page.pending_microsoft.is_none());
    mxr_test_support::render_to_string(80, 12, |frame| {
        crate::ui::todo_lens::draw(
            frame,
            ratatui::layout::Rect::new(0, 0, 80, 12),
            &crate::ui::todo_lens::TodoView {
                zone: crate::ui::received_label::DisplayZone::Fixed(
                    chrono::FixedOffset::east_opt(3600).unwrap(),
                ),
                now: chrono::Utc::now(),
                page: &app.mailbox.todo_page,
                selected_index: app.mailbox.selected_index,
                active_pane: &crate::app::ActivePane::MailList,
            },
            &crate::theme::Theme::default(),
        );
    });
    press_shifted(&mut app, 'M');
    assert!(
        matches!(app.mailbox.todo_page.pending_microsoft.take(), Some(Request::SendTodoToMicrosoft { todo_id, dry_run: false, fingerprint: Some(fingerprint), .. }) if todo_id == id && fingerprint == "exact-preview")
    );
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
fn microsoft_review_scrolls_without_moving_the_task_and_escape_discards_confirmation() {
    let mut app = todo_app(true);
    app.mailbox.todo_page.pending_microsoft = None;
    app.mailbox.todo_page.microsoft_busy = false;
    app.mailbox.todo_page.microsoft_status = Some(mxr_protocol::MicrosoftTodoStatusData {
        installed: true,
        install_url: "https://example.test".into(),
        hint: "installed".into(),
    });
    let id = app.selected_todo().unwrap().id.clone();
    app.microsoft_todo_answer(ResponseData::MicrosoftTodoSend {
        result: mxr_protocol::MicrosoftTodoSendData {
            todo_id: id.clone(),
            dry_run: true,
            fingerprint: "discarded-fingerprint".into(),
            title: "Pay selected".into(),
            due: None,
            scheduled: None,
            notes: "Long context\n".repeat(50),
            state: "preview".into(),
            task_id: None,
            summary: "Review".into(),
        },
    });
    mxr_test_support::render_to_string(80, 10, |frame| {
        crate::ui::todo_lens::draw(
            frame,
            ratatui::layout::Rect::new(0, 0, 80, 10),
            &crate::ui::todo_lens::TodoView {
                zone: crate::ui::received_label::DisplayZone::Fixed(
                    chrono::FixedOffset::east_opt(3600).unwrap(),
                ),
                now: chrono::Utc::now(),
                page: &app.mailbox.todo_page,
                selected_index: 0,
                active_pane: &crate::app::ActivePane::MailList,
            },
            &crate::theme::Theme::default(),
        );
    });
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.selected_todo().unwrap().id, id);
    assert_eq!(app.mailbox.todo_page.microsoft_scroll, 1);
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.todo_page.microsoft_preview.is_none());
    assert!(app.mailbox.todo_page.pending_microsoft.is_none());
    press_shifted(&mut app, 'M');
    assert!(matches!(
        app.mailbox.todo_page.pending_microsoft.take(),
        Some(Request::SendTodoToMicrosoft {
            dry_run: true,
            fingerprint: None,
            ..
        })
    ));
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
    press(&mut app, KeyCode::Char('v'));
    assert!(app.mailbox.todo_page.pending_resolution.is_some());
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
fn o_uses_the_task_source_when_its_resolution_is_not_visible() {
    for status in ["dismissed", "undone"] {
        let mut app = todo_app(true);
        let task_source = app
            .selected_todo()
            .unwrap()
            .source_message_id
            .clone()
            .unwrap();
        let hidden_offer_source = mxr_core::id::MessageId::new();
        let mut row = app.mailbox.todo_page.runway.as_ref().unwrap().now[0].clone();
        row.resolution = Some(mxr_protocol::TodoResolutionData {
            id: "resolution-1".into(),
            message_id: hidden_offer_source,
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

        press(&mut app, KeyCode::Char('o'));

        assert_eq!(
            app.mailbox.todo_page.pending_open.unwrap().message_id,
            task_source,
            "{status} evidence is hidden, so o must open the selected task's own source"
        );
    }
}

#[test]
fn o_uses_evidence_source_only_while_offer_evidence_is_visible() {
    for status in ["pending", "accepted"] {
        let mut app = todo_app(true);
        let task_source = app
            .selected_todo()
            .unwrap()
            .source_message_id
            .clone()
            .unwrap();
        let offer_source = mxr_core::id::MessageId::new();
        let mut row = app.mailbox.todo_page.runway.as_ref().unwrap().now[0].clone();
        row.resolution = Some(mxr_protocol::TodoResolutionData {
            id: "resolution-1".into(),
            message_id: offer_source.clone(),
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

        press(&mut app, KeyCode::Char('o'));

        assert_ne!(task_source, offer_source);
        assert_eq!(
            app.mailbox.todo_page.pending_open.unwrap().message_id,
            offer_source
        );
    }
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
fn u_undoes_the_recent_mutation_before_an_accepted_resolution() {
    let mut app = todo_app(true);
    attach_resolution(&mut app, "accepted");
    app.set_pending_undo(crate::app::PendingUndo {
        action: crate::app::UndoAction::Todos(vec!["todo_9".into()]),
        verb_past: "Ticked off".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });

    press(&mut app, KeyCode::Char('u'));

    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetTodoState {
            todo_ids,
            action: TodoStateActionData::Undo,
            ..
        }] if todo_ids == &["todo_9".to_string()]
    ));
}

#[test]
fn u_undoes_an_accepted_resolution_when_no_recent_mutation_is_pending() {
    let mut app = todo_app(true);
    attach_resolution(&mut app, "accepted");

    press(&mut app, KeyCode::Char('u'));

    assert!(matches!(
        queued(&app).as_slice(),
        [Request::ResolveTodoSuggestion {
            todo_id,
            suggestion_id,
            action: mxr_protocol::TodoResolutionActionData::Undo,
            dry_run: false,
        }] if todo_id == "todo_1" && suggestion_id == "resolution-1"
    ));
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
fn mutation_completion_clears_only_its_final_progress_status() {
    let mut app = todo_app(true);
    app.pending_mutation_count = 2;
    app.pending_mutation_status = Some("Ticking off...".into());
    app.status_message = Some("Ticking off...".into());

    app.finish_pending_mutation();
    assert_eq!(app.pending_mutation_count, 1);
    assert_eq!(
        app.pending_mutation_status.as_deref(),
        Some("Ticking off...")
    );
    assert_eq!(app.status_message.as_deref(), Some("Ticking off..."));

    app.status_message = Some("Search results updated".into());
    app.finish_pending_mutation();
    assert_eq!(app.pending_mutation_count, 0);
    assert_eq!(app.pending_mutation_status, None);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Search results updated")
    );
}

#[test]
fn mutation_failure_replaces_retired_progress_with_the_error() {
    let mut app = todo_app(true);
    press(&mut app, KeyCode::Char('e'));
    let queued = app.pending_mutation_queue[0].clone();

    app.finish_pending_mutation();
    app.handle_mutation_failure_result(
        queued.id,
        queued.best_effort,
        &mxr_core::MxrError::Ipc("daemon rejected the to-do update".into()),
    );

    let error = app.modals.error.as_ref().unwrap();
    assert!(error.detail.contains("daemon rejected the to-do update"));
    assert_ne!(app.status_message.as_deref(), Some("Ticking off..."));
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
fn task_undo_toast_uses_grammatical_singular_and_plural_copy() {
    let now = std::time::Instant::now();
    for (ids, expected) in [
        (vec!["todo_a"], "Ticked off 1 to-do"),
        (vec!["todo_a", "todo_b"], "Ticked off 2 to-dos"),
    ] {
        let undo = crate::runner::todo_undo(&catchup_change("done", &ids)).unwrap();
        let mut app = todo_app(true);
        app.set_pending_undo(undo);
        assert_eq!(app.pending_undo_toast(now).unwrap().text, expected);
    }
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
        .is_some_and(|line| line.contains("Press e to tick this off")));
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
fn source_reader_shows_email_and_escape_returns_to_the_resolution_preview() {
    let mut app = todo_app(true);
    let source_id = app
        .selected_todo()
        .unwrap()
        .source_message_id
        .clone()
        .unwrap();
    let mut row = app.selected_todo().unwrap().clone();
    row.resolution = Some(mxr_protocol::TodoResolutionData {
        id: "resolution-reader".into(),
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
    app.mailbox.todo_page.runway.as_mut().unwrap().now[0] = row.clone();
    let preview_summary = "Would complete the to-do. Nothing changed yet.";
    app.show_resolution_preview(
        row.id.clone(),
        "resolution-reader".into(),
        TodoChangeData {
            dry_run: true,
            action: "resolution_preview".into(),
            changed: vec![row],
            unchanged: vec![],
            summary: preview_summary.into(),
        },
    );
    let source = crate::test_fixtures::TestEnvelopeBuilder::new()
        .subject("Your council tax bill")
        .thread_id(mxr_core::id::ThreadId::new())
        .build();
    let mut source = source;
    source.id = source_id.clone();
    app.mailbox.body_cache.insert(
        source_id.clone(),
        mxr_core::types::MessageBody {
            message_id: source_id,
            text_plain: Some("The payment is due on 31 October.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        },
    );
    app.mailbox.all_envelopes.push(source.clone());
    let selected_id = app.selected_todo().unwrap().id.clone();

    press(&mut app, KeyCode::Char('o'));
    let open = app
        .mailbox
        .todo_page
        .pending_open
        .take()
        .expect("o opens the exact evidence message");
    assert_eq!(open.message_id, source.id);
    app.open_todo_envelope(source, open.link);
    let reader = render_to_string(120, 32, |frame| app.draw(frame));
    assert!(
        reader.contains("The payment is due on 31 October."),
        "{reader}"
    );

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Todo);
    assert_eq!(app.mailbox.layout_mode, LayoutMode::TwoPane);
    assert_eq!(app.mailbox.active_pane, ActivePane::MailList);
    assert_eq!(
        app.selected_todo().map(|todo| todo.id.clone()),
        Some(selected_id)
    );
    assert_eq!(
        app.mailbox
            .todo_page
            .resolution_preview
            .as_ref()
            .map(|(todo_id, resolution_id, _)| (todo_id.as_str(), resolution_id.as_str())),
        Some(("todo_1", "resolution-reader"))
    );
    let todo_view = render_to_string(120, 32, |frame| app.draw(frame));
    assert!(todo_view.contains(preview_summary), "{todo_view}");
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

#[test]
fn the_hint_bar_offers_to_dos_verbs_for_a_lit_task() {
    let app = todo_app(true);
    let hints = app.mode_strip_hints().expect("a lit to-do has a strip");
    let hints: Vec<(&str, &str)> = hints
        .iter()
        .map(|(k, l)| (k.as_str(), l.as_str()))
        .collect();
    assert_eq!(
        hints,
        vec![
            ("e", "Done"),
            ("o", "Email"),
            ("Z", "Schedule"),
            ("X", "Not a to-do"),
            (",", "Edit"),
        ]
    );
}

#[test]
fn the_hint_bar_names_the_key_the_person_moved_a_verb_to() {
    let mut app = todo_app(true);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("keys.toml"),
        "[todo]\n\"x\" = \"todo.done\"\n",
    )
    .unwrap();
    app.load_keybindings_from(dir.path());
    let hints = app.mode_strip_hints().expect("a lit to-do has a strip");
    assert_eq!(hints[0], ("x".to_string(), "Done".to_string()));
}
