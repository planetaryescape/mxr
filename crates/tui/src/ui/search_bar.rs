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

    pub fn activate_existing(&mut self) {
        self.active = true;
        self.cursor_pos = self.query.len();
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

pub fn draw(frame: &mut Frame, area: Rect, search_bar: &SearchBar, theme: &crate::theme::Theme) {
    if !search_bar.active {
        return;
    }

    let modal = centered_rect(area);
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .title(" Search ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent)),
        modal,
    );

    let inner = modal.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let query = if search_bar.query.is_empty() {
        Span::styled(
            "> Type to search mail",
            Style::default().fg(theme.text_muted),
        )
    } else {
        Span::styled(
            format!("> {}", search_bar.query),
            Style::default().fg(theme.text_primary),
        )
    };
    frame.render_widget(Paragraph::new(Line::from(query)), sections[0]);
    frame.render_widget(
        Paragraph::new("Enter submit  Esc cancel").style(Style::default().fg(theme.text_muted)),
        sections[1],
    );
}

fn centered_rect(area: Rect) -> Rect {
    let width = ((area.width as u32 * 3) / 5).clamp(36, 84) as u16;
    let height = 5u16;
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 3;
    Rect::new(x, y, width, height)
}
