use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Default)]
pub struct SearchBar {
    pub active: bool,
    pub query: String,
    pub cursor_pos: usize,
}

impl SearchBar {
    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.cursor_pos = 0;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn on_char(&mut self, c: char) {
        self.query.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn on_backspace(&mut self) {
        if self.cursor_pos > 0 {
            // Find the previous char boundary
            let prev = self.query[..self.cursor_pos]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.query.remove(prev);
            self.cursor_pos = prev;
        }
    }

    pub fn submit(&mut self) -> String {
        self.active = false;
        self.query.clone()
    }
}

pub fn draw(frame: &mut Frame, area: Rect, search_bar: &SearchBar) {
    if !search_bar.active {
        return;
    }

    let text = format!("Search: {}", search_bar.query);
    let bar = Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(bar, area);
}
