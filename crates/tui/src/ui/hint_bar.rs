use crate::app::{ActivePane, Screen};
use crate::keybindings::{display_bindings_for_actions, ViewContext};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct HintBarState<'a> {
    pub screen: Screen,
    pub active_pane: &'a ActivePane,
    pub search_active: bool,
    pub help_modal_open: bool,
    pub selected_count: usize,
    pub bulk_confirm_open: bool,
    pub sync_status: Option<String>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: HintBarState<'_>, theme: &Theme) {
    let lines = if state.bulk_confirm_open {
        vec![hint_line(
            &[("Enter", "Confirm"), ("y", "Confirm"), ("Esc", "Cancel")],
            theme,
        )]
    } else if state.help_modal_open {
        vec![hint_line(
            &[("Esc", "Close Help"), ("?", "Toggle Help")],
            theme,
        )]
    } else if state.search_active && state.screen == Screen::Mailbox {
        vec![hint_line(
            &[("Enter", "Confirm Search"), ("Esc", "Cancel Search")],
            theme,
        )]
    } else {
        build_lines(
            &hints_for_state(state.screen, state.active_pane, state.selected_count),
            theme,
        )
    };

    // Split area: hints on left, sync status on right
    let sync_text = state.sync_status.as_deref().unwrap_or("not synced");
    let sync_color = if sync_text == "syncing" {
        theme.accent
    } else if sync_text.starts_with("synced") {
        theme.success
    } else if sync_text == "degraded" {
        theme.warning
    } else {
        theme.error
    };
    let sync_width = (sync_text.len() + 4) as u16; // "● " + text + padding

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(sync_width)])
        .split(area);

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.hint_bar_bg)),
        chunks[0],
    );

    // Sync status indicator
    let sync_line = Line::from(vec![
        Span::styled("● ", Style::default().fg(sync_color)),
        Span::styled(sync_text.to_string(), Style::default().fg(theme.text_muted)),
    ]);
    frame.render_widget(
        Paragraph::new(sync_line).style(Style::default().bg(theme.hint_bar_bg)),
        chunks[1],
    );
}

pub fn hints_for_state(
    screen: Screen,
    active_pane: &ActivePane,
    selected_count: usize,
) -> Vec<(String, String)> {
    let context = match active_pane {
        ActivePane::Sidebar => ViewContext::MailList,
        ActivePane::MailList => ViewContext::MailList,
        ActivePane::MessageView => ViewContext::ThreadView,
    };

    match screen {
        Screen::Search => display_bindings_for_actions(
            context,
            &[
                "move_down",
                "move_up",
                "open",
                "search",
                "reply",
                "archive",
                "attachment_list",
                "help",
            ],
        ),
        Screen::Rules => vec![
            ("j".to_string(), "Down".to_string()),
            ("k".to_string(), "Up".to_string()),
            ("n".to_string(), "New Rule".to_string()),
            ("E".to_string(), "Edit Rule".to_string()),
            ("e".to_string(), "Toggle Enabled".to_string()),
            ("D".to_string(), "Dry Run".to_string()),
            ("H".to_string(), "History".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        Screen::Diagnostics => vec![
            ("Tab".to_string(), "Next Pane".to_string()),
            ("Shift-Tab".to_string(), "Prev Pane".to_string()),
            ("Enter/o".to_string(), "Full".to_string()),
            ("d".to_string(), "Details".to_string()),
            ("r".to_string(), "Refresh".to_string()),
            ("b".to_string(), "Bug Report".to_string()),
            ("L/gL".to_string(), "Logs".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        Screen::Accounts => vec![
            ("n".to_string(), "New".to_string()),
            ("t".to_string(), "Test".to_string()),
            ("d".to_string(), "Default".to_string()),
            ("Enter".to_string(), "Edit".to_string()),
            ("r".to_string(), "Refresh".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        Screen::Mailbox => match active_pane {
            ActivePane::Sidebar => display_bindings_for_actions(
                context,
                &[
                    "move_down",
                    "move_up",
                    "open",
                    "switch_panes",
                    "command_palette",
                    "help",
                ],
            ),
            ActivePane::MailList if selected_count > 0 => {
                let mut hints = display_bindings_for_actions(
                    context,
                    &[
                        "archive",
                        "trash",
                        "apply_label",
                        "move_to_label",
                        "mark_read",
                        "mark_unread",
                        "star",
                        "command_palette",
                        "help",
                    ],
                );
                hints.insert(0, ("Esc".to_string(), "Clear Sel".to_string()));
                hints
            }
            ActivePane::MailList => display_bindings_for_actions(
                context,
                &[
                    "move_down",
                    "move_up",
                    "open",
                    "reply",
                    "reply_all",
                    "forward",
                    "apply_label",
                    "search",
                    "toggle_select",
                    "command_palette",
                    "help",
                ],
            ),
            ActivePane::MessageView => display_bindings_for_actions(
                context,
                &[
                    "next_message",
                    "prev_message",
                    "reply",
                    "reply_all",
                    "forward",
                    "archive",
                    "star",
                    "attachment_list",
                    "toggle_reader_mode",
                    "help",
                ],
            ),
        },
    }
}

fn build_lines(hints: &[(String, String)], theme: &Theme) -> Vec<Line<'static>> {
    let mid = hints.len().div_ceil(2);
    vec![
        hint_line_owned(&hints[..mid], theme),
        hint_line_owned(&hints[mid..], theme),
    ]
}

fn hint_line(hints: &[(&str, &str)], theme: &Theme) -> Line<'static> {
    Line::from(
        hints
            .iter()
            .flat_map(|(key, action)| {
                [
                    Span::styled(
                        format!(" {key}"),
                        Style::default().fg(theme.text_primary).bold(),
                    ),
                    Span::styled(
                        format!(":{action}  "),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]
            })
            .collect::<Vec<_>>(),
    )
}

fn hint_line_owned(hints: &[(String, String)], theme: &Theme) -> Line<'static> {
    Line::from(
        hints
            .iter()
            .flat_map(|(key, action)| {
                [
                    Span::styled(
                        format!(" {key}"),
                        Style::default().fg(theme.text_primary).bold(),
                    ),
                    Span::styled(
                        format!(":{action}  "),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::hints_for_state;
    use crate::app::{ActivePane, Screen};

    #[test]
    fn selected_mailbox_hints_include_bulk_actions_and_clear() {
        let hints = hints_for_state(Screen::Mailbox, &ActivePane::MailList, 3);
        let labels: Vec<String> = hints.into_iter().map(|(_, label)| label).collect();
        assert!(labels.contains(&"Clear Sel".to_string()));
        assert!(labels.contains(&"Archive".to_string()));
        assert!(labels.contains(&"Apply Label".to_string()));
    }
}
