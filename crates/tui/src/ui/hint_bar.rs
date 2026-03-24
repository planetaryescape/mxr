use crate::mxr_tui::action::UiContext;
use crate::mxr_tui::keybindings::{display_bindings_for_actions, ViewContext};
use crate::mxr_tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct HintBarState<'a> {
    pub ui_context: UiContext,
    pub search_active: bool,
    pub help_modal_open: bool,
    pub selected_count: usize,
    pub bulk_confirm_open: bool,
    pub sync_status: Option<String>,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: HintBarState<'_>, theme: &Theme) {
    let lines = if state.bulk_confirm_open {
        vec![hint_line(
            &[("Enter", "Confirm"), ("y", "Confirm"), ("Esc", "Cancel")],
            theme,
        )]
    } else if state.help_modal_open {
        vec![hint_line(
            &[
                ("Type", "Search"),
                ("j/k ↑↓", "Navigate"),
                ("Ctrl-u/d", "Page"),
                ("Esc", "Close"),
            ],
            theme,
        )]
    } else if state.search_active && matches!(state.ui_context, UiContext::MailboxList) {
        vec![hint_line(
            &[
                ("Enter", "Apply Filter"),
                ("Esc", "Clear Filter"),
                ("Ctrl-f", "Mailbox Filter"),
            ],
            theme,
        )]
    } else {
        build_lines(
            &hints_for_context(state.ui_context, state.selected_count),
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

pub fn hints_for_context(context: UiContext, selected_count: usize) -> Vec<(String, String)> {
    match context {
        UiContext::MailboxSidebar => display_bindings_for_actions(
            ViewContext::MailList,
            &[
                "move_down",
                "move_up",
                "open",
                "search_all_mail",
                "command_palette",
                "help",
            ],
        ),
        UiContext::MailboxList if selected_count > 0 => {
            let mut hints = display_bindings_for_actions(
                ViewContext::MailList,
                &[
                    "archive",
                    "mark_read_archive",
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
        UiContext::MailboxList => display_bindings_for_actions(
            ViewContext::MailList,
            &[
                "move_down",
                "move_up",
                "open",
                "reply",
                "archive",
                "search_all_mail",
                "mailbox_filter",
                "command_palette",
                "help",
            ],
        ),
        UiContext::MailboxMessage => display_bindings_for_actions(
            ViewContext::ThreadView,
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
        UiContext::SearchEditor => vec![
            ("Enter".to_string(), "Run Now".to_string()),
            ("Tab".to_string(), "Mode".to_string()),
            ("Esc".to_string(), "Stop Editing".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::SearchResults => vec![
            ("j".to_string(), "Next Result".to_string()),
            ("k".to_string(), "Prev Result".to_string()),
            ("Enter".to_string(), "Preview".to_string()),
            ("/".to_string(), "Edit Query".to_string()),
            ("Tab".to_string(), "Switch Pane".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::SearchPreview => {
            let mut hints = display_bindings_for_actions(
                ViewContext::ThreadView,
                &[
                    "reply",
                    "archive",
                    "toggle_select",
                    "attachment_list",
                    "open_links",
                    "toggle_reader_mode",
                    "help",
                ],
            );
            hints.insert(0, ("Esc".to_string(), "Results".to_string()));
            hints.insert(0, ("h".to_string(), "Results".to_string()));
            hints.insert(0, ("/".to_string(), "Edit Query".to_string()));
            hints
        }
        UiContext::RulesList => vec![
            ("j".to_string(), "Down".to_string()),
            ("k".to_string(), "Up".to_string()),
            ("Enter".to_string(), "Refresh".to_string()),
            ("n".to_string(), "New Rule".to_string()),
            ("E".to_string(), "Edit Rule".to_string()),
            ("e".to_string(), "Toggle Enabled".to_string()),
            ("D".to_string(), "Dry Run".to_string()),
            ("H".to_string(), "History".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::RulesForm => vec![
            ("Tab".to_string(), "Next Field".to_string()),
            ("Shift-Tab".to_string(), "Prev Field".to_string()),
            ("Ctrl-s".to_string(), "Save".to_string()),
            ("Esc".to_string(), "Close Form".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::Diagnostics => vec![
            ("j".to_string(), "Next Section".to_string()),
            ("k".to_string(), "Prev Section".to_string()),
            ("Ctrl-d".to_string(), "Scroll Down".to_string()),
            ("Ctrl-u".to_string(), "Scroll Up".to_string()),
            ("Enter/o".to_string(), "Full".to_string()),
            ("r".to_string(), "Refresh".to_string()),
            ("c".to_string(), "Config".to_string()),
            ("L".to_string(), "Logs".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::AccountsList => vec![
            ("j".to_string(), "Next Account".to_string()),
            ("k".to_string(), "Prev Account".to_string()),
            ("n".to_string(), "New".to_string()),
            ("t".to_string(), "Test".to_string()),
            ("d".to_string(), "Default".to_string()),
            ("c".to_string(), "Config".to_string()),
            ("Enter".to_string(), "Edit".to_string()),
            ("r".to_string(), "Refresh".to_string()),
            ("Esc".to_string(), "Mailbox".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
        UiContext::AccountsForm => vec![
            ("j/k".to_string(), "Fields".to_string()),
            ("Tab".to_string(), "Next Field".to_string()),
            ("Shift-Tab".to_string(), "Prev Field".to_string()),
            ("h/l".to_string(), "Mode".to_string()),
            ("Enter/i".to_string(), "Edit".to_string()),
            ("s".to_string(), "Save".to_string()),
            ("t".to_string(), "Test".to_string()),
            ("Esc".to_string(), "Close".to_string()),
            ("?".to_string(), "Help".to_string()),
        ],
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
    use super::hints_for_context;
    use crate::mxr_tui::action::UiContext;

    #[test]
    fn selected_mailbox_hints_include_bulk_actions_and_clear() {
        let hints = hints_for_context(UiContext::MailboxList, 3);
        let labels: Vec<String> = hints.into_iter().map(|(_, label)| label).collect();
        assert!(labels.contains(&"Clear Sel".to_string()));
        assert!(labels.contains(&"Archive".to_string()));
        assert!(labels.contains(&"Read + Archive".to_string()));
        assert!(labels.contains(&"Apply Label".to_string()));
    }
}
