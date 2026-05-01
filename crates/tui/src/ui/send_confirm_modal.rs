use crate::app::{PendingSend, PendingSendMode};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    pending: Option<&PendingSend>,
    theme: &crate::theme::Theme,
) {
    let Some(pending) = pending else {
        return;
    };

    let popup = centered_rect(86, 30, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Draft Ready ")
        .border_type(BorderType::Rounded)
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
    let mut lines = vec![match pending.mode {
        PendingSendMode::SendOrSave => "Send this draft?".to_string(),
        PendingSendMode::DraftOnlyNoRecipients => "No recipients yet. Save as draft?".to_string(),
        PendingSendMode::Unchanged => "Draft unchanged. Discard or keep editing?".to_string(),
    }];

    lines.push(format!("Subject: {}", pending.fm.subject));
    lines.push(String::new());
    match pending.mode {
        PendingSendMode::SendOrSave => {
            lines.push("[s] send   [d] save draft   [e] edit again   [Esc] discard".to_string());
        }
        PendingSendMode::DraftOnlyNoRecipients => {
            lines.push("[d] save draft   [e] edit again   [Esc] discard".to_string());
        }
        PendingSendMode::Unchanged => {
            lines.push("[e] edit again   [Esc] discard".to_string());
        }
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
    use super::draw;
    use crate::app::{PendingSend, PendingSendMode};
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn pending(mode: PendingSendMode) -> PendingSend {
        PendingSend {
            account_id: mxr_core::AccountId::new(),
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
            mode,
        }
    }

    #[test]
    fn send_or_save_modal_renders_full_action_row() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::SendOrSave)),
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Send this draft?"));
        assert!(rendered.contains("Subject: Hello"));
        assert!(rendered.contains("[s] send   [d] save draft   [e] edit again   [Esc] discard"));
    }

    #[test]
    fn missing_recipient_modal_renders_draft_only_actions() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::DraftOnlyNoRecipients)),
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("No recipients yet. Save as draft?"));
        assert!(rendered.contains("[d] save draft   [e] edit again   [Esc] discard"));
        assert!(!rendered.contains("[s] send"));
    }

    #[test]
    fn unchanged_modal_renders_without_send_or_save_actions() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 20),
                Some(&pending(PendingSendMode::Unchanged)),
                &crate::theme::Theme::default(),
            );
        });

        assert!(rendered.contains("Draft unchanged. Discard or keep editing?"));
        assert!(rendered.contains("[e] edit again   [Esc] discard"));
        assert!(!rendered.contains("[s] send"));
        assert!(!rendered.contains("[d] save draft"));
    }
}
