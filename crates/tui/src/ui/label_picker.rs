use crate::mxr_core::types::Label;
use nucleo::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32Str};
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
        self.filtered = filtered_indices(&self.labels, &self.input);
    }
}

fn filtered_indices(labels: &[Label], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..labels.len()).collect();
    }

    let query_lower = query.to_lowercase();
    let pattern = Pattern::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut utf32_buf = Vec::new();
    let mut ranked: Vec<_> = labels
        .iter()
        .enumerate()
        .filter_map(|(index, label)| {
            let display = humanize_label(&label.name);
            let search_text = format!("{display} {}", label.name);
            let score = pattern.score(Utf32Str::new(&search_text, &mut utf32_buf), &mut matcher)?;
            Some((
                index,
                label_match_priority(display, &label.name, &query_lower),
                score,
            ))
        })
        .collect();

    ranked.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.0.cmp(&right.0))
    });

    ranked.into_iter().map(|(index, _, _)| index).collect()
}

fn label_match_priority(display: &str, raw: &str, query_lower: &str) -> u8 {
    if starts_with_query(display, query_lower) || starts_with_query(raw, query_lower) {
        0
    } else if has_word_prefix(display, query_lower) || has_word_prefix(raw, query_lower) {
        1
    } else {
        2
    }
}

fn starts_with_query(haystack: &str, query_lower: &str) -> bool {
    haystack.to_lowercase().starts_with(query_lower)
}

fn has_word_prefix(haystack: &str, query_lower: &str) -> bool {
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| !word.is_empty() && word.to_lowercase().starts_with(query_lower))
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    picker: &LabelPicker,
    theme: &crate::mxr_tui::theme::Theme,
) {
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
        .border_style(Style::default().fg(theme.success));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.height < 2 {
        return;
    }

    // Input line
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let input_line = Paragraph::new(format!("> {}", picker.input))
        .style(Style::default().fg(theme.text_primary));
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
                theme.highlight_style()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_core::id::{AccountId, LabelId};
    use crate::mxr_core::types::LabelKind;

    fn label(name: &str, kind: LabelKind) -> Label {
        Label {
            id: LabelId::from_provider_id("test", name),
            account_id: AccountId::new(),
            name: name.into(),
            kind,
            color: None,
            provider_id: name.into(),
            unread_count: 0,
            total_count: 0,
        }
    }

    #[test]
    fn label_picker_prefers_prefix_matches_over_interior_matches() {
        let mut picker = LabelPicker::default();
        picker.open(
            vec![
                label("DRAFT", LabelKind::System),
                label("Follow Up", LabelKind::User),
            ],
            LabelPickerMode::Apply,
        );

        picker.on_char('f');

        let ranked: Vec<&str> = picker
            .filtered
            .iter()
            .map(|&index| humanize_label(&picker.labels[index].name))
            .collect();
        assert_eq!(ranked, vec!["Follow Up", "Drafts"]);
    }
}
