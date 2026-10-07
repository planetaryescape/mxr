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
fn now_app(card_seen: bool) -> App {
    let mut app = App::new();
    chord(&mut app, 'h');
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Now);
    assert!(
        app.mailbox.now_page.pending_refresh && app.mailbox.pending_rail_refresh,
        "opening Now fetches it, its guide and the rail"
    );
    let loaded = page(populated(), card_seen);
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
        (
            'u',
            MailboxView::Place(mxr_protocol::MailPlaceData::PaperTrail),
        ),
        (
            'r',
            MailboxView::Place(mxr_protocol::MailPlaceData::Reading),
        ),
        ('e', MailboxView::ArchiveMode),
        (
            'p',
            MailboxView::Place(mxr_protocol::MailPlaceData::PaperTrail),
        ),
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
    let asked = app
        .mailbox
        .now_page
        .pending_digest_preview
        .take()
        .expect("asks the daemon for a dry run");
    assert_eq!(asked, card_threads);
    app.show_now_digest_preview(
        asked,
        card_threads
            .iter()
            .map(|thread| outcome(thread.clone(), ModeKindData::Updates, "Done."))
            .collect(),
    );
    assert!(app.mailbox.now_page.digest_preview.is_some());
    let rendered = render_to_string(100, 30, |frame| app.draw(frame));
    assert!(rendered.contains("Let go of this digest"), "{rendered}");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeDone { thread_ids, mode: ModeKindData::Updates, dry_run: false, .. }]
            if thread_ids == &card_threads
    ));
    assert!(app.mailbox.now_page.now.as_ref().unwrap().updates.is_none());
}

#[test]
fn esc_keeps_the_digest() {
    let mut app = now_app(true);
    let threads = vec![ThreadId::new()];
    app.show_now_digest_preview(
        threads.clone(),
        vec![outcome(threads[0].clone(), ModeKindData::Updates, "Done.")],
    );
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

#[test]
fn the_card_shows_once_and_retires_on_esc_or_the_main_verb() {
    let mut app = now_app(false);
    assert!(app.mailbox.now_page.card_visible());
    press(&mut app, KeyCode::Esc);
    assert!(!app.mailbox.now_page.card_visible());
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeGuideSeen { mode, seen: true }] if mode == "now"
    ));

    let mut app = now_app(false);
    press(&mut app, KeyCode::Char('e'));
    assert!(!app.mailbox.now_page.card_visible(), "e retires the card");
    assert!(queued(&app)
        .iter()
        .any(|request| matches!(request, Request::SetModeGuideSeen { mode, .. } if mode == "now")));
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
        MailboxView::Place(mxr_protocol::MailPlaceData::PaperTrail),
        "the Updates card opens Updates"
    );
}

#[test]
fn e_in_updates_is_done_in_updates() {
    let mut app = App::new();
    chord(&mut app, 'u');
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
