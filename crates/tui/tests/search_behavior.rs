//! Behavior tests for the type-ahead search debounce.
//!
//! Each keystroke should defer the actual search by a short window so a
//! burst of typing produces ONE query, not one per keystroke. When the
//! window elapses, `tick()` flushes the pending debounce into a real
//! search request.

use std::time::{Duration, Instant};

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
