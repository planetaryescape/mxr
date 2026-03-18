use mxr_core::types::{Envelope, MessageFlags};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, envelopes: &[Envelope]) {
    let unread_count = envelopes
        .iter()
        .filter(|e| !e.flags.contains(MessageFlags::READ))
        .count();

    let status = format!(
        " [INBOX] {} unread | {} total",
        unread_count,
        envelopes.len(),
    );

    let bar = Paragraph::new(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));

    frame.render_widget(bar, area);
}
