//! The `b` prompt: reply later at a time, with the live time preview every
//! TUI time field shares.

use super::centered_rect;
use crate::app::ReplyLaterPromptState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    prompt: Option<&ReplyLaterPromptState>,
    theme: &crate::theme::Theme,
) {
    let Some(prompt) = prompt else {
        return;
    };
    let popup = centered_rect(50, 40, area);
    frame.render_widget(Clear, popup);
    let title = if prompt.waiting {
        " Bring back if nobody replies "
    } else {
        " Reply later "
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(inner);

    let lead = if prompt.waiting {
        "Off Waiting on until then; back if nobody has replied."
    } else {
        "Off the desk until then; back in You owe and the reply queue."
    };
    frame.render_widget(
        Paragraph::new(lead)
            .style(Style::default().fg(theme.text_secondary))
            .wrap(Wrap { trim: true }),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(format!(" › {}_", prompt.input))
            .style(Style::default().fg(theme.text_primary)),
        chunks[1],
    );

    let (lines, style) = if let Some(error) = &prompt.error {
        (vec![error.clone()], Style::default().fg(theme.error))
    } else if prompt.input.trim().is_empty() {
        let hint = if prompt.waiting {
            "Type a time: in 3d · fri 3 · next week"
        } else {
            "Type a time: tue 9 · tomorrow · in 2d. Enter with none: the reply queue now."
        };
        (
            vec![hint.to_string()],
            Style::default().fg(theme.text_secondary),
        )
    } else {
        let colour = if prompt.time.is_resolved() {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        (prompt.time.lines(), Style::default().fg(colour))
    };
    frame.render_widget(
        Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
            .style(style)
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new("Enter set  Tab other reading  Esc cancel")
            .style(Style::default().fg(theme.text_secondary)),
        chunks[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_config::SnoozeConfig;
    use mxr_core::id::{MessageId, ThreadId};
    use mxr_test_support::render_to_string;

    fn render(prompt: &ReplyLaterPromptState) -> String {
        render_to_string(90, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 24),
                Some(prompt),
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn the_prompt_previews_the_typed_time_and_names_what_happens() {
        let mut prompt = ReplyLaterPromptState::new(ThreadId::new(), MessageId::new(), false);
        let empty = render(&prompt);
        assert!(empty.contains("Reply later"));
        assert!(empty.contains("reply queue now"));

        prompt.input = "in 2d".into();
        prompt.time.update(&prompt.input, &SnoozeConfig::default());
        let typed = render(&prompt);
        assert!(typed.contains("→ "), "the resolved time shows:\n{typed}");
        assert!(typed.contains("in 2 days"), "{typed}");
    }

    #[test]
    fn on_a_waiting_row_it_says_it_comes_back_if_nobody_replies() {
        let prompt = ReplyLaterPromptState::new(ThreadId::new(), MessageId::new(), true);
        let out = render(&prompt);
        assert!(out.contains("Bring back if nobody replies"));
        assert!(out.contains("in 3d"));
    }
}
