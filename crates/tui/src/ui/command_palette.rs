use crate::action::Action;
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
}

impl Default for CommandPalette {
    fn default() -> Self {
        let commands = default_commands();
        let filtered: Vec<usize> = (0..commands.len()).collect();
        Self {
            visible: false,
            input: String::new(),
            commands,
            filtered,
            selected: 0,
        }
    }
}

impl CommandPalette {
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.input.clear();
            self.selected = 0;
            self.update_filtered();
        }
    }

    pub fn on_char(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
        self.update_filtered();
    }

    pub fn on_backspace(&mut self) {
        self.input.pop();
        self.selected = 0;
        self.update_filtered();
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

    pub fn update_filtered(&mut self) {
        let query = self.input.to_lowercase();
        self.filtered = self
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| {
                if query.is_empty() {
                    return true;
                }
                cmd.label.to_lowercase().contains(&query)
                    || cmd.shortcut.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
    }
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
            label: "Open In Browser".into(),
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
            label: "Search".into(),
            shortcut: "/".into(),
            action: Action::OpenSearch,
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
            label: "Quit".into(),
            shortcut: "q".into(),
            action: Action::QuitView,
            category: "Navigation".into(),
        },
    ]
}

pub fn draw(frame: &mut Frame, area: Rect, palette: &CommandPalette) {
    if !palette.visible {
        return;
    }

    // Center overlay: 60% width, up to 50% height
    let width = (area.width as u32 * 60 / 100).min(80) as u16;
    let height = (palette.filtered.len() as u16 + 4)
        .min(area.height * 50 / 100)
        .max(6);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    // Clear background
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Command Palette ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 2 {
        return;
    }

    // Input line
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let input_line =
        Paragraph::new(format!("> {}", palette.input)).style(Style::default().fg(Color::White));
    frame.render_widget(input_line, input_area);

    // Results
    let list_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );

    let visible_len = list_area.height as usize;
    let start = palette
        .selected
        .saturating_sub(visible_len.saturating_sub(1));
    let items: Vec<ListItem> = palette
        .filtered
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_len)
        .map(|(i, &cmd_idx)| {
            let cmd = &palette.commands[cmd_idx];
            let shortcut = if cmd.shortcut.is_empty() {
                String::new()
            } else {
                format!(" [{}]", cmd.shortcut)
            };
            let style = if i + start == palette.selected {
                Style::default().bg(Color::DarkGray).bold()
            } else {
                Style::default()
            };
            ListItem::new(format!("  {:<12} {}{}", cmd.category, cmd.label, shortcut)).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, list_area);

    let mut scrollbar_state =
        ScrollbarState::new(palette.filtered.len().saturating_sub(visible_len)).position(start);
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(Color::Yellow)),
        list_area,
        &mut scrollbar_state,
    );
}
