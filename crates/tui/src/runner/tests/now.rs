//! Now and the rail in the TUI: `g h` opens the front page, each rail key
//! opens its mode, `e` is done here through `SetModeDone` with the
//! daemon's handoff copy and `u` to undo, `A` lets go of the Updates card
//! only after a preview, and a new sender's question is answered in place.

use super::*;
use crate::ui::now_lens::tests::{page, populated};
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, SenderKindData};

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

/// Now open on the demo's front page.
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
fn each_rail_key_opens_its_mode() {
    let mut app = App::new();
    for (key, view) in [
        ('h', MailboxView::Now),
        ('m', MailboxView::People),
        ('x', MailboxView::Todo),
        ('u', MailboxView::Updates),
        ('r', MailboxView::Reading),
        ('e', MailboxView::ArchiveMode),
        // Paper trail became Updates.
        ('p', MailboxView::Updates),
    ] {
        chord(&mut app, key);
        assert_eq!(app.mailbox.mailbox_view, view, "g {key}");
    }
    app.mailbox.labels = make_test_labels();
    chord(&mut app, 'i');
    assert_eq!(
        app.mailbox.mailbox_view,
        MailboxView::Messages,
        "g i: Inbox"
    );
    assert!(app.mailbox.pending_active_label.is_some());
}

#[test]
fn the_sidebar_is_the_rail_with_the_rest_under_more() {
    let mut app = App::new();
    app.mailbox.labels = make_test_labels();
    let items = app.sidebar_items();
    assert!(matches!(
        &items[..7],
        [
            SidebarItem::Now,
            SidebarItem::Messages,
            SidebarItem::Todo,
            SidebarItem::Updates,
            SidebarItem::Reading,
            SidebarItem::ArchiveMode,
            SidebarItem::Inbox,
        ]
    ));
    assert!(matches!(
        &items[7..10],
        [
            SidebarItem::Screener,
            SidebarItem::ReplyQueue,
            SidebarItem::Waiting
        ]
    ));
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, SidebarItem::Label(label) if label.name == "INBOX")),
        "Inbox is on the rail, not repeated under More"
    );
    app.mailbox.sidebar_selected = 5;
    app.mailbox.active_pane = ActivePane::Sidebar;
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::ArchiveMode);
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

    // The daemon's answer: its handoff copy is the toast, with undo.
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

#[test]
fn the_last_mode_letting_go_says_it_archived_in_gmail() {
    let thread = ThreadId::new();
    let (effect, _) = crate::runner::mode_done_outcome(
        &[outcome(
            thread,
            ModeKindData::Todo,
            "Done. Archived in Gmail.",
        )],
        None,
        false,
    );
    assert!(matches!(
        effect,
        Ok(MutationEffect::ModeDone(ref copy)) if copy == "Done. Archived in Gmail."
    ));
    let mut failed = outcome(ThreadId::new(), ModeKindData::Messages, "");
    failed.error = Some("not in Messages".into());
    let (effect, undo) = crate::runner::mode_done_outcome(&[failed], None, false);
    assert!(effect.is_err() && undo.is_none());
}

#[test]
fn e_on_a_due_row_is_done_in_to_do_for_its_thread() {
    let mut app = now_app(true);
    let todo = app.mailbox.now_page.now.as_ref().unwrap().due_soon.todos[0].clone();
    let thread = todo.todo.thread_id.clone().expect("a thread");
    app.mailbox.selected_index = 3;
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeDone { thread_ids, mode: ModeKindData::Todo, todo_ids, .. }]
            if thread_ids == &vec![thread.clone()] && todo_ids == &vec![todo.todo.id.clone()]
    ));
}

/// The daemon's dry run of letting go of a digest.
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
fn esc_keeps_the_digest() {
    let mut app = now_app(true);
    app.show_now_digest_preview(let_go_preview(&[ThreadId::new()]));
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.now_page.digest_preview.is_none());
    assert!(queued(&app).is_empty());
}

#[test]
fn a_new_senders_question_is_answered_on_its_row() {
    let mut app = now_app(true);
    // Iris, the third person, is new.
    app.mailbox.selected_index = 2;
    press(&mut app, KeyCode::Char('2'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetSenderKind { sender_email, kind: Some(SenderKindData::PaperTrail), .. }]
            if sender_email == "iris.chen@example.com"
    ));
    // On a row with no question a digit is still a tab.
    app.pending_mutation_queue.clear();
    app.mailbox.selected_index = 0;
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE)),
        Some(Action::OpenTab2)
    );
}

fn status_line(app: &App) -> String {
    app.status_bar_state().status_message.unwrap_or_default()
}

fn hint_requests(app: &App) -> Vec<String> {
    queued(app)
        .into_iter()
        .filter_map(|request| match request {
            Request::SetHintSeen { hint, seen: true } => Some(hint),
            _ => None,
        })
        .collect()
}

#[test]
fn first_launch_opens_now_with_no_tour_and_no_card() {
    let mut app = now_app(false);
    let rendered = render_to_string(100, 30, |frame| app.draw(frame));
    assert!(!rendered.contains("Start Here"), "{rendered}");
    assert!(
        !rendered.contains("Now shows at most ten things"),
        "{rendered}"
    );
    // Arriving is not a need: no hint until a key is pressed on Now.
    assert!(app.active_hint().is_none());
    assert!(!status_line(&app).starts_with("Hint:"));
}

#[test]
fn the_why_line_hint_shows_in_the_status_line_when_the_cursor_reaches_it() {
    let mut app = now_app(false);
    press(&mut app, KeyCode::Char('j'));
    assert!(app.active_hint().is_none(), "the second row has no hint");
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(
        app.active_hint().map(|h| h.id.as_str()),
        Some("now.from_mode")
    );
    assert_eq!(
        status_line(&app),
        "Hint: Each row comes from a mode; Enter opens it there. (Esc dismisses)"
    );
    press(&mut app, KeyCode::Esc);
    assert_eq!(hint_requests(&app), ["now.from_mode"]);
    assert!(app.active_hint().is_none());
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert!(app.active_hint().is_none(), "dismissed means never again");
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        hint_requests(&app).len(),
        1,
        "a dismissed hint isn't sent twice"
    );
}

#[test]
fn acting_on_the_row_dismisses_its_hint_and_seen_hints_never_show() {
    let mut app = now_app(false);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(hint_requests(&app), ["now.from_mode"]);

    let mut app = now_app(true);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    assert!(app.active_hint().is_none());
}

#[test]
fn the_first_done_here_says_what_it_clears_once() {
    let mut app = now_app(false);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        app.status_message.as_deref(),
        Some("Done here (e) only clears this mode; it stays in To do until done there.")
    );
    assert!(hint_requests(&app).contains(&"done_here".to_string()));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        hint_requests(&app)
            .iter()
            .filter(|id| *id == "done_here")
            .count(),
        1
    );
}

#[test]
fn enter_opens_each_row_in_its_own_mode() {
    let mut app = now_app(true);
    let message = app.mailbox.now_page.now.as_ref().unwrap().people.rows[0]
        .row
        .message_id
        .clone();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mailbox.pending_invite_open, Some(message));

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
fn e_in_paper_trail_is_done_in_updates() {
    let mut app = App::new();
    app.apply(Action::OpenPlace(mxr_protocol::MailPlaceData::PaperTrail));
    let thread = ThreadId::new();
    let fetched = crate::app::PlacePageState {
        place: Some(mxr_protocol::MailPlaceData::PaperTrail),
        bundles: vec![mxr_protocol::PlaceBundleData {
            account_id: AccountId::new(),
            sender_email: "notifications@github.com".into(),
            sender_name: None,
            kind: mxr_protocol::MailKindData {
                kind: SenderKindData::PaperTrail,
                rule: mxr_protocol::KindRuleData::AutomatedAddress,
                reason: "automated sender".into(),
                corrected: false,
            },
            message_count: 1,
            unread_count: 1,
            pinned_count: 0,
            newest_at: chrono::Utc::now(),
            newest_subject: "Build failed".into(),
            messages: vec![mxr_protocol::PlaceMessageData {
                message_id: MessageId::new(),
                thread_id: thread.clone(),
                subject: "Build failed".into(),
                snippet: String::new(),
                date: chrono::Utc::now(),
                unread: true,
                pinned: false,
                starred: false,
            }],
        }],
        total_bundles: 1,
        total_messages: 1,
        loaded: true,
    };
    app.mailbox.place_page = fetched;
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeDone { thread_ids, mode: ModeKindData::Updates, .. }]
            if thread_ids == &vec![thread.clone()]
    ));
    assert_eq!(app.mailbox.place_page.row_count(), 0);
}

#[test]
fn done_moves_the_cursor_to_the_next_row_and_at_the_end_to_the_previous() {
    let mut app = now_app(true);
    let why = |app: &App| -> Vec<String> {
        app.mailbox
            .now_page
            .rows()
            .iter()
            .map(|row| row.why().to_string())
            .collect()
    };
    let before = why(&app);
    app.mailbox.selected_index = 1;
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        why(&app)[app.mailbox.selected_index],
        before[2],
        "the next row"
    );

    // The last row done: only rows with a done of their own, so no card.
    if let Some(now) = app.mailbox.now_page.now.as_mut() {
        now.updates = None;
        now.reading = None;
    }
    let last = app.mailbox.now_page.row_count() - 1;
    let previous = why(&app)[last - 1].clone();
    app.mailbox.selected_index = last;
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        why(&app)[app.mailbox.selected_index],
        previous,
        "the previous row"
    );
}
