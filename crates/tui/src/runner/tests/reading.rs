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
        Some(
            mxr_protocol::READING_GUIDE
                .to_data(|_| Some(chrono::Utc::now()), &mxr_keymap::guide_key),
        ),
    );
    app
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
    assert!(app.mailbox.reading_page.pending_let_go_preview.is_none());
    key(&mut app, 'E');
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
fn unsubscribe_previews_with_the_evidence_before_it_commits() {
    let mut app = reading_app();
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
    app.show_reading_unsubscribe_preview(
        target.clone(),
        purge_preview(
            &target.sender_email,
            mxr_core::types::UnsubscribeMethod::Mailto {
                address: "leave@growth.example".into(),
                subject: None,
            },
            Some("tok-1"),
        ),
    );
    let Some(ReadingConfirm::Unsubscribe { target: shown, .. }) =
        app.mailbox.reading_page.confirm.clone()
    else {
        panic!("a preview is on screen");
    };
    assert_eq!(shown.method, ReadingUnsubscribeData::Mailto);
    press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
    let requests = queued(&app);
    assert!(
        matches!(
            requests.as_slice(),
            [Request::CommitUnsubscribePreview { address, archive: true, preview_token: token, .. }]
                if *address == target.sender_email && token == "tok-1"
        ),
        "{requests:?}"
    );
}

#[test]
fn a_failed_or_tokenless_preview_never_unsubscribes() {
    let mut app = reading_app();
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
    app.show_reading_unsubscribe_preview(
        target.clone(),
        purge_preview(
            &target.sender_email,
            mxr_core::types::UnsubscribeMethod::None,
            None,
        ),
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(queued(&app).is_empty());
}

fn purge_preview(
    address: &str,
    method: mxr_core::types::UnsubscribeMethod,
    token: Option<&str>,
) -> mxr_protocol::UnsubscribePurgeResultData {
    mxr_protocol::UnsubscribePurgeResultData {
        address: address.to_string(),
        query: format!("from:{address}"),
        account_id: None,
        dry_run: true,
        method,
        status: mxr_protocol::UnsubscribePurgeStatusData::Preview,
        message_count: 11,
        archived_count: 0,
        message_ids: Vec::new(),
        mutation_id: None,
        error: None,
        preview_token: token.map(str::to_string),
    }
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

fn highlight(id: &str, origin: &str, note: Option<&str>) -> mxr_protocol::ReadingHighlightData {
    let item = edition().bands[0].items[0].clone();
    mxr_protocol::ReadingHighlightData {
        id: id.into(),
        item_key: item.item_key,
        account_id: item.account_id,
        message_id: item.message_id,
        view: "issue".into(),
        quote: format!("quote {id}"),
        note: note.map(str::to_string),
        origin: origin.into(),
        paragraph: None,
        title: item.title,
        source: item.source,
        url: None,
        created_at: chrono::Utc::now(),
    }
}

fn highlights_app() -> App {
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
            highlights: vec![
                highlight("own", "mxr", Some("first")),
                highlight("theirs", "marginalia", None),
            ],
            marginalia: None,
            minutes_left: 1,
            pace_wpm: 230,
            item: detail_item,
        },
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.pending_mutation_queue.clear();
    key(&mut app, '3');
    assert!(app.mailbox.reading_page.highlights.is_some());
    app
}

#[test]
fn shift_h_removes_the_selected_highlight_only_after_asking() {
    let mut app = highlights_app();
    key(&mut app, 'x');
    assert!(queued(&app).is_empty(), "x asks first");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(queued(&app).is_empty());
    key(&mut app, 'x');
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::RemoveHighlight { highlight_id, dry_run: false }] if highlight_id == "own"
    ));
}
