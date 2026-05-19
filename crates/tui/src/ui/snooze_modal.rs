#![cfg_attr(test, allow(clippy::items_after_test_module))]

use crate::app::{snooze_presets, SnoozePanelState, SnoozePreset};
use mxr_config::SnoozeConfig;
use ratatui::prelude::*;
use ratatui::widgets::*;

/// Row label rendered for the trailing "Custom..." entry that opens
/// the conversational time input. Kept as a constant so tests can match
/// it without re-hardcoding the string.
pub const CUSTOM_ROW_LABEL: &str = "Custom… (e.g. tomorrow 9am, in 2h)";

/// Length of the preset list including the trailing custom-input row.
/// `selected_index == presets_len()` is the custom row; everything below
/// is a regular preset.
pub fn rows_count() -> usize {
    snooze_presets().len() + 1
}

pub fn custom_row_index() -> usize {
    snooze_presets().len()
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    panel: &SnoozePanelState,
    config: &SnoozeConfig,
    theme: &crate::theme::Theme,
) {
    if !panel.visible {
        return;
    }

    let popup = centered_rect(50, 60, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Snooze ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if panel.custom_input.is_some() {
        draw_custom_input(frame, inner, panel, theme);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    let mut items: Vec<ListItem> = snooze_presets()
        .iter()
        .enumerate()
        .map(|(index, preset)| {
            let label = format_preset(*preset, config);
            let style = if index == panel.selected_index {
                theme.highlight_style()
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();
    let custom_idx = custom_row_index();
    let custom_style = if custom_idx == panel.selected_index {
        theme.highlight_style()
    } else {
        Style::default().fg(theme.text_secondary)
    };
    items.push(ListItem::new(CUSTOM_ROW_LABEL).style(custom_style));

    let total = rows_count();
    let list_height = chunks[0].height as usize;
    frame.render_widget(List::new(items), chunks[0]);

    if total > list_height {
        let mut scrollbar_state =
            ScrollbarState::new(total.saturating_sub(list_height)).position(panel.selected_index);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.warning)),
            chunks[0],
            &mut scrollbar_state,
        );
    }

    frame.render_widget(
        Paragraph::new("Enter snooze  j/k move  Esc cancel")
            .style(Style::default().fg(theme.text_secondary)),
        chunks[1],
    );
}

fn draw_custom_input(
    frame: &mut Frame,
    inner: Rect,
    panel: &SnoozePanelState,
    theme: &crate::theme::Theme,
) {
    let input = panel.custom_input.as_deref().unwrap_or("");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new("Type a custom snooze time:").style(Style::default().fg(theme.text_primary)),
        chunks[0],
    );
    let prompt = format!(" › {input}_");
    frame.render_widget(
        Paragraph::new(prompt).style(Style::default().fg(theme.text_primary)),
        chunks[1],
    );
    let hint = "Examples: in 2h · tomorrow 9am · monday 17:00 · 2026-06-01T15:00:00Z";
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(theme.text_secondary)),
        chunks[2],
    );
    if let Some(error) = &panel.custom_error {
        frame.render_widget(
            Paragraph::new(format!("Error: {error}")).style(Style::default().fg(theme.error)),
            chunks[3],
        );
    }
    frame.render_widget(
        Paragraph::new("Enter parse  Backspace  Esc back to presets")
            .style(Style::default().fg(theme.text_secondary)),
        chunks[4],
    );
}

fn format_preset(preset: SnoozePreset, config: &SnoozeConfig) -> String {
    mxr_config::snooze::format_preset(preset, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_test_support::render_to_string;

    #[test]
    fn rows_count_includes_custom_entry() {
        // The list rows = N presets + 1 "Custom…" trailer. The contract
        // matters because input.rs uses `rows_count()` for cursor wrap.
        assert_eq!(rows_count(), snooze_presets().len() + 1);
        assert_eq!(custom_row_index(), snooze_presets().len());
    }

    #[test]
    fn renders_custom_row_in_preset_list() {
        let panel = SnoozePanelState {
            visible: true,
            ..Default::default()
        };
        let snapshot = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &panel,
                &SnoozeConfig::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("Custom"),
            "Custom… row must appear in preset list; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_input_prompt_with_examples_in_custom_mode() {
        let panel = SnoozePanelState {
            visible: true,
            selected_index: custom_row_index(),
            custom_input: Some("in 2h".into()),
            custom_error: None,
        };
        let snapshot = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &panel,
                &SnoozeConfig::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("Type a custom"),
            "custom-mode prompt must appear; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("in 2h"),
            "current buffer must be visible; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("tomorrow 9am"),
            "examples line must surface; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_validation_error_inline_in_custom_mode() {
        let panel = SnoozePanelState {
            visible: true,
            selected_index: custom_row_index(),
            custom_input: Some("asdf".into()),
            custom_error: Some("couldn't parse `asdf` as a time".into()),
        };
        let snapshot = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &panel,
                &SnoozeConfig::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("Error:"),
            "validation error must surface in modal; got:\n{snapshot}",
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
