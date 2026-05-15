//! Render-level checks for the saved-search tab strip above the inbox.
//!
//! These tests pin two pieces of contract:
//!
//! 1. **Top-of-strip is bounded to nine tabs.** Adding a tenth saved
//!    search must not push it into the strip; the `g0..g9` keyboard
//!    surface only addresses positions 0–9.
//! 2. **The user-facing label is the saved-search name, not the
//!    underlying query.** Names are how the user named their workspace;
//!    the query is the implementation behind it. Swapping the rendered
//!    label to the query would silently regress the UX even though the
//!    state machine looks fine.

use chrono::{TimeZone, Utc};
use mxr_core::id::SavedSearchId;
use mxr_core::types::{SavedSearch, SearchMode, SortOrder};
use mxr_test_support::render_to_string;
use mxr_tui::theme::Theme;
use mxr_tui::ui::saved_search_tabs::{draw, SavedSearchTabsView};
use ratatui::layout::Rect;

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

/// Phase 1.5: at most nine saved-search tabs appear in the strip. The
/// keybinding registry only maps `g0..g9`, so anything beyond `g9` is
/// inaccessible from the keyboard and must not be rendered.
#[test]
fn tab_strip_renders_first_nine_saved_searches() {
    // Twelve searches with deliberately distinct names so we can spot
    // which ones made the cut.
    let searches: Vec<SavedSearch> = (0..12)
        .map(|i| {
            fixture_saved_search(
                &format!("search-{:02}", i + 1),
                &format!("is:label-{:02}", i + 1),
                i,
            )
        })
        .collect();

    let theme = Theme::default();
    // The strip widget draws a 1-row bottom border, so we need at least
    // two rows for the labels to land above it.
    let rendered = render_to_string(160, 2, |frame| {
        draw(
            frame,
            Rect::new(0, 0, 160, 2),
            &SavedSearchTabsView {
                searches: &searches,
                active_query: None,
                active_mode: None,
                unread_counts: &std::collections::HashMap::new(),
            },
            &theme,
        );
    });

    // The strip always prefixes a `g0 Inbox` slot, then `g1`..`g9`.
    assert!(
        rendered.contains("g0 Inbox"),
        "strip starts with the `g0 Inbox` default slot. got: {rendered}"
    );
    for i in 1..=9 {
        let needle = format!("g{i} search-{i:02}");
        assert!(
            rendered.contains(&needle),
            "saved search #{i} renders as `{needle}`. got: {rendered}"
        );
    }
    for i in 10..=12 {
        // The strip stops at g9, so g10/g11/g12 must not appear at all —
        // both because no keybinding addresses them and because letting
        // them render would visually imply otherwise.
        let needle = format!("g{i} search-{i:02}");
        assert!(
            !rendered.contains(&needle),
            "saved search beyond #9 must not appear in the strip. \
             leaked: `{needle}` in {rendered}"
        );
    }
}

/// Phase 1.5: each tab shows its unread match count. The user's
/// inbox is the home base; saved-search tabs are workspaces. They
/// need to surface "new stuff arrived for me" the same way Gmail
/// surfaces unread counts next to labels.
#[test]
fn tab_unread_count_reflects_search_match_count() {
    let searches = vec![
        fixture_saved_search("Unread", "is:unread", 0),
        fixture_saved_search("From Alice", "from:alice", 1),
        fixture_saved_search("Quiet", "label:archived", 2),
    ];

    let mut counts = std::collections::HashMap::new();
    counts.insert(searches[0].id.clone(), 12);
    counts.insert(searches[1].id.clone(), 3);
    // searches[2] intentionally absent / zero → bare label.

    let theme = Theme::default();
    let rendered = render_to_string(120, 2, |frame| {
        draw(
            frame,
            Rect::new(0, 0, 120, 2),
            &SavedSearchTabsView {
                searches: &searches,
                active_query: None,
                active_mode: None,
                unread_counts: &counts,
            },
            &theme,
        );
    });

    assert!(
        rendered.contains("g1 Unread (12)"),
        "saved search with unread count shows `(N)`. got: {rendered}"
    );
    assert!(
        rendered.contains("g2 From Alice (3)"),
        "second saved search count appears too. got: {rendered}"
    );
    // Zero / missing count renders bare label — no `(0)` clutter.
    assert!(
        rendered.contains("g3 Quiet"),
        "tab with no unread count keeps the bare label. got: {rendered}"
    );
    assert!(
        !rendered.contains("g3 Quiet ("),
        "tab with zero unread must not show `(0)`. got: {rendered}"
    );
}

/// Phase 1.5: the rendered label is the saved-search NAME, not its
/// query. If we ever flipped to rendering the query string by accident
/// the tabs would become unreadable for any non-trivial saved search.
#[test]
fn tab_strip_renders_saved_search_name_not_query() {
    let searches = vec![fixture_saved_search(
        "Open replies",
        "is:owed-reply -has:label:archived",
        0,
    )];

    let theme = Theme::default();
    let rendered = render_to_string(80, 2, |frame| {
        draw(
            frame,
            Rect::new(0, 0, 80, 2),
            &SavedSearchTabsView {
                searches: &searches,
                active_query: None,
                active_mode: None,
                unread_counts: &std::collections::HashMap::new(),
            },
            &theme,
        );
    });

    assert!(
        rendered.contains("g1 Open replies"),
        "tab label is the user-given name. got: {rendered}"
    );
    assert!(
        !rendered.contains("is:owed-reply"),
        "tab label must not leak the underlying query. got: {rendered}"
    );
}
