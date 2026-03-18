use mxr_core::types::Label;
use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelPickerMode {
    Apply,
    Move,
}

pub struct LabelPicker {
    pub visible: bool,
    pub input: String,
    pub labels: Vec<Label>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub mode: LabelPickerMode,
}

impl Default for LabelPicker {
    fn default() -> Self {
        Self {
            visible: false,
            input: String::new(),
            labels: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            mode: LabelPickerMode::Apply,
        }
    }
}

impl LabelPicker {
    pub fn open(&mut self, labels: Vec<Label>, mode: LabelPickerMode) {
        self.visible = true;
        self.input.clear();
        self.selected = 0;
        self.labels = labels;
        self.mode = mode;
        self.update_filtered();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.input.clear();
        self.labels.clear();
        self.filtered.clear();
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

    /// Returns the selected label's name, or None.
    pub fn confirm(&mut self) -> Option<String> {
        if let Some(&idx) = self.filtered.get(self.selected) {
            let name = self.labels[idx].name.clone();
            self.close();
            Some(name)
        } else {
            None
        }
    }

    fn update_filtered(&mut self) {
        let query = self.input.to_lowercase();
        self.filtered = self
            .labels
            .iter()
            .enumerate()
            .filter(|(_, label)| {
                if query.is_empty() {
                    return true;
                }
                label.name.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
    }
}

pub fn draw(frame: &mut Frame, area: Rect, picker: &LabelPicker) {
    if !picker.visible {
        return;
    }

    let title = match picker.mode {
        LabelPickerMode::Apply => " Apply Label ",
        LabelPickerMode::Move => " Move to Label ",
    };

    let width = (area.width as u32 * 50 / 100).min(60) as u16;
    let height = (picker.filtered.len() as u16 + 4)
        .min(area.height * 50 / 100)
        .max(6);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 2 {
        return;
    }

    // Input line
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let input_line =
        Paragraph::new(format!("> {}", picker.input)).style(Style::default().fg(Color::White));
    frame.render_widget(input_line, input_area);

    // Label list
    let list_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );

    let items: Vec<ListItem> = picker
        .filtered
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(i, &label_idx)| {
            let label = &picker.labels[label_idx];
            let display = humanize_label(&label.name);
            let style = if i == picker.selected {
                Style::default().bg(Color::DarkGray).bold()
            } else {
                Style::default()
            };
            ListItem::new(format!("  {}", display)).style(style)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, list_area);
}

fn humanize_label(name: &str) -> &str {
    match name {
        "INBOX" => "Inbox",
        "SENT" => "Sent",
        "DRAFT" => "Drafts",
        "TRASH" => "Trash",
        "SPAM" => "Spam",
        "STARRED" => "Starred",
        "IMPORTANT" => "Important",
        "UNREAD" => "Unread",
        other => other,
    }
}
