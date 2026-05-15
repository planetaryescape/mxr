use crate::theme::Theme;
use mxr_core::id::SavedSearchId;
use mxr_core::{SavedSearch, SearchMode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashMap;

pub struct SavedSearchTabsView<'a> {
    pub searches: &'a [SavedSearch],
    pub active_query: Option<&'a str>,
    pub active_mode: Option<SearchMode>,
    /// Optional per-saved-search unread match count, keyed by id.
    /// When a saved search has a non-zero count, the label is
    /// suffixed with ` (N)`. Missing or zero counts render the bare
    /// label so the strip doesn't visually flap as counts settle.
    pub unread_counts: &'a HashMap<SavedSearchId, u32>,
}

pub fn draw(frame: &mut Frame, area: Rect, view: &SavedSearchTabsView<'_>, theme: &Theme) {
    if view.searches.is_empty() || area.height == 0 {
        return;
    }

    let mut spans = vec![Span::styled(
        "g0 Inbox",
        Style::default().fg(if view.active_query.is_none() {
            theme.accent
        } else {
            theme.text_muted
        }),
    )];
    for (index, search) in view.searches.iter().take(9).enumerate() {
        spans.push(Span::styled("  ", Style::default().fg(theme.text_muted)));
        let active = view.active_query.is_some_and(|query| query == search.query)
            && view.active_mode == Some(search.search_mode);
        let style = if active {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_secondary)
        };
        let count = view.unread_counts.get(&search.id).copied().unwrap_or(0);
        let label = if count > 0 {
            format!("g{} {} ({count})", index + 1, search.name)
        } else {
            format!("g{} {}", index + 1, search.name)
        };
        spans.push(Span::styled(label, style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border_unfocused)),
        ),
        area,
    );
}
