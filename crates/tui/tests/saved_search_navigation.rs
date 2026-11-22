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
fn open_saved_search_by_index_zero_clears_active_filter() {
    let mut app = App::new();
    app.mailbox.saved_searches = vec![fixture_saved_search("Unread", "is:unread", 0)];

    app.apply(Action::OpenSavedSearchByIndex(1));
    assert!(
        app.search.active,
        "precondition: a tab is active before clearing"
    );

    app.apply(Action::OpenSavedSearchByIndex(0));

    assert!(
        !app.search.active,
        "index 0 returns to the default inbox view"
    );
}

#[test]
fn open_saved_search_with_empty_registry_is_noop() {
    let mut app = App::new();
    app.apply(Action::OpenSavedSearchByIndex(1));

    assert!(
        !app.search.active,
        "no saved searches means the action has nothing to do"
    );
}
