//! Activity log modal (Phase 5). Read-only viewer of the last day of
//! local `user_activity` rows. Lives over whatever screen the user is on
//! — opened via `g y` chord or palette entry.

use crate::app::ActivityModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;
use super::centered_rect;

const MODAL_WIDTH_PERCENT: u16 = 90;
const MODAL_HEIGHT_PERCENT: u16 = 80;

pub fn draw(frame: &mut Frame, area: Rect, state: &ActivityModalState, theme: &Theme) {
    if !state.visible {
        return;
    }
    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let pause_marker = if state.paused { " [PAUSED]" } else { "" };
    let title = format!(
        " Activity — local-only, last 24h{pause_marker} · j/k navigate · p pause · Esc close "
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if let Some(err) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to load activity: {err}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Loading recent activity...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.entries.is_empty() {
        let paragraph = Paragraph::new(
            "No activity in the last 24h.\n\nmxr starts recording as you use it — read, search, archive, send — locally only.",
        )
        .style(Style::default().fg(theme.text_muted))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner.inner(Margin::new(2, 2)));
        return;
    }

    // Split: rows table on top, detail panel below.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(8)])
        .split(inner.inner(Margin::new(1, 1)));

    // Rows table
    let rows: Vec<Row> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let ts = chrono::DateTime::from_timestamp_millis(e.ts)
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_default();
            let target = match (&e.target_kind, &e.target_id) {
                (Some(k), Some(id)) => {
                    let trimmed: String = id.chars().take(12).collect();
                    format!("{k}:{trimmed}")
                }
                (Some(k), None) => k.clone(),
                _ => "—".into(),
            };
            let context = if e.redacted {
                "(redacted)".to_string()
            } else if let Some(c) = &e.context {
                serde_json::to_string(c)
                    .unwrap_or_default()
                    .chars()
                    .take(40)
                    .collect()
            } else {
                String::new()
            };
            let style = if i == state.selected_index {
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
            } else if e.redacted {
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::DIM)
            } else {
                Style::default().fg(theme.text_primary)
            };
            Row::new(vec![
                Cell::from(ts),
                Cell::from(format!("{:?}", e.source).to_lowercase()),
                Cell::from(e.action.clone()),
                Cell::from(target),
                Cell::from(context),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(24),
            Constraint::Length(28),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["TIME", "SRC", "ACTION", "TARGET", "CONTEXT"]).style(
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        ),
    );
    frame.render_widget(table, chunks[0]);

    // Detail panel
    let detail_text = if let Some(entry) = state.selected() {
        let ts = chrono::DateTime::from_timestamp_millis(entry.ts)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();
        let context = if entry.redacted {
            "(redacted)".to_string()
        } else if let Some(c) = &entry.context {
            serde_json::to_string_pretty(c).unwrap_or_default()
        } else {
            "—".into()
        };
        format!(
            "ID: {}    Time: {ts}    Tier: {:?}\nAction: {}\nTarget: {} {}\nContext: {context}",
            entry.id,
            entry.tier,
            entry.action,
            entry.target_kind.as_deref().unwrap_or("—"),
            entry.target_id.as_deref().unwrap_or("—"),
        )
    } else {
        String::new()
    };
    let detail = Paragraph::new(detail_text)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme.border_unfocused)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(detail, chunks[1]);
}

