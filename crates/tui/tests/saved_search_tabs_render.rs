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

#[test]
fn tab_strip_renders_first_nine_saved_searches() {
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
        let needle = format!("g{i} search-{i:02}");
        assert!(
            !rendered.contains(&needle),
            "saved search beyond #9 must not appear in the strip. \
             leaked: `{needle}` in {rendered}"
        );
    }
}
