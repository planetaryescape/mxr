use super::*;
use crate::ui::now_lens::tests::{page, populated};
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData};

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn chord(app: &mut App, second: char) {
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(app, KeyCode::Char(second));
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

fn now_app(hints_seen: bool) -> App {
    let mut app = App::new();
    chord(&mut app, 'h');
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Now);
    assert!(
        app.mailbox.now_page.pending_refresh && app.mailbox.pending_rail_refresh,
        "opening Now fetches it, its guide and the rail"
    );
    let loaded = page(populated(), hints_seen);
    app.set_now(loaded.now.unwrap(), loaded.guide);
    app
}

fn outcome(thread_id: ThreadId, mode: ModeKindData, copy: &str) -> ModeDoneOutcomeData {
    ModeDoneOutcomeData {
        thread_id,
        account_id: None,
        mode,
        marked: true,
        todos_ticked: Vec::new(),
        still_in: Vec::new(),
        archived: 0,
        marked_read: 0,
        provider: "Gmail".into(),
        copy: copy.into(),
        error: None,
    }
}

#[test]
fn e_on_a_person_is_done_in_messages_and_u_undoes_it() {
    let mut app = now_app(true);
    let thread = app.mailbox.now_page.now.as_ref().unwrap().people.rows[0]
        .row
        .thread_id
        .clone();
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeDone { thread_ids, mode: ModeKindData::Messages, dry_run: false, .. }]
            if thread_ids == &vec![thread.clone()]
    ));
    let people = &app.mailbox.now_page.now.as_ref().unwrap().people.rows;
    assert!(
        people.iter().all(|person| person.row.thread_id != thread),
        "the row leaves at once"
    );

    let (effect, undo) = crate::runner::mode_done_outcome(
        &[outcome(
            thread.clone(),
            ModeKindData::Messages,
            "Done in Messages. Still in To do (due Mon).",
        )],
        Some("mutation-1".into()),
        false,
    );
    assert!(matches!(
        effect,
        Ok(MutationEffect::ModeDone(ref copy))
            if copy == "Done in Messages. Still in To do (due Mon). u to undo"
    ));
    app.set_pending_undo(undo.expect("an undo"));
    app.pending_mutation_queue.clear();
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::UndoMutation { mutation_id }] if mutation_id == "mutation-1"
    ));
}

pub(super) fn let_go_preview(threads: &[ThreadId]) -> mxr_protocol::UpdatesLetGoData {
    mxr_protocol::UpdatesLetGoData {
        dry_run: true,
        cut_at: chrono::Utc::now(),
        line: format!("Let go of {} updates from 2 sources.", threads.len()),
        message_count: u32::try_from(threads.len()).unwrap(),
        source_count: 2,
        hidden_count: 0,
        in_todo_count: 0,
        thread_ids: threads.to_vec(),
        message_ids: threads.iter().map(|_| MessageId::new()).collect(),
        selection_token: "token-1".into(),
        items: threads
            .iter()
            .map(|thread| outcome(thread.clone(), ModeKindData::Updates, "Done."))
            .collect(),
        mutation_id: None,
        undo_unavailable: false,
    }
}

#[test]
fn a_letting_go_of_the_digest_is_previewed_and_commits_what_was_previewed() {
    let mut app = now_app(true);
    let card_threads = app
        .mailbox
        .now_page
        .now
        .as_ref()
        .unwrap()
        .updates
        .as_ref()
        .unwrap()
        .thread_ids
        .clone();
    press(&mut app, KeyCode::Char('A'));
    assert!(!app.mailbox.now_page.pending_digest_preview);
    press(&mut app, KeyCode::Char('E'));
    assert!(
        queued(&app).is_empty(),
        "nothing changes before the preview"
    );
    assert!(
        std::mem::take(&mut app.mailbox.now_page.pending_digest_preview),
        "asks the daemon for a dry run"
    );
    let preview = let_go_preview(&card_threads);
    app.show_now_digest_preview(preview.clone());
    let rendered = render_to_string(100, 30, |frame| app.draw(frame));
    assert!(rendered.contains("Let go of this digest"), "{rendered}");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::LetGoDigest { selection_token: Some(token), cut: Some(cut), dry_run: false, .. }]
            if token == "token-1" && *cut == preview.cut_at
    ));
    assert!(app.mailbox.now_page.now.as_ref().unwrap().updates.is_none());
}

#[test]
fn enter_opens_each_row_in_its_own_mode() {
    let mut app = now_app(true);
    let thread = app.mailbox.now_page.now.as_ref().unwrap().people.rows[0]
        .row
        .thread_id
        .clone();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::People);
    assert_eq!(app.mailbox.messages_page.select_thread, Some(thread));

    let mut app = now_app(true);
    app.mailbox.selected_index = 3;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Todo);

    let mut app = now_app(true);
    app.mailbox.selected_index = 5;
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.mailbox.mailbox_view,
        MailboxView::Updates,
        "the Updates card opens Updates"
    );
}

#[test]
fn r_a_and_f_answer_or_forward_the_lit_person_and_nothing_else() {
    let person = |app: &App| {
        app.mailbox.now_page.now.as_ref().unwrap().people.rows[0]
            .row
            .message_id
            .clone()
    };
    for (key, expected) in [('r', "reply"), ('a', "reply all"), ('f', "forward")] {
        let mut app = now_app(true);
        let message = person(&app);
        press(&mut app, KeyCode::Char(key));
        let composed = match app.compose.pending_compose.as_ref() {
            Some(crate::app::ComposeAction::Reply { message_id, .. }) => {
                Some(("reply", message_id.clone()))
            }
            Some(crate::app::ComposeAction::ReplyAll { message_id, .. }) => {
                Some(("reply all", message_id.clone()))
            }
            Some(crate::app::ComposeAction::Forward { message_id, .. }) => {
                Some(("forward", message_id.clone()))
            }
            _ => None,
        };
        assert_eq!(composed, Some((expected, message)), "{key}");
    }

    for key in ['a', 'f'] {
        let mut app = now_app(true);
        app.mailbox.selected_index = 3;
        assert!(matches!(
            app.selected_now_row(),
            Some(crate::app::NowRow::Todo(_))
        ));
        press(&mut app, KeyCode::Char(key));
        assert!(app.compose.pending_compose.is_none(), "{key}");
        assert!(queued(&app).is_empty(), "{key}");
    }
}
