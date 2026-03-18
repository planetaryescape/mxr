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
            label: "Search".into(),
            shortcut: "/".into(),
            action: Action::OpenSearch,
            category: "Search".into(),
        },
        PaletteCommand {
            label: "Sync now".into(),
            shortcut: "".into(),
            action: Action::SyncNow,
            category: "Sync".into(),
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

    let items: Vec<ListItem> = palette
        .filtered
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(i, &cmd_idx)| {
            let cmd = &palette.commands[cmd_idx];
            let shortcut = if cmd.shortcut.is_empty() {
                String::new()
            } else {
                format!(" [{}]", cmd.shortcut)
            };
            let style = if i == palette.selected {
                Style::default().bg(Color::DarkGray).bold()
            } else {
                Style::default()
            };
            ListItem::new(format!("  {}{}", cmd.label, shortcut)).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, list_area);
}
