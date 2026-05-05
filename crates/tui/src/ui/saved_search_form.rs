use crate::app::{SavedSearchFormField, SavedSearchFormState};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    form: Option<&SavedSearchFormState>,
    theme: &crate::theme::Theme,
) {
    let Some(form) = form else {
        return;
    };

    let popup = centered_rect(70, 36, area);
    frame.render_widget(Clear, popup);

    let title = if form.existing_name.is_some() {
        " Edit Saved Search "
    } else {
        " New Saved Search "
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
            Constraint::Length(1), // Name
            Constraint::Length(1), // Query
            Constraint::Length(1), // Mode
            Constraint::Length(1), // blank
            Constraint::Length(2), // validation error (1 line + 1 spacing)
            Constraint::Min(0),    // help hint
            Constraint::Length(1), // footer
        ])
        .split(inner);

    frame.render_widget(
        field_line(
            "Name",
            &form.name,
            form.active_field == SavedSearchFormField::Name,
            theme,
        ),
        chunks[0],
    );
    frame.render_widget(
        field_line(
            "Query",
            &form.query,
            form.active_field == SavedSearchFormField::Query,
            theme,
        ),
        chunks[1],
    );
    frame.render_widget(
        field_line(
            "Mode",
            search_mode_label(form.search_mode),
            form.active_field == SavedSearchFormField::Mode,
            theme,
        ),
        chunks[2],
    );

    if let Some(error) = form.validation_error.as_ref() {
        frame.render_widget(
            Paragraph::new(error.clone()).style(Style::default().fg(theme.error)),
            chunks[4],
        );
    }

    frame.render_widget(
        Paragraph::new("Use saved searches to recall any query as a sidebar pin.")
            .style(Style::default().fg(theme.text_muted))
            .wrap(Wrap { trim: false }),
        chunks[5],
    );

    let footer = "[Tab] next field   [Space] cycle mode   [Enter] save   [Esc] cancel";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme.text_secondary)),
        chunks[6],
    );
}

fn field_line<'a>(
    label: &'a str,
    value: &'a str,
    active: bool,
    theme: &crate::theme::Theme,
) -> Paragraph<'a> {
    let label_span = Span::styled(
        format!("{label:<7}"),
        if active {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_muted)
        },
    );
    let active_marker = if active { "› " } else { "  " };
    let marker_span = Span::styled(
        active_marker,
        if active {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.text_muted)
        },
    );
    let value_span = Span::styled(value, Style::default().fg(theme.text_primary));
    Paragraph::new(Line::from(vec![marker_span, label_span, value_span]))
}

pub fn draw_delete_confirm(
    frame: &mut Frame,
    area: Rect,
    name: Option<&str>,
    theme: &crate::theme::Theme,
) {
    let Some(name) = name else {
        return;
    };

    let popup = centered_rect(60, 22, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Delete Saved Search ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.error))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("Delete saved search '{name}'?"))
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new("[y / Enter] confirm   [n / Esc] cancel")
            .style(Style::default().fg(theme.text_secondary)),
        chunks[1],
    );
}

fn search_mode_label(mode: mxr_core::types::SearchMode) -> &'static str {
    match mode {
        mxr_core::types::SearchMode::Lexical => "lexical",
        mxr_core::types::SearchMode::Semantic => "semantic",
        mxr_core::types::SearchMode::Hybrid => "hybrid",
    }
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
    use super::*;
    use crate::app::SavedSearchFormState;
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn theme() -> crate::theme::Theme {
        crate::theme::Theme::default()
    }

    #[test]
    fn renders_new_form_with_name_query_mode_fields() {
        let form = SavedSearchFormState::for_new();
        let rendered = render_to_string(90, 20, |frame| {
            draw(frame, Rect::new(0, 0, 90, 20), Some(&form), &theme());
        });
        assert!(
            rendered.contains("New Saved Search"),
            "expected `New Saved Search` title; got:\n{rendered}"
        );
        assert!(rendered.contains("Name"));
        assert!(rendered.contains("Query"));
        assert!(rendered.contains("Mode"));
        assert!(rendered.contains("lexical"));
    }

    #[test]
    fn edit_form_shows_edit_title_and_prefilled_values() {
        let form = SavedSearchFormState::for_edit(
            "Important".into(),
            "label:starred".into(),
            mxr_core::types::SearchMode::Hybrid,
        );
        let rendered = render_to_string(90, 20, |frame| {
            draw(frame, Rect::new(0, 0, 90, 20), Some(&form), &theme());
        });
        assert!(
            rendered.contains("Edit Saved Search"),
            "expected `Edit Saved Search` title; got:\n{rendered}"
        );
        assert!(rendered.contains("Important"));
        assert!(rendered.contains("label:starred"));
        assert!(rendered.contains("hybrid"));
    }

    #[test]
    fn validation_error_renders_when_set() {
        let mut form = SavedSearchFormState::for_new();
        form.validation_error = Some("Saved search name is required".into());
        let rendered = render_to_string(90, 20, |frame| {
            draw(frame, Rect::new(0, 0, 90, 20), Some(&form), &theme());
        });
        assert!(
            rendered.contains("Saved search name is required"),
            "validation error should render verbatim; got:\n{rendered}"
        );
    }

    #[test]
    fn footer_contains_tab_enter_esc_hints() {
        let form = SavedSearchFormState::for_new();
        let rendered = render_to_string(90, 20, |frame| {
            draw(frame, Rect::new(0, 0, 90, 20), Some(&form), &theme());
        });
        assert!(rendered.contains("Tab"));
        assert!(rendered.contains("Enter"));
        assert!(rendered.contains("Esc"));
    }

    #[test]
    fn delete_confirm_renders_name_and_keybindings() {
        let rendered = render_to_string(90, 20, |frame| {
            draw_delete_confirm(frame, Rect::new(0, 0, 90, 20), Some("Important"), &theme());
        });
        assert!(
            rendered.contains("Delete saved search 'Important'?"),
            "expected confirmation prompt with name; got:\n{rendered}"
        );
        assert!(rendered.contains("[y"));
        assert!(rendered.contains("[n"));
    }

    #[test]
    fn delete_confirm_no_render_when_name_is_none() {
        let rendered = render_to_string(90, 20, |frame| {
            draw_delete_confirm(frame, Rect::new(0, 0, 90, 20), None, &theme());
        });
        assert!(!rendered.contains("Delete saved search"));
    }

    #[test]
    fn no_render_when_form_is_none() {
        let rendered = render_to_string(90, 20, |frame| {
            draw(frame, Rect::new(0, 0, 90, 20), None, &theme());
        });
        // Should be all whitespace — no modal title, no field labels.
        assert!(!rendered.contains("Saved Search"));
        assert!(!rendered.contains("Name"));
        assert!(!rendered.contains("Query"));
    }
}
