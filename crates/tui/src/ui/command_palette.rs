use crate::mxr_tui::action::{action_allowed_in_context, Action, UiContext};
use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub label: String,
    pub shortcut: String,
    pub action: Action,
    pub category: String,
}

pub struct CommandPalette {
    pub visible: bool,
    pub input: String,
    pub commands: Vec<PaletteCommand>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub context: UiContext,
}

impl Default for CommandPalette {
    fn default() -> Self {
        let commands = default_commands();
        let context = UiContext::MailboxList;
        let filtered = filtered_indices(&commands, context, "");
        Self {
            visible: false,
            input: String::new(),
            commands,
            filtered,
            selected: 0,
            context,
        }
    }
}

impl CommandPalette {
    pub fn toggle(&mut self, context: UiContext) {
        self.visible = !self.visible;
        if self.visible {
            self.context = context;
            self.input.clear();
            self.selected = 0;
            self.update_filtered(None);
        }
    }

    pub fn on_char(&mut self, c: char) {
        let preferred = self.selected_action().cloned();
        self.input.push(c);
        self.update_filtered(preferred);
    }

    pub fn on_backspace(&mut self) {
        let preferred = self.selected_action().cloned();
        self.input.pop();
        self.update_filtered(preferred);
    }

    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    pub fn confirm(&mut self) -> Option<Action> {
        if let Some(&idx) = self.filtered.get(self.selected) {
            self.visible = false;
            Some(self.commands[idx].action.clone())
        } else {
            None
        }
    }

    pub fn visible_commands(&self) -> impl Iterator<Item = &PaletteCommand> {
        self.filtered
            .iter()
            .filter_map(|&index| self.commands.get(index))
    }

    fn selected_action(&self) -> Option<&Action> {
        self.filtered
            .get(self.selected)
            .and_then(|&index| self.commands.get(index))
            .map(|command| &command.action)
    }

    fn update_filtered(&mut self, preferred_action: Option<Action>) {
        self.filtered = filtered_indices(&self.commands, self.context, &self.input);
        if let Some(action) = preferred_action {
            if let Some(index) = self
                .filtered
                .iter()
                .position(|&command_index| self.commands[command_index].action == action)
            {
                self.selected = index;
                return;
            }
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }
}

pub fn commands_for_context(context: UiContext) -> Vec<PaletteCommand> {
    default_commands()
        .into_iter()
        .filter(|command| action_allowed_in_context(&command.action, context))
        .collect()
}

fn filtered_indices(commands: &[PaletteCommand], context: UiContext, input: &str) -> Vec<usize> {
    let query = input.to_lowercase();
    commands
        .iter()
        .enumerate()
        .filter(|(_, command)| {
            action_allowed_in_context(&command.action, context)
                && (query.is_empty()
                    || command.label.to_lowercase().contains(&query)
                    || command.shortcut.to_lowercase().contains(&query)
                    || command.category.to_lowercase().contains(&query))
        })
        .map(|(index, _)| index)
        .collect()
}

pub fn default_commands() -> Vec<PaletteCommand> {
    vec![
        PaletteCommand {
            label: "Compose".into(),
            shortcut: "c".into(),
            action: Action::Compose,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Reply".into(),
            shortcut: "r".into(),
            action: Action::Reply,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Reply All".into(),
            shortcut: "a".into(),
            action: Action::ReplyAll,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Forward".into(),
            shortcut: "f".into(),
            action: Action::Forward,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Archive".into(),
            shortcut: "e".into(),
            action: Action::Archive,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Mark Read and Archive".into(),
            shortcut: "".into(),
            action: Action::MarkReadAndArchive,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Delete".into(),
            shortcut: "#".into(),
            action: Action::Trash,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Mark Spam".into(),
            shortcut: "!".into(),
            action: Action::Spam,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Star / Unstar".into(),
            shortcut: "s".into(),
            action: Action::Star,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Mark Read".into(),
            shortcut: "I".into(),
            action: Action::MarkRead,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Mark Unread".into(),
            shortcut: "U".into(),
            action: Action::MarkUnread,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Apply Label".into(),
            shortcut: "l".into(),
            action: Action::ApplyLabel,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Move To Label".into(),
            shortcut: "v".into(),
            action: Action::MoveToLabel,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Snooze".into(),
            shortcut: "Z".into(),
            action: Action::Snooze,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Unsubscribe".into(),
            shortcut: "D".into(),
            action: Action::Unsubscribe,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Attachments".into(),
            shortcut: "A".into(),
            action: Action::AttachmentList,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Open Links".into(),
            shortcut: "L".into(),
            action: Action::OpenLinks,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Open Message Source".into(),
            shortcut: "O".into(),
            action: Action::OpenInBrowser,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Toggle Reader Mode".into(),
            shortcut: "R".into(),
            action: Action::ToggleReaderMode,
            category: "View".into(),
        },
        PaletteCommand {
            label: "Toggle HTML View".into(),
            shortcut: "H".into(),
            action: Action::ToggleHtmlView,
            category: "View".into(),
        },
        PaletteCommand {
            label: "Toggle Remote Content".into(),
            shortcut: "M".into(),
            action: Action::ToggleRemoteContent,
            category: "View".into(),
        },
        PaletteCommand {
            label: "Export Thread".into(),
            shortcut: "E".into(),
            action: Action::ExportThread,
            category: "Mail".into(),
        },
        PaletteCommand {
            label: "Clear Selection".into(),
            shortcut: "Esc".into(),
            action: Action::ClearSelection,
            category: "Selection".into(),
        },
        PaletteCommand {
            label: "Toggle Select".into(),
            shortcut: "x".into(),
            action: Action::ToggleSelect,
            category: "Selection".into(),
        },
        PaletteCommand {
            label: "Visual Select".into(),
            shortcut: "V".into(),
            action: Action::VisualLineMode,
            category: "Selection".into(),
        },
        PaletteCommand {
            label: "Go to Inbox".into(),
            shortcut: "gi".into(),
            action: Action::GoToInbox,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Go to Starred".into(),
            shortcut: "gs".into(),
            action: Action::GoToStarred,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Go to Sent".into(),
            shortcut: "gt".into(),
            action: Action::GoToSent,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Go to Drafts".into(),
            shortcut: "gd".into(),
            action: Action::GoToDrafts,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Go to All Mail".into(),
            shortcut: "ga".into(),
            action: Action::GoToAllMail,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Edit Config".into(),
            shortcut: "gc".into(),
            action: Action::EditConfig,
            category: "System".into(),
        },
        PaletteCommand {
            label: "Open Logs".into(),
            shortcut: "gL".into(),
            action: Action::OpenLogs,
            category: "System".into(),
        },
        PaletteCommand {
            label: "Search All Mail".into(),
            shortcut: "/".into(),
            action: Action::OpenGlobalSearch,
            category: "Search".into(),
        },
        PaletteCommand {
            label: "Filter Current Mailbox".into(),
            shortcut: "Ctrl-f".into(),
            action: Action::OpenMailboxFilter,
            category: "Search".into(),
        },
        PaletteCommand {
            label: "Switch Pane".into(),
            shortcut: "Tab".into(),
            action: Action::SwitchPane,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Open Mailbox".into(),
            shortcut: "".into(),
            action: Action::OpenMailboxScreen,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Open Search Page".into(),
            shortcut: "".into(),
            action: Action::OpenSearchScreen,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Open Rules Page".into(),
            shortcut: "".into(),
            action: Action::OpenRulesScreen,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Open Diagnostics Page".into(),
            shortcut: "".into(),
            action: Action::OpenDiagnosticsScreen,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Open Accounts Page".into(),
            shortcut: "".into(),
            action: Action::OpenAccountsScreen,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Refresh Rules".into(),
            shortcut: "".into(),
            action: Action::RefreshRules,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "New Rule".into(),
            shortcut: "".into(),
            action: Action::OpenRuleFormNew,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "Edit Rule".into(),
            shortcut: "".into(),
            action: Action::OpenRuleFormEdit,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "Toggle Rule Enabled".into(),
            shortcut: "".into(),
            action: Action::ToggleRuleEnabled,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "Rule Dry Run".into(),
            shortcut: "".into(),
            action: Action::ShowRuleDryRun,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "Rule History".into(),
            shortcut: "".into(),
            action: Action::ShowRuleHistory,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "Delete Rule".into(),
            shortcut: "".into(),
            action: Action::DeleteRule,
            category: "Rules".into(),
        },
        PaletteCommand {
            label: "Refresh Diagnostics".into(),
            shortcut: "".into(),
            action: Action::RefreshDiagnostics,
            category: "Diagnostics".into(),
        },
        PaletteCommand {
            label: "Generate Bug Report".into(),
            shortcut: "".into(),
            action: Action::GenerateBugReport,
            category: "Diagnostics".into(),
        },
        PaletteCommand {
            label: "Open Diagnostics Details".into(),
            shortcut: "d".into(),
            action: Action::OpenDiagnosticsPaneDetails,
            category: "Diagnostics".into(),
        },
        PaletteCommand {
            label: "Refresh Accounts".into(),
            shortcut: "".into(),
            action: Action::RefreshAccounts,
            category: "Accounts".into(),
        },
        PaletteCommand {
            label: "New IMAP/SMTP Account".into(),
            shortcut: "".into(),
            action: Action::OpenAccountFormNew,
            category: "Accounts".into(),
        },
        PaletteCommand {
            label: "Test Account".into(),
            shortcut: "".into(),
            action: Action::TestAccountForm,
            category: "Accounts".into(),
        },
        PaletteCommand {
            label: "Set Default Account".into(),
            shortcut: "".into(),
            action: Action::SetDefaultAccount,
            category: "Accounts".into(),
        },
        PaletteCommand {
            label: "Toggle Thread/Message List".into(),
            shortcut: "".into(),
            action: Action::ToggleMailListMode,
            category: "View".into(),
        },
        PaletteCommand {
            label: "Toggle Fullscreen".into(),
            shortcut: "F".into(),
            action: Action::ToggleFullscreen,
            category: "View".into(),
        },
        PaletteCommand {
            label: "Sync now".into(),
            shortcut: "".into(),
            action: Action::SyncNow,
            category: "Sync".into(),
        },
        PaletteCommand {
            label: "Help".into(),
            shortcut: "?".into(),
            action: Action::Help,
            category: "Navigation".into(),
        },
        PaletteCommand {
            label: "Start Here".into(),
            shortcut: "".into(),
            action: Action::ShowOnboarding,
            category: "System".into(),
        },
        PaletteCommand {
            label: "Quit".into(),
            shortcut: "q".into(),
            action: Action::QuitView,
            category: "Navigation".into(),
        },
    ]
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    palette: &CommandPalette,
    theme: &crate::mxr_tui::theme::Theme,
) {
    if !palette.visible {
        return;
    }

    let width = ((area.width as u32 * 62) / 100).clamp(56, 96) as u16;
    let width = width.min(area.width.saturating_sub(4)).max(1);
    let preferred_height = area.height.saturating_sub(6).clamp(14, 22);
    let height = preferred_height.min(area.height.saturating_sub(2).max(1));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = (area.y + 2).min(area.y + area.height.saturating_sub(height));
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Command Palette ")
        .title_style(Style::default().fg(theme.accent).bold())
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(inner);

    let selected_command = palette
        .filtered
        .get(palette.selected)
        .and_then(|&idx| palette.commands.get(idx));

    let query_text = if palette.input.is_empty() {
        "type a command or shortcut".to_string()
    } else {
        palette.input.clone()
    };
    let input_block = Block::bordered()
        .title(format!(
            " Query  {} matches  {} ",
            palette.filtered.len(),
            palette.context.label()
        ))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_unfocused))
        .style(Style::default().bg(theme.hint_bar_bg));
    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent).bold()),
        Span::styled(
            query_text,
            Style::default().fg(if palette.input.is_empty() {
                theme.text_muted
            } else {
                theme.text_primary
            }),
        ),
    ]))
    .block(input_block);
    frame.render_widget(input, chunks[0]);

    let list_area = chunks[1];

    let visible_len = list_area.height.saturating_sub(2) as usize;
    let start = if visible_len == 0 {
        0
    } else {
        palette
            .selected
            .saturating_sub(visible_len.saturating_sub(1) / 2)
    };
    let rows: Vec<Row> = palette
        .filtered
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_len)
        .map(|(i, &cmd_idx)| {
            let cmd = &palette.commands[cmd_idx];
            let style = if i + start == palette.selected {
                theme.highlight_style()
            } else {
                Style::default().fg(theme.text_secondary)
            };
            let (icon, category_color) = category_style(&cmd.category, theme);
            let shortcut = if cmd.shortcut.is_empty() {
                Span::styled("palette", Style::default().fg(theme.text_muted))
            } else {
                Span::styled(
                    cmd.shortcut.clone(),
                    Style::default().fg(theme.text_primary).bold(),
                )
            };
            Row::new(vec![
                Cell::from(Span::styled(
                    icon,
                    Style::default().fg(category_color).bold(),
                )),
                Cell::from(Line::from(vec![
                    Span::styled(
                        format!(" {} ", cmd.category),
                        Style::default().bg(category_color).fg(Color::Black).bold(),
                    ),
                    Span::raw(" "),
                    Span::styled(&cmd.label, Style::default().fg(theme.text_primary)),
                ])),
                Cell::from(shortcut),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(10),
        ],
    )
    .column_spacing(1)
    .block(
        Block::bordered()
            .title(" Commands ")
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused)),
    );
    frame.render_widget(table, list_area);

    let mut scrollbar_state =
        ScrollbarState::new(palette.filtered.len().saturating_sub(visible_len)).position(start);

    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.warning)),
        list_area,
        &mut scrollbar_state,
    );

    let footer_text = selected_command
        .map(|cmd| {
            let shortcut = if cmd.shortcut.is_empty() {
                "palette".to_string()
            } else {
                cmd.shortcut.clone()
            };
            Line::from(vec![
                Span::styled("enter ", Style::default().fg(theme.accent).bold()),
                Span::styled("run", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("↑↓ ", Style::default().fg(theme.accent).bold()),
                Span::styled("move", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("esc ", Style::default().fg(theme.accent).bold()),
                Span::styled("close", Style::default().fg(theme.text_secondary)),
                Span::raw("   "),
                Span::styled("selected ", Style::default().fg(theme.text_muted)),
                Span::styled(&cmd.label, Style::default().fg(theme.text_primary).bold()),
                Span::styled(" · ", Style::default().fg(theme.text_muted)),
                Span::styled(shortcut, Style::default().fg(theme.accent)),
                Span::styled(" · ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    palette.context.label(),
                    Style::default().fg(theme.text_muted),
                ),
            ])
        })
        .unwrap_or_else(|| {
            Line::from(Span::styled(
                "No matching commands",
                Style::default().fg(theme.text_muted),
            ))
        });
    let footer = Paragraph::new(footer_text).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_unfocused)),
    );
    frame.render_widget(footer, chunks[2]);
}

fn category_style(category: &str, theme: &crate::mxr_tui::theme::Theme) -> (&'static str, Color) {
    match category {
        "Mail" => ("@", theme.warning),
        "Navigation" => (">", theme.accent),
        "Search" => ("/", theme.link_fg),
        "Selection" => ("+", theme.success),
        "View" => ("~", theme.text_secondary),
        "Rules" => ("#", theme.error),
        "Diagnostics" => ("!", theme.warning),
        "Accounts" => ("=", theme.accent_dim),
        "Sync" => ("*", theme.success),
        _ => ("?", theme.text_muted),
    }
}

#[cfg(test)]
mod tests {
    use super::{commands_for_context, CommandPalette};
    use crate::mxr_tui::action::UiContext;

    #[test]
    fn rules_context_hides_mail_only_commands() {
        let labels: Vec<String> = commands_for_context(UiContext::RulesList)
            .into_iter()
            .map(|command| command.label)
            .collect();

        assert!(labels.contains(&"New Rule".to_string()));
        assert!(!labels.contains(&"Apply Label".to_string()));
        assert!(!labels.contains(&"Archive".to_string()));
    }

    #[test]
    fn filtering_preserves_selected_command_identity() {
        let mut palette = CommandPalette::default();
        palette.toggle(UiContext::MailboxList);
        palette.selected = palette
            .filtered
            .iter()
            .position(|&index| palette.commands[index].label == "Open Search Page")
            .unwrap();

        palette.on_char('o');
        palette.on_char('p');

        let selected = palette
            .filtered
            .get(palette.selected)
            .map(|&index| palette.commands[index].label.clone());

        assert_eq!(selected.as_deref(), Some("Open Search Page"));
    }
}
