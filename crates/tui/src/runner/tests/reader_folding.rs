//! The reader's `o`, `;` and `Q` folds and its `]` and `[` archive-and-step,
//! as the web reader has them.

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

/// The reader open on a three-message thread whose messages read
/// "Body 0" to "Body 2" over `lines` lines each. The middle one is unread.
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

#[test]
fn semicolon_folds_all_but_the_newest_and_unread_then_opens_every_message() {
    let mut app = open_thread(2);
    let ids: Vec<MessageId> = app
        .mailbox
        .viewed_thread_messages
        .iter()
        .map(|env| env.id.clone())
        .collect();

    press(&mut app, KeyCode::Char(';'));

    // 0 is read and older; 1 is unread; 2 is the newest.
    assert_eq!(
        app.mailbox.collapsed_messages,
        std::collections::HashSet::from([ids[0].clone()])
    );

    press(&mut app, KeyCode::Char(';'));

    assert!(app.mailbox.collapsed_messages.is_empty());
    assert_eq!(app.status_message.as_deref(), Some("Every message open"));
}

#[test]
fn folding_puts_the_focused_message_at_the_top_of_the_reader_once() {
    let mut app = open_thread(30);
    app.mailbox.thread_selected_index = 1;

    press(&mut app, KeyCode::Char('o'));
    let rendered = screen(&mut app);

    let lines: Vec<&str> = rendered.lines().collect();
    let top = lines
        .iter()
        .position(|line| line.contains(" Thread "))
        .expect("the reader is drawn");
    assert!(
        lines[top + 1].contains("▸ User 1"),
        "the folded message heads the reader:\n{rendered}"
    );
    assert!(app.mailbox.message_scroll_offset > 0);
    assert!(!app.mailbox.scroll_to_focused_message);

    // The next frame keeps the person's scroll.
    app.mailbox.message_scroll_offset = 0;
    let again = screen(&mut app);
    assert!(again.contains("Body 0 line 0"), "{again}");
}

#[test]
fn q_shows_and_folds_quoted_text_in_the_reading_view() {
    let mut app = open_thread(1);
    let first = app.mailbox.viewed_thread_messages[0].id.clone();
    app.mailbox.body_cache.insert(
        first.clone(),
        plain_body(
            &first,
            "Thursday works.\n\nOn Mon, 3 Mar 2026, Ada wrote:\n> Are you around Thursday?\n> Or Friday?",
        ),
    );

    let folded = screen(&mut app);
    assert!(
        folded.contains("[previous message from Ada] (Q shows it)"),
        "{folded}"
    );
    assert!(!folded.contains("Are you around"), "{folded}");

    press(&mut app, KeyCode::Char('Q'));

    assert!(app.mailbox.quotes_expanded);
    let shown = screen(&mut app);
    assert!(shown.contains("Are you around Thursday?"), "{shown}");
    assert!(shown.contains("Or Friday?"), "{shown}");

    press(&mut app, KeyCode::Char('Q'));
    assert!(!screen(&mut app).contains("Are you around"));
}

#[test]
fn q_folds_the_quoted_conversation_in_the_html_view() {
    let mut app = open_thread(1);
    app.mailbox.html_view = true;
    let first = app.mailbox.viewed_thread_messages[0].id.clone();
    app.mailbox.body_cache.insert(
        first.clone(),
        MessageBody {
            text_plain: None,
            text_html: Some(
                r#"<div>Thursday works.</div><div class="gmail_quote"><div class="gmail_attr">On Mon, Ada wrote:</div><blockquote class="gmail_quote">Are you around Thursday?</blockquote></div>"#
                    .into(),
            ),
            ..plain_body(&first, "")
        },
    );

    let folded = screen(&mut app);
    assert!(folded.contains("Thursday works."), "{folded}");
    assert!(
        folded.contains("quoted text folded (Q shows it)"),
        "{folded}"
    );
    assert!(!folded.contains("Are you around"), "{folded}");

    press(&mut app, KeyCode::Char('Q'));

    let shown = screen(&mut app);
    assert!(shown.contains("Are you around Thursday?"), "{shown}");
    assert!(!shown.contains("quoted text folded"), "{shown}");
}

#[test]
fn q_in_plain_text_says_plain_text_shows_quotes_in_full() {
    let mut app = open_thread(1);
    app.mailbox.reader_mode = false;

    press(&mut app, KeyCode::Char('Q'));

    assert!(!app.mailbox.quotes_expanded);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Plain text shows quoted text in full; R folds it in the reading view")
    );
}

/// The reader open on row `row` of a three-conversation Inbox.
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

/// What the runner does when the daemon finishes the archive.
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

    // The same request and undo as `e`; the reader waits for the daemon.
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
fn open_bracket_archives_then_opens_the_previous() {
    let mut app = reader_on_row(1, true);
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();

    press(&mut app, KeyCode::Char('['));
    assert_eq!(complete(&mut app), ids[1]);

    // The Inbox lost the row, so the reader first lands on the one that
    // took its place; the step takes it back to the previous one.
    assert_eq!(viewing(&app), Some(ids[0].clone()));
    assert_eq!(app.mailbox.selected_index, 0);
}

#[test]
fn close_bracket_steps_on_where_the_archive_leaves_the_row_in_the_list() {
    // No active label: the archived row stays, as in All Mail.
    let mut app = reader_on_row(0, false);
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();

    press(&mut app, KeyCode::Char(']'));
    complete(&mut app);

    assert_eq!(app.mailbox.envelopes.len(), 3);
    assert_eq!(viewing(&app), Some(ids[1].clone()));
    assert_eq!(app.mailbox.selected_index, 1);
}

#[test]
fn close_bracket_on_the_last_conversation_says_so() {
    let mut app = reader_on_row(2, false);
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();

    press(&mut app, KeyCode::Char(']'));
    complete(&mut app);

    assert_eq!(viewing(&app), Some(ids[2].clone()));
    assert_eq!(
        app.status_message.as_deref(),
        Some("Archived; that was the last conversation")
    );
}

#[test]
fn archive_step_keeps_where_the_person_went_while_it_ran() {
    let mut app = reader_on_row(1, true);
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();

    press(&mut app, KeyCode::Char(']'));
    press(&mut app, KeyCode::Char('N'));
    assert_eq!(viewing(&app), Some(ids[0].clone()));
    complete(&mut app);

    assert_eq!(viewing(&app), Some(ids[0].clone()));
}

#[test]
fn a_failed_archive_steps_nowhere() {
    let mut app = reader_on_row(0, true);
    let ids: Vec<MessageId> = app.mailbox.envelopes.iter().map(|e| e.id.clone()).collect();

    press(&mut app, KeyCode::Char(']'));
    let queued = app.pending_mutation_queue.remove(0);
    // The runner drops the step when the daemon refuses the archive.
    assert!(app.take_archive_step(queued.id).is_some());

    assert_eq!(viewing(&app), Some(ids[0].clone()));
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

#[test]
fn cancelling_the_bulk_confirm_drops_the_step() {
    let mut app = App::new();
    let thread = ThreadId::new();
    let mut envelopes = make_test_envelopes(2);
    for env in &mut envelopes {
        env.thread_id = thread.clone();
    }
    app.mailbox.envelopes = envelopes;
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.apply(Action::OpenSelected);

    press(&mut app, KeyCode::Char('['));
    press(&mut app, KeyCode::Esc);

    assert!(app.modals.pending_bulk_confirm.is_none());
    assert!(app.mailbox.archive_step.is_none());
}
