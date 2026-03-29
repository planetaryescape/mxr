use crate::app::{snooze_presets, SnoozePanelState, SnoozePreset};
use mxr_config::SnoozeConfig;
use ratatui::prelude::*;
use ratatui::widgets::*;

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

    let popup = centered_rect(46, 38, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Snooze ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    let items: Vec<ListItem> = snooze_presets()
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
    let presets_len = snooze_presets().len();
    let list_height = chunks[0].height as usize;
    frame.render_widget(List::new(items), chunks[0]);

    if presets_len > list_height {
        let mut scrollbar_state = ScrollbarState::new(presets_len.saturating_sub(list_height))
            .position(panel.selected_index);
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

fn format_preset(preset: SnoozePreset, config: &SnoozeConfig) -> String {
    mxr_config::snooze::format_preset(preset, config)
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
