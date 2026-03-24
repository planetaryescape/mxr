use crate::mxr_tui::action::UiContext;
use crate::mxr_tui::keybindings::{all_bindings_for_context, ViewContext};
use crate::mxr_tui::ui::command_palette::commands_for_context;
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
    pub ui_context: UiContext,
    pub selected_count: usize,
    pub scroll_offset: u16,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: HelpModalState<'_>,
    theme: &crate::mxr_tui::theme::Theme,
) {
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

    sections.extend(screen_sections(state.ui_context));
    sections.extend(command_sections(state.ui_context));
    sections
}

fn context_entries(state: &HelpModalState<'_>) -> Vec<(String, String)> {
    let mut entries = vec![("Context".into(), state.ui_context.label().into())];

    if state.selected_count > 0 {
        entries.push((
            "Selection".into(),
            format!(
                "{} selected: archive, delete, label, move, read/unread, star, Esc clears",
                state.selected_count
            ),
        ));
    }

    if matches!(
        state.ui_context,
        UiContext::SearchEditor | UiContext::SearchResults | UiContext::SearchPreview
    ) {
        entries.push((
            "Search".into(),
            "Search tab hits the full local index; mailbox / is only a quick filter".into(),
        ));
    }

    entries
}

fn screen_sections(context: UiContext) -> Vec<HelpSection> {
    match context {
        UiContext::MailboxSidebar => vec![HelpSection {
            title: "Mailbox Sidebar".into(),
            entries: all_bindings_for_context(ViewContext::MailList),
        }],
        UiContext::MailboxList => vec![HelpSection {
            title: "Mailbox List".into(),
            entries: all_bindings_for_context(ViewContext::MailList),
        }],
        UiContext::MailboxMessage => vec![HelpSection {
            title: "Mailbox Message".into(),
            entries: all_bindings_for_context(ViewContext::ThreadView),
        }],
        UiContext::SearchEditor | UiContext::SearchResults => {
            vec![HelpSection {
                title: "Search Page".into(),
                entries: vec![
                    ("/".into(), "Edit query".into()),
                    ("Enter".into(), "Run search / preview result".into()),
                    ("x".into(), "Select result".into()),
                    ("Tab".into(), "Switch results and preview".into()),
                    ("j / k".into(), "Move through results or preview".into()),
                    ("Esc".into(), "Leave search or return to results".into()),
                ],
            }]
        }
        UiContext::SearchPreview => vec![HelpSection {
            title: "Search Preview".into(),
            entries: vec![
                ("j / k".into(), "Move through messages in the thread".into()),
                ("h / Esc".into(), "Return to results".into()),
                ("/".into(), "Edit query".into()),
                ("x".into(), "Select current message".into()),
                ("Tab".into(), "Switch results and preview".into()),
                ("A".into(), "Open attachments".into()),
                ("L".into(), "Open links".into()),
                ("R".into(), "Toggle reader mode".into()),
                (
                    "r / a / f / e".into(),
                    "Reply, reply all, forward, archive".into(),
                ),
            ],
        }],
        UiContext::RulesList | UiContext::RulesForm => vec![HelpSection {
            title: "Rules Page".into(),
            entries: vec![
                ("j / k".into(), "Move rules or form fields".into()),
                ("Enter".into(), "Open overview or save form".into()),
                ("n".into(), "New rule".into()),
                ("E".into(), "Edit rule".into()),
                ("D".into(), "Dry run".into()),
                ("H".into(), "History".into()),
            ],
        }],
        UiContext::Diagnostics => vec![HelpSection {
            title: "Diagnostics Page".into(),
            entries: vec![
                ("j / k".into(), "Change section".into()),
                ("Ctrl-d / Ctrl-u".into(), "Scroll details".into()),
                ("Enter / o".into(), "Toggle fullscreen".into()),
                ("d".into(), "Open selected section details".into()),
                ("r".into(), "Refresh".into()),
                ("b".into(), "Generate bug report".into()),
                ("L".into(), "Open logs".into()),
            ],
        }],
        UiContext::AccountsList | UiContext::AccountsForm => vec![HelpSection {
            title: "Accounts Page".into(),
            entries: vec![
                ("j / k".into(), "Move accounts or fields".into()),
                ("Enter".into(), "Edit selected account".into()),
                ("n".into(), "New account".into()),
                ("t".into(), "Test account".into()),
                ("d".into(), "Set default".into()),
                ("s".into(), "Save account form".into()),
            ],
        }],
    }
}

fn command_sections(context: UiContext) -> Vec<HelpSection> {
    let mut by_category: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for command in commands_for_context(context) {
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

fn render_sections(
    sections: &[HelpSection],
    theme: &crate::mxr_tui::theme::Theme,
) -> Vec<Line<'static>> {
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
    use crate::mxr_tui::action::UiContext;

    #[test]
    fn help_sections_cover_accounts_and_commands() {
        let state = HelpModalState {
            open: true,
            ui_context: UiContext::AccountsList,
            selected_count: 2,
            scroll_offset: 0,
            _marker: std::marker::PhantomData,
        };
        let titles: Vec<String> = help_sections(&state)
            .into_iter()
            .map(|section| section.title)
            .collect();
        assert!(titles.contains(&"Accounts Page".to_string()));
        assert!(titles
            .iter()
            .any(|title| title.starts_with("Commands: Accounts")));
        assert!(!titles
            .iter()
            .any(|title| title.starts_with("Commands: Mail")));
    }
}
