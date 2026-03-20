use ratatui::prelude::*;
use ratatui::widgets::*;

/// A contact entry for autocomplete.
#[derive(Debug, Clone)]
pub struct Contact {
    pub name: String,
    pub email: String,
}

impl Contact {
    pub fn display(&self) -> String {
        if self.name.is_empty() {
            self.email.clone()
        } else {
            format!("{} <{}>", self.name, self.email)
        }
    }
}

#[derive(Default)]
pub struct ComposePicker {
    pub visible: bool,
    pub input: String,
    pub contacts: Vec<Contact>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    /// Already-chosen recipients.
    pub recipients: Vec<String>,
}

impl ComposePicker {
    pub fn open(&mut self, contacts: Vec<Contact>) {
        self.visible = true;
        self.input.clear();
        self.selected = 0;
        self.recipients.clear();
        self.contacts = contacts;
        self.update_filtered();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.input.clear();
        self.contacts.clear();
        self.filtered.clear();
        self.recipients.clear();
    }

    pub fn on_char(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
        self.update_filtered();
    }

    pub fn on_backspace(&mut self) {
        if self.input.is_empty() {
            // Remove last recipient on backspace with empty input
            self.recipients.pop();
        } else {
            self.input.pop();
            self.selected = 0;
            self.update_filtered();
        }
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

    /// Add the selected contact (or raw input) to recipients.
    /// Returns true if added, false if nothing to add.
    pub fn add_recipient(&mut self) -> bool {
        let email = if let Some(&idx) = self.filtered.get(self.selected) {
            self.contacts[idx].email.clone()
        } else if !self.input.is_empty() {
            // Use raw input as email if no match selected
            self.input.clone()
        } else {
            return false;
        };

        if !email.is_empty() && !self.recipients.contains(&email) {
            self.recipients.push(email);
        }
        self.input.clear();
        self.selected = 0;
        self.update_filtered();
        true
    }

    /// Confirm all recipients. Returns the comma-separated recipient string.
    /// Returns empty string if no recipients (user will fill in editor).
    pub fn confirm(&mut self) -> String {
        // Add any remaining input as a recipient
        if !self.input.is_empty() {
            self.add_recipient();
        }
        let result = self.recipients.join(", ");
        self.close();
        result
    }

    fn update_filtered(&mut self) {
        let query = self.input.to_lowercase();
        self.filtered = self
            .contacts
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                // Exclude already-selected recipients
                if self.recipients.contains(&c.email) {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                c.name.to_lowercase().contains(&query) || c.email.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
    }
}

pub fn draw(frame: &mut Frame, area: Rect, picker: &ComposePicker, theme: &crate::theme::Theme) {
    if !picker.visible {
        return;
    }

    let width = (area.width as u32 * 60 / 100).min(70) as u16;
    let height = (picker.filtered.len() as u16 + 6)
        .min(area.height * 60 / 100)
        .max(8);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Compose — To: (Tab to add, Enter to compose) ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 3 {
        return;
    }

    // Recipients line
    let recipients_area = Rect::new(inner.x, inner.y, inner.width, 1);
    if picker.recipients.is_empty() {
        frame.render_widget(
            Paragraph::new("").style(Style::default().fg(theme.text_muted)),
            recipients_area,
        );
    } else {
        let chips: Vec<Span> = picker
            .recipients
            .iter()
            .flat_map(|r| {
                vec![
                    Span::styled(
                        format!(" {} ", r),
                        Style::default().bg(theme.selection_bg).fg(theme.text_primary),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect();
        frame.render_widget(Paragraph::new(Line::from(chips)), recipients_area);
    }

    // Input line
    let input_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let input_line =
        Paragraph::new(format!("> {}", picker.input)).style(Style::default().fg(theme.text_primary));
    frame.render_widget(input_line, input_area);

    // Contact suggestions
    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );

    let items: Vec<ListItem> = picker
        .filtered
        .iter()
        .enumerate()
        .take(list_area.height as usize)
        .map(|(i, &idx)| {
            let contact = &picker.contacts[idx];
            let display = contact.display();
            let style = if i == picker.selected {
                theme.highlight_style()
            } else {
                Style::default()
            };
            ListItem::new(format!("  {}", display)).style(style)
        })
        .collect();

    frame.render_widget(List::new(items), list_area);
}
