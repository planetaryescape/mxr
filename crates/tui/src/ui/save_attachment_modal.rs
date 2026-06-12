use super::centered_rect;
use crate::app::SaveAttachmentModalState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &SaveAttachmentModalState,
    theme: &crate::theme::Theme,
) {
    if !state.visible {
        return;
    }

    let popup = centered_rect(70, 35, area);
    frame.render_widget(Clear, popup);

    let title = if state.filename.is_empty() {
        " Save Attachment ".to_string()
    } else {
        format!(" Save: {} ", state.filename)
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 4 {
        return;
    }

    let mut row = inner.y;

    let prompt = Paragraph::new("Save to:").style(Style::default().fg(theme.text_secondary));
    frame.render_widget(prompt, Rect::new(inner.x, row, inner.width, 1));
    row += 1;

    let input_area = Rect::new(inner.x, row, inner.width, 1);
    let input_line =
        Paragraph::new(format!("> {}", state.input)).style(Style::default().fg(theme.text_primary));
    frame.render_widget(input_line, input_area);
    row += 1;

    if let Some(error) = &state.error {
        let error_area = Rect::new(inner.x, row, inner.width, 1);
        frame.render_widget(
            Paragraph::new(error.clone()).style(Style::default().fg(theme.warning)),
            error_area,
        );
        row += 1;
    } else if state.awaiting_overwrite_confirm {
        let warn_area = Rect::new(inner.x, row, inner.width, 1);
        frame.render_widget(
            Paragraph::new("File exists — press Enter again to overwrite")
                .style(Style::default().fg(theme.warning)),
            warn_area,
        );
        row += 1;
    }

    let footer = "1 Downloads  2 Desktop  3 cwd  ·  Enter save  ·  Esc cancel";
    let footer_y = inner.y + inner.height.saturating_sub(1);
    if footer_y > row {
        let footer_area = Rect::new(inner.x, footer_y, inner.width, 1);
        frame.render_widget(
            Paragraph::new(footer).style(Style::default().fg(theme.text_muted)),
            footer_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::app::SaveAttachmentModalState;
    use mxr_core::id::{AttachmentId, MessageId};
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn opened_state() -> SaveAttachmentModalState {
        let mut state = SaveAttachmentModalState::default();
        state.open(
            MessageId::new(),
            AttachmentId::new(),
            "budget.xlsx".into(),
            "/Users/bhekanik/Downloads/budget.xlsx".into(),
        );
        state
    }

    #[test]
    fn modal_renders_prefilled_path_and_filename_in_title() {
        let state = opened_state();
        let snapshot = render_to_string(120, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 20),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(snapshot.contains("budget.xlsx"), "snapshot:\n{snapshot}");
        assert!(snapshot.contains("Downloads"), "snapshot:\n{snapshot}");
        assert!(snapshot.contains("Enter save"), "snapshot:\n{snapshot}");
    }

    #[test]
    fn modal_surfaces_overwrite_confirm() {
        let mut state = opened_state();
        state.awaiting_overwrite_confirm = true;
        let snapshot = render_to_string(120, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 20),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("press Enter again"),
            "snapshot:\n{snapshot}"
        );
    }

    #[test]
    fn modal_surfaces_error_message() {
        let mut state = opened_state();
        state.error = Some("parent dir not writable".into());
        let snapshot = render_to_string(120, 20, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 20),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            snapshot.contains("parent dir not writable"),
            "snapshot:\n{snapshot}"
        );
    }
}
