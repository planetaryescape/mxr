//! Behavior tests for fast saved-search navigation.
//!
//! Power users keep a small constellation of saved searches as their
//! workspace and jump between them by index. The keyboard surface uses
//! the `g` prefix (`g 1` ... `g 9`) to avoid conflicting with the
//! existing single-digit screen tabs (1=Mailbox, 2=Search, etc.).

use chrono::{TimeZone, Utc};
use mxr_core::id::SavedSearchId;
use mxr_core::types::{SavedSearch, SearchMode, SortOrder};
use mxr_tui::action::Action;
use mxr_tui::app::App;

fn fixture_saved_search(name: &str, query: &str, position: i32) -> SavedSearch {
    SavedSearch {
        id: SavedSearchId::new(),
        account_id: None,
        name: name.to_string(),
        query: query.to_string(),
        search_mode: SearchMode::Lexical,
        sort: SortOrder::DateDesc,
        icon: None,
        position,
        created_at: Utc.with_ymd_and_hms(2024, 5, 7, 9, 0, 0).unwrap(),
    }
}

#[test]
fn open_saved_search_by_index_1_targets_first_saved_search() {
    let mut app = App::new();
    app.mailbox.saved_searches = vec![
        fixture_saved_search("Unread", "is:unread", 0),
        fixture_saved_search("Has attachments", "has:attachment", 1),
    ];

    app.apply(Action::OpenSavedSearchByIndex(1));

    assert!(
        app.search.active,
        "selecting a saved-search tab activates the search workspace"
    );
    assert_eq!(
        app.search.bar.query, "is:unread",
        "search bar reflects the targeted saved search's query"
    );
    assert_eq!(
        app.search.bar.mode,
        SearchMode::Lexical,
        "search bar reflects the targeted saved search's mode"
    );
}

#[test]
fn open_saved_search_by_index_2_targets_second_saved_search() {
    let mut app = App::new();
    app.mailbox.saved_searches = vec![
        fixture_saved_search("Unread", "is:unread", 0),
        fixture_saved_search("Has attachments", "has:attachment", 1),
        fixture_saved_search("From Alice", "from:alice", 2),
    ];

    app.apply(Action::OpenSavedSearchByIndex(2));

    assert_eq!(
        app.search.bar.query, "has:attachment",
        "index 2 targets the second saved search"
    );
}

#[test]
fn open_saved_search_by_index_zero_clears_active_filter() {
    let mut app = App::new();
    app.mailbox.saved_searches = vec![fixture_saved_search("Unread", "is:unread", 0)];

    // Activate a saved-search tab first.
    app.apply(Action::OpenSavedSearchByIndex(1));
    assert!(
        app.search.active,
        "precondition: a tab is active before clearing"
    );

    // Index 0 means "default inbox" — clear any saved-search filter.
    app.apply(Action::OpenSavedSearchByIndex(0));

    assert!(
        !app.search.active,
        "index 0 returns to the default inbox view"
    );
}

#[test]
fn open_saved_search_by_out_of_range_index_is_noop() {
    let mut app = App::new();
    app.mailbox.saved_searches = vec![fixture_saved_search("Unread", "is:unread", 0)];

    // Only one saved search exists; asking for the third should be a
    // safe no-op (no panic, no spurious search activation).
    app.apply(Action::OpenSavedSearchByIndex(3));

    assert!(
        !app.search.active,
        "out-of-range index does not activate any search"
    );
    assert!(
        app.search.bar.query.is_empty(),
        "no query was applied for an out-of-range index"
    );
}

#[test]
fn open_saved_search_with_empty_registry_is_noop() {
    let mut app = App::new();
    // No saved searches configured.
    app.apply(Action::OpenSavedSearchByIndex(1));

    assert!(
        !app.search.active,
        "no saved searches means the action has nothing to do"
    );
}
