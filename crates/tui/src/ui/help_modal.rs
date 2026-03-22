use crate::app::{ActivePane, Screen};
use crate::keybindings::{all_bindings_for_context, ViewContext};
use crate::ui::command_palette::default_commands;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
struct HelpSection {
    title: String,
    entries: Vec<(String, String)>,
}

pub struct HelpModalState<'a> {
    pub open: bool,
    pub screen: Screen,
    pub active_pane: &'a ActivePane,
    pub selected_count: usize,
    pub scroll_offset: u16,
}

pub fn draw(frame: &mut Frame, area: Rect, state: HelpModalState<'_>, theme: &crate::theme::Theme) {
    if !state.open {
        return;
    }

    let popup = centered_rect(88, 88, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Help ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = render_sections(&help_sections(&state), theme);
    let content_height = lines.len();
    let paragraph = Paragraph::new(lines)
        .scroll((state.scroll_offset, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);

    let mut scrollbar_state =
        ScrollbarState::new(content_height.saturating_sub(inner.height as usize))
            .position(state.scroll_offset as usize);
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.warning)),
        inner,
        &mut scrollbar_state,
    );
}

fn help_sections(state: &HelpModalState<'_>) -> Vec<HelpSection> {
    let mut sections = vec![
        HelpSection {
            title: "Global".into(),
            entries: vec![
                ("Ctrl-p".into(), "Command Palette".into()),
                ("?".into(), "Toggle Help".into()),
                ("Esc".into(), "Back / Close".into()),
                ("q".into(), "Quit".into()),
            ],
        },
        HelpSection {
            title: "Current Context".into(),
            entries: context_entries(state),
        },
        HelpSection {
            title: "Mailbox List".into(),
            entries: all_bindings_for_context(ViewContext::MailList),
        },
        HelpSection {
            title: "Thread View".into(),
            entries: all_bindings_for_context(ViewContext::ThreadView),
        },
        HelpSection {
            title: "Single Message".into(),
            entries: all_bindings_for_context(ViewContext::MessageView),
        },
        HelpSection {
            title: "Search Page".into(),
            entries: vec![
                ("/".into(), "Edit Query".into()),
                ("Enter".into(), "Run Query / Open Result".into()),
                ("o".into(), "Open In Mailbox".into()),
                ("j / k".into(), "Move Results".into()),
                ("Esc".into(), "Return To Mailbox".into()),
            ],
        },
        HelpSection {
            title: "Rules Page".into(),
            entries: vec![
                ("j / k".into(), "Move Rules".into()),
                ("n".into(), "New Rule".into()),
                ("E".into(), "Edit Rule".into()),
                ("e".into(), "Enable / Disable".into()),
                ("D".into(), "Dry Run".into()),
                ("H".into(), "History".into()),
                ("#".into(), "Delete Rule".into()),
            ],
        },
        HelpSection {
            title: "Diagnostics Page".into(),
            entries: vec![
                ("Tab / Shift-Tab".into(), "Select Pane".into()),
                ("j / k Ctrl-d/u".into(), "Scroll Selected Pane".into()),
                ("Enter / o".into(), "Toggle Fullscreen Pane".into()),
                ("d".into(), "Open Selected Pane Details".into()),
                ("r".into(), "Refresh".into()),
                ("b".into(), "Generate Bug Report".into()),
                ("L / gL".into(), "Open Log File".into()),
            ],
        },
        HelpSection {
            title: "Accounts Page".into(),
            entries: vec![
                ("n".into(), "New IMAP/SMTP Account".into()),
                ("Enter".into(), "Edit Selected Account".into()),
                ("t".into(), "Test Account".into()),
                ("d".into(), "Set Default".into()),
                ("r".into(), "Refresh Accounts".into()),
            ],
        },
        HelpSection {
            title: "Modals".into(),
            entries: vec![
                ("Help: j/k Ctrl-d/u".into(), "Scroll".into()),
                ("Label Picker".into(), "Type, j/k, Enter, Esc".into()),
                ("Compose Picker".into(), "Type, Tab, Enter, Esc".into()),
                ("Attachments".into(), "j/k, Enter/o, d, Esc".into()),
                ("Links".into(), "j/k, Enter/o open, y copy, Esc".into()),
                (
                    "Unsubscribe".into(),
                    "Enter unsubscribe, a archive sender, Esc cancel".into(),
                ),
                (
                    "Bulk Confirm".into(),
                    "Enter/y confirm, Esc/n cancel".into(),
                ),
            ],
        },
    ];

    sections.extend(command_sections());
    sections
}

fn context_entries(state: &HelpModalState<'_>) -> Vec<(String, String)> {
    let mut entries = vec![(
        "Screen".into(),
        match state.screen {
            Screen::Mailbox => "Mailbox".into(),
            Screen::Search => "Search".into(),
            Screen::Rules => "Rules".into(),
            Screen::Diagnostics => "Diagnostics".into(),
            Screen::Accounts => "Accounts".into(),
        },
    )];

    if state.screen == Screen::Mailbox {
        entries.push((
            "Pane".into(),
            match state.active_pane {
                ActivePane::Sidebar => "Sidebar".into(),
                ActivePane::MailList => "Mail List".into(),
                ActivePane::MessageView => "Message".into(),
            },
        ));
    }

    if state.selected_count > 0 {
        entries.push((
            "Selection".into(),
            format!(
                "{} selected: archive, delete, label, move, read/unread, star, Esc clears",
                state.selected_count
            ),
        ));
    }

    entries
}

fn command_sections() -> Vec<HelpSection> {
    let mut by_category: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for command in default_commands() {
        let shortcut = if command.shortcut.is_empty() {
            "palette".to_string()
        } else {
            command.shortcut
        };
        by_category
            .entry(command.category)
            .or_default()
            .push((shortcut, command.label));
    }

    by_category
        .into_iter()
        .map(|(category, mut entries)| {
            entries.sort_by(|left, right| left.1.cmp(&right.1));
            HelpSection {
                title: format!("Commands: {category}"),
                entries,
            }
        })
        .collect()
}

fn render_sections(sections: &[HelpSection], theme: &crate::theme::Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            section.title.clone(),
            Style::default().fg(theme.accent).bold(),
        )));
        for (key, action) in &section.entries {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{key:<20}"),
                    Style::default().fg(theme.text_primary).bold(),
                ),
                Span::styled(action.clone(), Style::default().fg(theme.text_secondary)),
            ]));
        }
    }

    lines
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

#[cfg(test)]
mod tests {
    use super::{help_sections, HelpModalState};
    use crate::app::{ActivePane, Screen};

    #[test]
    fn help_sections_cover_accounts_and_commands() {
        let state = HelpModalState {
            open: true,
            screen: Screen::Accounts,
            active_pane: &ActivePane::MailList,
            selected_count: 2,
            scroll_offset: 0,
        };
        let titles: Vec<String> = help_sections(&state)
            .into_iter()
            .map(|section| section.title)
            .collect();
        assert!(titles.contains(&"Accounts Page".to_string()));
        assert!(titles
            .iter()
            .any(|title| title.starts_with("Commands: Accounts")));
        assert!(titles
            .iter()
            .any(|title| title.starts_with("Commands: Mail")));
    }
}
