use crate::app::{snooze_presets, SnoozePanelState, SnoozePreset};
use mxr_config::SnoozeConfig;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, panel: &SnoozePanelState, config: &SnoozeConfig) {
    if !panel.visible {
        return;
    }

    let popup = centered_rect(46, 38, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Snooze ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Rgb(18, 18, 26)));
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
                Style::default().bg(Color::DarkGray).fg(Color::White).bold()
            } else {
                Style::default()
            };
            ListItem::new(label).style(style)
        })
        .collect();
    frame.render_widget(List::new(items), chunks[0]);

    frame.render_widget(
        Paragraph::new("Enter snooze  j/k move  Esc cancel")
            .style(Style::default().fg(Color::Gray)),
        chunks[1],
    );
}

fn format_preset(preset: SnoozePreset, config: &SnoozeConfig) -> String {
    match preset {
        SnoozePreset::TomorrowMorning => format!("Tomorrow morning ({:02}:00)", config.morning_hour),
        SnoozePreset::Tonight => format!("Tonight ({:02}:00)", config.evening_hour),
        SnoozePreset::Weekend => format!(
            "{} ({:02}:00)",
            capitalize(&config.weekend_day),
            config.weekend_hour
        ),
        SnoozePreset::NextMonday => format!("Monday ({:02}:00)", config.morning_hour),
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
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
