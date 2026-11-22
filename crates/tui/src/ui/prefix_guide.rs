use crate::theme::Theme;
use crate::ui::help_keys::KeyRows;
use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Continuation {
    pub id: String,
    pub next: String,
    pub label: String,
}

pub(crate) fn continuations(
    keys: &KeyRows<'_>,
    places: &[&str],
    prefix: char,
) -> Vec<Continuation> {
    let mut found: Vec<Continuation> = Vec::new();
    for place in places {
        for action in &mxr_keymap::defaults().actions {
            for shown in keys.keys(action, place) {
                let Some(next) = shown.strip_prefix(prefix).filter(|next| !next.is_empty()) else {
                    continue;
                };
                if found.iter().any(|known| known.next == next) {
                    continue;
                }
                found.push(Continuation {
                    id: action.id.clone(),
                    next: next.to_string(),
                    label: action
                        .short_label
                        .clone()
                        .unwrap_or_else(|| action.label.clone()),
                });
            }
        }
    }
    found
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    prefix: char,
    choices: &[Continuation],
    theme: &Theme,
) {
    if choices.is_empty() {
        return;
    }
    let key_width = choices
        .iter()
        .map(|c| c.next.chars().count())
        .max()
        .unwrap_or(1);
    let label_width = choices
        .iter()
        .map(|c| c.label.chars().count())
        .max()
        .unwrap_or(1);
    let cell = (key_width + 2 + label_width.min(28)) as u16;
    let gap = 3u16;
    let room = area.width.saturating_sub(3);
    let columns = ((room + gap) / (cell + gap)).clamp(1, 3) as usize;
    let rows_needed = choices.len().div_ceil(columns);
    let max_rows = area.height.saturating_sub(4) as usize;
    if max_rows == 0 {
        return;
    }
    let rows = rows_needed.min(max_rows);
    let shown = (rows * columns).min(choices.len());
    let footer = shown < choices.len();

    let mut lines: Vec<Line> = (0..rows)
        .map(|row| {
            let mut spans = Vec::new();
            for column in 0..columns {
                let Some(choice) = choices
                    .get(column * rows + row)
                    .filter(|_| column * rows + row < shown)
                else {
                    break;
                };
                if column > 0 {
                    spans.push(Span::raw(" ".repeat(gap as usize)));
                }
                spans.push(Span::styled(
                    format!("{:<key_width$}", choice.next),
                    Style::default().fg(theme.accent).bold(),
                ));
                let label: String = choice.label.chars().take(28).collect();
                spans.push(Span::styled(
                    format!("  {label:<width$}", width = label_width.min(28)),
                    Style::default().fg(theme.text_primary),
                ));
            }
            Line::from(spans)
        })
        .collect();
    if footer {
        lines.push(Line::styled(
            format!("{shown} of {} · Esc, then ? shows all keys", choices.len()),
            Style::default().fg(theme.text_muted),
        ));
    }

    let content_width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    let title = format!(" {prefix} then… ");
    let width = (content_width.max(title.chars().count() as u16 + 12) + 2).min(area.width);
    let height = lines.len() as u16 + 2;
    let rect = Rect {
        x: area.right().saturating_sub(width + 1),
        y: area.bottom().saturating_sub(1 + height),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.accent))
                .title(Span::styled(
                    title,
                    Style::default().fg(theme.accent).bold(),
                ))
                .title_bottom(
                    Line::styled(" Esc cancel ", Style::default().fg(theme.text_muted))
                        .right_aligned(),
                )
                .style(Style::default().bg(theme.modal_bg)),
        ),
        rect,
    );
}
