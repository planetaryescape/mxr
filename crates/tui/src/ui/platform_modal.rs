use crate::app::PlatformModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;
use super::centered_rect;

pub fn draw(frame: &mut Frame, area: Rect, state: &PlatformModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(76, 62, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = format!(" {} · Esc close ", state.title);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area).inner(Margin::new(1, 1));
    frame.render_widget(block, modal_area);

    if let Some(error) = &state.error {
        let paragraph = Paragraph::new(error.clone())
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
        return;
    }

    let text = state.body.as_deref().unwrap_or_default();
    let style = if state.loading {
        Style::default().fg(theme.text_muted)
    } else {
        Style::default().fg(theme.text_primary)
    };
    let paragraph = Paragraph::new(text.to_string())
        .style(style)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}


#[cfg(test)]
mod tests {
    use super::*;
    use mxr_test_support::render_to_string;

    #[test]
    fn renders_platform_text() {
        let mut state = PlatformModalState::default();
        state.open_loading("Voice profile", "Loading voice profile...");
        state.set_body("casual register: formality 0.30".into());

        let snapshot = render_to_string(90, 18, |frame| {
            draw(frame, Rect::new(0, 0, 90, 18), &state, &Theme::default());
        });

        assert!(snapshot.contains("Voice profile"), "got:\n{snapshot}");
        assert!(snapshot.contains("casual register"), "got:\n{snapshot}");
    }
}
