use crate::app::FeatureOnboardingState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &FeatureOnboardingState,
    theme: &crate::theme::Theme,
) {
    if !state.visible {
        return;
    }

    let popup = centered_rect(62, 42, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" Start Here {}/5 ", state.step + 1))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(step_lines(state.step))
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left),
        layout[0],
    );
    frame.render_widget(
        Paragraph::new("Enter/l next  h previous  Esc close").alignment(Alignment::Center),
        layout[1],
    );
}

fn step_lines(step: usize) -> Vec<Line<'static>> {
    match step {
        0 => vec![
            Line::from("mxr is keyboard-first, but you should not need to memorize it."),
            Line::from(""),
            Line::from("1. Mailbox is for triage and reading."),
            Line::from("2. Search is for the full local index."),
            Line::from("3. Rules automate repeatable cleanup."),
            Line::from("4. Accounts and Diagnostics manage setup."),
        ],
        1 => vec![
            Line::from("First sync"),
            Line::from(""),
            Line::from("Press 5 for Diagnostics if sync health looks wrong."),
            Line::from("Press r to refresh diagnostics."),
            Line::from("Press c to edit config without leaving the TUI."),
        ],
        2 => vec![
            Line::from("Full search"),
            Line::from(""),
            Line::from("Press / in Mailbox to jump into Search."),
            Line::from("Search hits the full local index, not just what is loaded on screen."),
            Line::from("Use Ctrl-f in Mailbox when you only want a quick local filter."),
        ],
        3 => vec![
            Line::from("Command palette"),
            Line::from(""),
            Line::from("Press Ctrl-p to find actions by name or shortcut."),
            Line::from("It is the fastest way to discover config edit, logs, and navigation."),
        ],
        _ => vec![
            Line::from("Rules and config"),
            Line::from(""),
            Line::from("Press 3 for Rules. Start with a dry run before saving."),
            Line::from("Press gc or c on Accounts / Diagnostics to open config."),
            Line::from("Open Help with ? any time, then search Start Here or use Ctrl-p to reopen this walkthrough."),
        ],
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, rect: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(rect);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
