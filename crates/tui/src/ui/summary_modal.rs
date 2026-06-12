use super::centered_rect;
use crate::app::ThreadSummaryModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 70;
const MODAL_HEIGHT_PERCENT: u16 = 50;

pub fn draw(frame: &mut Frame, area: Rect, state: &ThreadSummaryModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = match &state.model {
        Some(model) => format!(" Thread summary · {model} · Esc close "),
        None => " Thread summary · Esc close ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area).inner(Margin::new(1, 1));
    frame.render_widget(block, modal_area);

    if let Some(error) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to summarize: {error}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Summarizing thread...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    if let Some(summary) = &state.summary {
        let paragraph = Paragraph::new(summary.clone())
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::ThreadId;
    use mxr_test_support::render_to_string;

    #[test]
    fn loading_state_shows_placeholder() {
        let mut state = ThreadSummaryModalState::default();
        state.open_loading(ThreadId::new());
        let snapshot = render_to_string(80, 18, |frame| {
            draw(frame, Rect::new(0, 0, 80, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Summarizing thread..."),
            "loading placeholder must appear; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_summary_text_and_model_name() {
        let mut state = ThreadSummaryModalState::default();
        state.open_loading(ThreadId::new());
        state.set_summary(
            "ACTION REQUIRED — confirm the launch checklist\n\nAlice asked Bob to confirm the launch checklist. He hasn't replied.".into(),
            "qwen2.5-3b-instruct".into(),
        );
        let snapshot = render_to_string(100, 18, |frame| {
            draw(frame, Rect::new(0, 0, 100, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("ACTION REQUIRED")
                && snapshot.contains("confirm the launch checklist"),
            "triage verdict first line must render without clipping; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Alice asked Bob"),
            "summary body text must render; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("qwen2.5-3b-instruct"),
            "model name must surface in title; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_disabled_error_message() {
        let mut state = ThreadSummaryModalState::default();
        state.open_loading(ThreadId::new());
        state.set_error("LLM disabled (set [llm] enabled = true in mxr.toml)".into());
        let snapshot = render_to_string(80, 18, |frame| {
            draw(frame, Rect::new(0, 0, 80, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Failed to summarize"),
            "error banner must appear; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("LLM disabled"),
            "underlying daemon error must surface verbatim; got:\n{snapshot}",
        );
    }
}
