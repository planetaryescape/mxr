use crate::app::PendingSend;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, pending: Option<&PendingSend>, theme: &crate::theme::Theme) {
    let Some(pending) = pending else {
        return;
    };

    let popup = centered_rect(58, 24, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Draft Ready ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = modal_lines(pending)
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn modal_lines(pending: &PendingSend) -> Vec<String> {
    let mut lines = vec![if pending.allow_send {
        "Send this draft?".to_string()
    } else {
        "Draft unchanged. Discard or keep editing?".to_string()
    }];

    lines.push(format!("Subject: {}", pending.fm.subject));
    lines.push(String::new());
    if pending.allow_send {
        lines.push("[s] send   [d] save draft   [e] edit again   [Esc] discard".to_string());
    } else {
        lines.push("[e] edit again   [Esc] discard".to_string());
    }
    lines
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::modal_lines;
    use crate::app::PendingSend;

    fn pending(allow_send: bool) -> PendingSend {
        PendingSend {
            fm: mxr_compose::frontmatter::ComposeFrontmatter {
                to: "a@example.com".into(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Hello".into(),
                from: "me@example.com".into(),
                in_reply_to: None,
                references: vec![],
                attach: vec![],
            },
            body: "hi".into(),
            draft_path: std::path::PathBuf::from("/tmp/draft.md"),
            allow_send,
        }
    }

    #[test]
    fn unchanged_modal_hides_send_actions() {
        let text = modal_lines(&pending(false)).join("\n");
        assert!(text.contains("Draft unchanged"));
        assert!(!text.contains("[s] send"));
        assert!(!text.contains("[d] save draft"));
    }

    #[test]
    fn changed_modal_shows_send_actions() {
        let text = modal_lines(&pending(true)).join("\n");
        assert!(text.contains("Send this draft?"));
        assert!(text.contains("[s] send"));
        assert!(text.contains("[d] save draft"));
    }
}
