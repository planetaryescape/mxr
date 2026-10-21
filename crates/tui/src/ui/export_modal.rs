use super::centered_rect;
use crate::app::{ExportModalState, ExportStage};
use crate::export_output::{format_key, format_label, FORMAT_CHOICES};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &ExportModalState, theme: &crate::theme::Theme) {
    if !state.visible {
        return;
    }

    let popup = centered_rect(60, 50, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Export thread ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 4 {
        return;
    }

    let (body, footer) = match state.stage {
        ExportStage::Format => format_stage(state, theme),
        ExportStage::Destination => destination_stage(state, theme),
        ExportStage::Path => path_stage(state, theme),
    };

    let body_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );
    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), body_area);

    let footer_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme.text_muted)),
        footer_area,
    );
}

fn format_stage(
    state: &ExportModalState,
    theme: &crate::theme::Theme,
) -> (Vec<Line<'static>>, &'static str) {
    let mut lines = vec![Line::styled(
        "Format",
        Style::default().fg(theme.text_secondary),
    )];
    for (index, format) in FORMAT_CHOICES.iter().enumerate() {
        let selected = index == state.selected;
        let style = if selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let marker = if selected { "> " } else { "  " };
        lines.push(Line::styled(
            format!("{marker}{}  {}", format_key(format), format_label(format)),
            style,
        ));
    }
    (lines, "m j x l pick · ↑↓ Enter choose · Esc cancel")
}

fn destination_stage(
    state: &ExportModalState,
    theme: &crate::theme::Theme,
) -> (Vec<Line<'static>>, &'static str) {
    // The destination step is only reached after a format is chosen.
    let format = state.format.as_ref().map_or("", format_label);
    let lines = vec![
        Line::styled(
            format!("Format: {format}"),
            Style::default().fg(theme.text_secondary),
        ),
        Line::styled(
            "  c  Copy to clipboard",
            Style::default().fg(theme.text_primary),
        ),
        Line::styled(
            "  s  Save to a file",
            Style::default().fg(theme.text_primary),
        ),
    ];
    (lines, "c copy · s save · Esc cancel")
}

fn path_stage(
    state: &ExportModalState,
    theme: &crate::theme::Theme,
) -> (Vec<Line<'static>>, &'static str) {
    let mut lines = vec![
        Line::styled("Save to:", Style::default().fg(theme.text_secondary)),
        Line::styled(
            format!("> {}", state.input),
            Style::default().fg(theme.text_primary),
        ),
    ];
    if let Some(error) = &state.error {
        lines.push(Line::styled(
            error.clone(),
            Style::default().fg(theme.warning),
        ));
    } else if state.awaiting_overwrite_confirm {
        lines.push(Line::styled(
            "File exists — press Enter again to overwrite",
            Style::default().fg(theme.warning),
        ));
    }
    (lines, "Enter save · Esc cancel")
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::app::{ExportModalState, ExportStage};
    use crate::export_output::{format_extension, FORMAT_CHOICES};
    use mxr_core::id::ThreadId;
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn render(state: &ExportModalState) -> String {
        render_to_string(100, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 24),
                state,
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn format_stage_lists_all_four_formats_with_keys() {
        let mut state = ExportModalState::default();
        state.open(ThreadId::new());
        let snapshot = render(&state);
        for label in ["Markdown", "JSON", "Mbox", "LLM context"] {
            assert!(snapshot.contains(label), "snapshot:\n{snapshot}");
        }
        assert!(snapshot.contains("m j x l pick"), "snapshot:\n{snapshot}");
    }

    #[test]
    fn destination_stage_names_the_chosen_format() {
        let mut state = ExportModalState::default();
        state.open(ThreadId::new());
        state.format = Some(FORMAT_CHOICES[2].clone());
        state.stage = ExportStage::Destination;
        let snapshot = render(&state);
        assert!(snapshot.contains("Format: Mbox"), "snapshot:\n{snapshot}");
        assert!(
            snapshot.contains("Copy to clipboard"),
            "snapshot:\n{snapshot}"
        );
        assert_eq!(format_extension(&FORMAT_CHOICES[2]), "mbox");
    }

    #[test]
    fn path_stage_surfaces_error_and_overwrite_prompt() {
        let mut state = ExportModalState::default();
        state.open(ThreadId::new());
        state.stage = ExportStage::Path;
        state.input = "~/mxr/exports/mxr-thread-x.md".into();
        state.error = Some("Path cannot be empty".into());
        assert!(render(&state).contains("Path cannot be empty"));

        state.error = None;
        state.awaiting_overwrite_confirm = true;
        assert!(render(&state).contains("press Enter again to overwrite"));
    }
}
