use super::*;
use crate::app::ArchiveStep;

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn plain_body(id: &MessageId, text: &str) -> MessageBody {
    MessageBody {
        message_id: id.clone(),
        text_plain: Some(text.into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: Default::default(),
    }
}

fn open_thread(lines: usize) -> App {
    let mut app = App::new();
    app.mailbox.html_view = false;
    app.mailbox.envelopes = make_test_envelopes(1);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let thread = make_test_envelopes(3);
    for (index, env) in thread.iter().enumerate() {
        let text = (0..lines)
            .map(|line| format!("Body {index} line {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        app.mailbox
            .body_cache
            .insert(env.id.clone(), plain_body(&env.id, &text));
    }
    app.mailbox.viewed_thread_messages = thread;
    app.mailbox.thread_selected_index = 0;
    app.mailbox.active_pane = ActivePane::MessageView;
    app
}

fn screen(app: &mut App) -> String {
    render_to_string(140, 40, |frame| app.draw(frame))
}

#[test]
fn o_folds_the_focused_message_to_its_header_line_and_opens_it_again() {
    let mut app = open_thread(2);
    let first = app.mailbox.viewed_thread_messages[0].id.clone();

    press(&mut app, KeyCode::Char('o'));

    assert!(app.mailbox.collapsed_messages.contains(&first));
    let folded = screen(&mut app);
    assert!(folded.contains("▸ User 0"), "{folded}");
    assert!(!folded.contains("Body 0 line 0"), "{folded}");
    assert!(folded.contains("Body 1 line 0"), "{folded}");

    press(&mut app, KeyCode::Char('o'));

    assert!(app.mailbox.collapsed_messages.is_empty());
    let open = screen(&mut app);
    assert!(open.contains("Body 0 line 0"), "{open}");
    assert!(!open.contains("▸ User 0"), "{open}");
}

fn reader_on_row(row: usize, inbox: bool) -> App {
    let mut app = App::new();
    if inbox {
        set_active_inbox(&mut app);
    }
    app.mailbox.envelopes = make_test_envelopes(3);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.selected_index = row;
    app.apply(Action::OpenSelected);
    app
}

fn viewing(app: &App) -> Option<MessageId> {
    app.mailbox
        .viewing_envelope
        .as_ref()
        .map(|env| env.id.clone())
}

fn complete(app: &mut App) -> MessageId {
    let queued = app.pending_mutation_queue.remove(0);
    let step = app.take_archive_step(queued.id);
    app.apply_mutation_completion(queued.effect, true);
    if let Some((step, reader_on_it)) = step {
        app.finish_archive_step(step, reader_on_it);
    }
    match queued.request {
        Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } => message_ids[0].clone(),
        other => panic!("expected an archive, got {other:?}"),
    }
}

#[test]
fn close_bracket_archives_with_the_lists_archive_then_opens_the_next() {
    let mut app = reader_on_row(0, true);
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();

    press(&mut app, KeyCode::Char(']'));

    assert_eq!(viewing(&app), Some(ids[0].clone()));
    assert!(matches!(
        app.mailbox.archive_step,
        Some(ArchiveStep { forward: true, ref neighbour, mutation: Some(_), .. })
            if neighbour.as_ref() == Some(&ids[1])
    ));

    assert_eq!(complete(&mut app), ids[0]);

    assert_eq!(viewing(&app), Some(ids[1].clone()));
    assert_eq!(app.mailbox.active_pane, ActivePane::MessageView);
    assert!(app.mailbox.archive_step.is_none());
}

#[test]
fn a_bulk_confirm_carries_the_step_to_the_archive_it_queues() {
    let mut app = App::new();
    set_active_inbox(&mut app);
    let thread = ThreadId::new();
    let mut envelopes = make_test_envelopes(3);
    for env in &mut envelopes[..2] {
        env.thread_id = thread.clone();
    }
    app.mailbox.envelopes = envelopes;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);
    let next = app.mailbox.envelopes[2].id.clone();

    press(&mut app, KeyCode::Char(']'));
    assert!(app.modals.pending_bulk_confirm.is_some());
    assert!(matches!(
        app.mailbox.archive_step,
        Some(ArchiveStep { mutation: None, .. })
    ));

    press(&mut app, KeyCode::Char('y'));
    assert!(matches!(
        app.mailbox.archive_step,
        Some(ArchiveStep {
            mutation: Some(_),
            ..
        })
    ));
    complete(&mut app);

    assert_eq!(viewing(&app), Some(next));
}
