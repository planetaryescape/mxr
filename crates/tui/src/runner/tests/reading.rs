//! Reading in the TUI: `g r` opens the edition, Enter reads beside it and
//! records the open, `b` on a digest link puts it on Later and saves its
//! article, `A` and `D` preview before they change anything and commit
//! exactly what they previewed, `L` names the site before it fetches.

use super::*;
use crate::app::ReadingConfirm;
use crate::ui::reading_lens::tests::edition;
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, ReadingUnsubscribeData};

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn key(app: &mut App, c: char) {
    let modifiers = if c.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    press(app, KeyCode::Char(c), modifiers);
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

/// Reading open on a loaded edition with its card retired.
fn reading_app() -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    key(&mut app, 'r');
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Reading);
    let page = &app.mailbox.reading_page;
    assert!(
        page.pending_refresh && page.pending_mark_visit,
        "opening Reading fetches the edition as a visit"
    );
    app.set_reading_edition(
        edition(),
        Some(mxr_protocol::READING_GUIDE.to_data(Some(chrono::Utc::now()))),
    );
    app
}

#[test]
fn the_cursor_loads_the_item_beside_the_list_once() {
    let mut app = reading_app();
    let first = app.reading_item_to_load().expect("the first item loads");
    assert!(first.ends_with(":0"));
    assert_eq!(
        app.reading_item_to_load(),
        None,
        "asked once while in flight"
    );
    key(&mut app, 'j');
    let link = app.reading_item_to_load().expect("the next row loads");
    assert_ne!(link, first);
}

#[test]
fn enter_reads_and_records_the_open() {
    let mut app = reading_app();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.mailbox.reading_page.reader_focused);
    assert!(queued(&app).iter().any(|request| matches!(
        request,
        Request::RecordReadingEngagement { opened: true, item_key, .. } if item_key.ends_with(":0")
    )));
}

#[test]
fn b_on_a_digest_link_saves_it_to_later_and_fetches_its_article() {
    let mut app = reading_app();
    // The lead essay, then the digest, then its first link.
    key(&mut app, 'j');
    key(&mut app, 'j');
    let link_key = app.selected_reading_row().expect("a row").key().to_string();
    assert!(link_key.ends_with(":1"), "{link_key}");
    key(&mut app, 'b');
    assert!(queued(&app).iter().any(|request| matches!(
        request,
        Request::SetReadingLater { item_keys, later: true, dry_run: false } if *item_keys == [link_key.clone()]
    )));
    assert_eq!(
        app.mailbox.reading_page.pending_fetch,
        Some((link_key, false)),
        "the article is saved so Later reads offline"
    );
    assert!(app
        .status_message
        .as_deref()
        .is_some_and(|status| status.contains("links.demo.mxr.local")));
}

#[test]
fn l_names_the_site_before_fetching_and_an_essay_has_nothing_to_fetch() {
    let mut app = reading_app();
    key(&mut app, 'L');
    assert!(app.mailbox.reading_page.pending_fetch.is_none());
    assert!(app
        .status_message
        .as_deref()
        .is_some_and(|status| status.contains("no article")));
    // The teaser, after the digest's four shown links.
    for _ in 0..6 {
        key(&mut app, 'j');
    }
    key(&mut app, 'L');
    assert!(app.mailbox.reading_page.pending_fetch.is_some());
    assert_eq!(
        app.status_message.as_deref(),
        Some("Fetching from platform.demo.mxr.local\u{2026}")
    );
}

fn outcome(thread_id: ThreadId) -> ModeDoneOutcomeData {
    ModeDoneOutcomeData {
        thread_id,
        account_id: None,
        mode: ModeKindData::Reading,
        marked: true,
        todos_ticked: Vec::new(),
        still_in: Vec::new(),
        archived: 0,
        marked_read: 0,
        provider: "Gmail".into(),
        copy: "Done in Reading.".into(),
        error: None,
    }
}

#[test]
fn let_go_of_everything_previews_then_commits_the_same_threads() {
    let mut app = reading_app();
    key(&mut app, 'A');
    let asked = app
        .mailbox
        .reading_page
        .pending_let_go_preview
        .take()
        .expect("a dry run is asked for first");
    assert!(
        queued(&app).is_empty(),
        "nothing changes before the preview"
    );
    assert_eq!(asked.len(), 4);
    // The daemon previews three of them (one went meanwhile).
    let previewed: Vec<ThreadId> = asked[..3].to_vec();
    app.show_reading_let_go_preview(&asked, previewed.iter().cloned().map(outcome).collect());
    assert!(matches!(
        app.mailbox.reading_page.confirm,
        Some(ReadingConfirm::LetGoAll { .. })
    ));
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let requests = queued(&app);
    assert!(
        matches!(
            requests.as_slice(),
            [Request::SetModeDone { thread_ids, mode: ModeKindData::Reading, dry_run: false, .. }]
                if *thread_ids == previewed
        ),
        "{requests:?}"
    );
}

#[test]
fn esc_on_a_preview_changes_nothing() {
    let mut app = reading_app();
    app.mailbox.reading_page.confirm = Some(ReadingConfirm::LetGoAll {
        thread_ids: vec![ThreadId::new()],
        items: Vec::new(),
    });
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.mailbox.reading_page.confirm.is_none());
    assert!(queued(&app).is_empty());
}

#[test]
fn unsubscribe_previews_with_the_evidence_before_it_commits() {
    let mut app = reading_app();
    // The Growth Digest in the Fading band is the last row.
    for _ in 0..7 {
        key(&mut app, 'j');
    }
    key(&mut app, 'D');
    let target = app
        .mailbox
        .reading_page
        .pending_unsubscribe_preview
        .take()
        .expect("a dry run is asked for first");
    assert!(queued(&app).is_empty());
    assert_eq!(target.evidence, "You opened 0 of the last 11 issues");
    assert_eq!(target.method, ReadingUnsubscribeData::OneClick);
    app.show_reading_unsubscribe_preview(target.clone(), 11);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let requests = queued(&app);
    assert!(
        matches!(
            requests.as_slice(),
            [Request::UnsubscribePurge { address, dry_run: false, .. }]
                if *address == target.sender_email
        ),
        "{requests:?}"
    );
}

#[test]
fn e_lets_go_of_the_issue_at_once() {
    let mut app = reading_app();
    let thread = app
        .selected_reading_row()
        .expect("a row")
        .issue()
        .thread_id
        .clone();
    key(&mut app, 'e');
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeDone { thread_ids, mode: ModeKindData::Reading, .. }] if *thread_ids == [thread.clone()]
    ));
    assert!(!app.mailbox.reading_page.thread_ids().contains(&thread));
}

#[test]
fn h_in_the_reader_highlights_the_top_paragraph() {
    let mut app = reading_app();
    let mut detail_item = edition().bands[0].items[0].clone();
    detail_item.words = 50;
    app.set_reading_item(
        &detail_item.item_key.clone(),
        mxr_protocol::ReadingItemDetailData {
            source_data: edition().sources[0].clone(),
            paragraphs: vec![mxr_protocol::ReadingParagraphData {
                kind: "text".into(),
                text: "Every reader since 2002 shipped the same window.".into(),
            }],
            html: None,
            article: None,
            article_error: None,
            highlights: Vec::new(),
            minutes_left: 1,
            pace_wpm: 230,
            item: detail_item,
        },
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    key(&mut app, 'h');
    assert!(queued(&app).iter().any(|request| matches!(
        request,
        Request::SaveHighlight { quote, view: Some(view), .. }
            if quote.starts_with("Every reader") && view == "issue"
    )));
}

#[test]
fn shift_b_opens_the_later_shelf_and_esc_comes_back() {
    let mut app = reading_app();
    key(&mut app, 'B');
    assert!(app.mailbox.reading_page.later_shelf);
    assert_eq!(
        app.mailbox.reading_page.row_count(),
        0,
        "the fixture shelf is empty"
    );
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.mailbox.reading_page.later_shelf);
}

#[test]
fn shift_k_moves_the_items_sender() {
    let mut app = reading_app();
    key(&mut app, 'K');
    let menu = app
        .mailbox
        .sender_kind_menu
        .as_ref()
        .expect("the menu opens");
    assert_eq!(menu.display, "Long Reads Weekly");
}
