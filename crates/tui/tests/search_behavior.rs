//! Behavior tests for the type-ahead search debounce.
//!
//! Each keystroke should defer the actual search by a short window so a
//! burst of typing produces ONE query, not one per keystroke. When the
//! window elapses, `tick()` flushes the pending debounce into a real
//! search request.

use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageFlags, UnsubscribeMethod};
use mxr_core::{SearchMode, SortOrder};
use mxr_tui::app::{App, PendingSearchDebounce, PendingSearchRequest};

fn schedule_debounce(app: &mut App, query: &str, due_at: Instant) {
    let session_id = app.search.page.session_id;
    app.search.pending_debounce = Some(PendingSearchDebounce {
        query: query.to_string(),
        mode: SearchMode::Lexical,
        session_id,
        due_at,
    });
    // The debouncer also ticks the page query so the rendered search
    // bar mirrors the in-flight intent.
    app.search.page.query = query.to_string();
}

#[test]
fn pending_debounce_does_not_fire_before_due_time() {
    let mut app = App::new();
    let future = Instant::now() + Duration::from_secs(60);
    schedule_debounce(&mut app, "from:alice", future);

    assert!(
        app.search.pending_debounce.is_some(),
        "precondition: a debounce was scheduled"
    );
    assert!(
        app.search.pending.is_none(),
        "precondition: no real search yet"
    );

    app.tick();

    assert!(
        app.search.pending_debounce.is_some(),
        "debounce remains pending while due_at is in the future"
    );
    assert!(
        app.search.pending.is_none(),
        "no search request issued before the window elapses"
    );
}

#[test]
fn expired_debounce_fires_pending_search_on_tick() {
    let mut app = App::new();
    let past = Instant::now() - Duration::from_millis(10);
    schedule_debounce(&mut app, "from:alice", past);

    app.tick();

    assert!(
        app.search.pending_debounce.is_none(),
        "expired debounce drained from the queue"
    );
    let request: &PendingSearchRequest = app
        .search
        .pending
        .as_ref()
        .expect("expired debounce flushes a real search request");
    assert_eq!(request.query, "from:alice");
    assert_eq!(request.mode, SearchMode::Lexical);
    assert_eq!(request.sort, SortOrder::DateDesc);
}

#[test]
fn new_debounce_replaces_an_unfired_one() {
    let mut app = App::new();
    let future = Instant::now() + Duration::from_secs(60);

    schedule_debounce(&mut app, "fr", future);
    let first_query = app
        .search
        .pending_debounce
        .as_ref()
        .map(|d| d.query.clone())
        .expect("first debounce scheduled");
    assert_eq!(first_query, "fr");

    schedule_debounce(&mut app, "from", future);
    let second_query = app
        .search
        .pending_debounce
        .as_ref()
        .map(|d| d.query.clone())
        .expect("second debounce replaced first");

    assert_eq!(
        second_query, "from",
        "later keystrokes overwrite the pending query"
    );
}

fn search_result_envelope(slug: &str) -> Envelope {
    Envelope {
        id: MessageId::new(),
        account_id: AccountId::new(),
        provider_id: slug.into(),
        thread_id: ThreadId::new(),
        message_id_header: Some(format!("<{slug}@example.com>")),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some(format!("Sender {slug}")),
            email: format!("{slug}@example.com"),
        },
        to: vec![Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: format!("subject {slug}"),
        date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
        flags: MessageFlags::READ,
        snippet: format!("snippet for {slug}"),
        has_attachments: false,
        size_bytes: 100,
        unsubscribe: UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: vec!["INBOX".into()],
        keywords: std::collections::BTreeSet::new(),
    }
}

/// Phase 1.4 regression: clearing the search input (or executing search
/// against an empty query) must wipe the workspace so the previous
/// query's results cannot linger as stale UI. The user's contract is
/// "empty input = no results" — anything else lies about what's being
/// shown.
#[test]
fn empty_query_clears_results() {
    let mut app = App::new();

    // Simulate a prior search session: one stored result, a non-trivial
    // status, and a leftover debounce timer.
    app.search.page.query.clear();
    app.search
        .page
        .results
        .push(search_result_envelope("hit-1"));
    app.search.page.session_active = true;
    app.search.page.total_count = Some(1);
    app.search.pending_debounce = Some(PendingSearchDebounce {
        query: "lingering".into(),
        mode: SearchMode::Lexical,
        session_id: app.search.page.session_id,
        due_at: Instant::now() + Duration::from_secs(60),
    });

    assert_eq!(
        app.search.page.results.len(),
        1,
        "precondition: there is a stale result in the workspace"
    );

    app.execute_search_page_search();

    assert!(
        app.search.page.results.is_empty(),
        "empty query must drop all rendered results"
    );
    assert_eq!(
        app.search.page.total_count, None,
        "empty query must drop the total-count summary"
    );
    assert!(
        !app.search.page.session_active,
        "empty query ends the search session"
    );
    assert!(
        app.search.pending_debounce.is_none(),
        "empty query must drain any pending debounce so it can't re-fire"
    );
    assert!(
        app.search.pending.is_none(),
        "empty query must not enqueue a real search request"
    );
}
