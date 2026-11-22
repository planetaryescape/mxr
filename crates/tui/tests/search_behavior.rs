use std::time::{Duration, Instant};

use mxr_core::SearchMode;
use mxr_tui::app::{App, PendingSearchDebounce, PendingSearchRequest};

fn schedule_debounce(app: &mut App, query: &str, due_at: Instant) {
    let session_id = app.search.page.session_id;
    app.search.pending_debounce = Some(PendingSearchDebounce {
        query: query.to_string(),
        mode: SearchMode::Lexical,
        session_id,
        due_at,
    });
    app.search.page.query = query.to_string();
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
    assert_eq!(request.sort, None);
}
