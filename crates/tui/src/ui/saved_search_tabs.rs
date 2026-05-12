use crate::theme::Theme;
use mxr_core::{SavedSearch, SearchMode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct SavedSearchTabsView<'a> {
    pub searches: &'a [SavedSearch],
    pub active_query: Option<&'a str>,
    pub active_mode: Option<SearchMode>,
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
        spans.push(Span::styled(
            format!("g{} {}", index + 1, search.name),
            style,
        ));
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
