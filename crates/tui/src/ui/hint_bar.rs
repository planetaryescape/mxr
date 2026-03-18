use crate::app::ActivePane;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, active_pane: &ActivePane, search_active: bool) {
    let hints = if search_active {
        "Enter:Confirm  Esc:Cancel"
    } else {
        match active_pane {
            ActivePane::MailList => "j/k:Nav  l:Open  h:Labels  /:Search  gi:Inbox  q:Quit",
            ActivePane::Sidebar => "j/k:Nav  l:Mail  Enter:Select  Esc:All  q:Quit",
            ActivePane::MessageView => "j/k:Scroll  h:Back  Esc:Close  q:Quit",
        }
    };

    let spans: Vec<Span> = hints
        .split("  ")
        .flat_map(|hint| {
            if let Some((key, action)) = hint.split_once(':') {
                vec![
                    Span::styled(format!(" {key}"), Style::default().fg(Color::White).bold()),
                    Span::styled(format!(":{action} "), Style::default().fg(Color::Gray)),
                ]
            } else {
                vec![Span::raw(hint.to_string())]
            }
        })
        .collect();

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Rgb(30, 30, 40))),
        area,
    );
}
