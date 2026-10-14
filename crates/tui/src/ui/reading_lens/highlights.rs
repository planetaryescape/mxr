//! The `H` panel: the open item's highlights with their notes, and the
//! three things you can do to the selected one. Pure render; the keys live
//! in `app/reading_highlights.rs`.

use crate::app::{HighlightStep, ReadingPageState};
use crate::ui::sanitize::{one_line, truncate};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub(super) fn draw(
    frame: &mut Frame,
    area: Rect,
    page: &ReadingPageState,
    theme: &crate::theme::Theme,
) {
    let (Some(panel), Some(reader)) = (page.highlights.as_ref(), page.reader.as_ref()) else {
        return;
    };
    let highlights = &reader.highlights;
    let selected = panel.selected.min(highlights.len().saturating_sub(1));
    let height = u16::try_from(highlights.len() * 2)
        .unwrap_or(u16::MAX)
        .saturating_add(5)
        .min(area.height);
    let popup = super::super::centered_rect_fixed_height(76, height, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(format!(" Highlights ({}) ", highlights.len()))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let width = usize::from(inner.width).saturating_sub(4);

    let mut lines: Vec<Line> = Vec::new();
    for (index, highlight) in highlights.iter().enumerate() {
        let here = index == selected;
        let marker = if here { "> " } else { "  " };
        let quote_style = if here {
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_secondary)
        };
        let origin = if highlight.origin == "mxr" {
            ""
        } else {
            " (made in Marginalia)"
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{marker}{}{origin}",
                truncate(&one_line(&highlight.quote), width.saturating_sub(origin.len()))
            ),
            quote_style,
        )));
        let note = match (&panel.step, here) {
            (HighlightStep::Note(typing), true) => format!("Note: {typing}\u{2581}"),
            _ => highlight
                .note
                .as_deref()
                .map(|note| format!("Note: {}", truncate(&one_line(note), width)))
                .unwrap_or_default(),
        };
        lines.push(Line::from(Span::styled(
            format!("    {note}"),
            Style::default().fg(theme.text_muted),
        )));
    }
    lines.push(Line::from(""));
    let (hint, hint_style) = match &panel.step {
        HighlightStep::Browse => (
            "j/k move \u{b7} e note \u{b7} x remove \u{b7} u undo \u{b7} Esc close".to_string(),
            Style::default().fg(theme.text_muted),
        ),
        HighlightStep::Note(_) => (
            "Enter save (blank clears it) \u{b7} Esc cancel".to_string(),
            Style::default().fg(theme.text_muted),
        ),
        HighlightStep::Remove => (
            "Remove the selected highlight? Enter removes \u{b7} u undoes it \u{b7} Esc keeps it"
                .to_string(),
            Style::default().fg(theme.warning),
        ),
    };
    lines.push(Line::from(Span::styled(hint, hint_style)));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
