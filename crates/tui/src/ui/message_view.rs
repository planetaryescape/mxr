use crate::app::{ActivePane, BodyViewState};
use crate::theme::Theme;
use mxr_core::types::Envelope;
use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone)]
pub struct ThreadMessageBlock {
    pub envelope: Envelope,
    pub body_state: BodyViewState,
    pub labels: Vec<String>,
    pub attachments: Vec<String>,
    pub selected: bool,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    messages: &[ThreadMessageBlock],
    scroll_offset: u16,
    active_pane: &ActivePane,
    theme: &Theme,
) {
    let is_focused = *active_pane == ActivePane::MessageView;
    let border_style = theme.border_style(is_focused);

    let title = if messages.len() > 1 {
        " Thread "
    } else {
        " Message "
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    for (index, message) in messages.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "────────────────────────────────────────",
                Style::default().fg(theme.text_muted),
            )));
            lines.push(Line::from(""));
        }

        let env = &message.envelope;
        let from = env.from.name.as_deref().unwrap_or(&env.from.email);
        let label_style = if message.selected {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_muted)
        };
        let value_style = Style::default().fg(theme.text_primary);

        // Aligned headers with consistent label width
        let label_width = 10; // "Subject: " padded
        lines.push(Line::from(vec![
            Span::styled(format!("{:<label_width$}", "From:"), label_style),
            Span::styled(format!("{} <{}>", from, env.from.email), value_style),
        ]));
        if !env.to.is_empty() {
            let to_str = env
                .to
                .iter()
                .map(|a| {
                    a.name
                        .as_ref()
                        .map(|n| format!("{} <{}>", n, a.email))
                        .unwrap_or_else(|| a.email.clone())
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(Line::from(vec![
                Span::styled(format!("{:<label_width$}", "To:"), label_style),
                Span::styled(to_str, value_style),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{:<label_width$}", "Date:"), label_style),
            Span::styled(env.date.format("%Y-%m-%d %H:%M").to_string(), value_style),
        ]));
        lines.push(Line::from(vec![
            Span::styled(format!("{:<label_width$}", "Subject:"), label_style),
            Span::styled(env.subject.clone(), value_style),
        ]));

        // Label chips with colored backgrounds
        if !message.labels.is_empty() {
            let mut chips: Vec<Span> = Vec::new();
            for label in &message.labels {
                chips.push(Span::styled(
                    format!(" {} ", label),
                    Style::default()
                        .bg(Theme::label_color(label))
                        .fg(Color::Black),
                ));
                chips.push(Span::raw(" "));
            }
            lines.push(Line::from(chips));
        }

        // Attachments
        if !message.attachments.is_empty() {
            let mut chips: Vec<Span> = vec![Span::styled(
                format!("{:<label_width$}", "Attach:"),
                label_style,
            )];
            for attachment in &message.attachments {
                chips.push(Span::styled(
                    format!("[{}]", attachment),
                    Style::default().fg(theme.success).bold(),
                ));
                chips.push(Span::raw(" "));
            }
            lines.push(Line::from(chips));
        }
        lines.push(Line::from(""));

        match &message.body_state {
            BodyViewState::Ready { rendered, .. } => {
                lines.extend(process_body_lines(rendered, theme));
            }
            BodyViewState::Loading { preview } => {
                if let Some(preview) = preview {
                    lines.extend(process_body_lines(preview, theme));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    "Loading...",
                    Style::default().fg(theme.text_muted),
                )));
            }
            BodyViewState::Empty { preview } => {
                if let Some(preview) = preview {
                    lines.extend(process_body_lines(preview, theme));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    "(no body available)",
                    Style::default().fg(theme.text_muted),
                )));
            }
            BodyViewState::Error { message, preview } => {
                if let Some(preview) = preview {
                    lines.extend(process_body_lines(preview, theme));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!("Error: {message}"),
                    Style::default().fg(theme.error),
                )));
            }
        }
    }

    if messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "No message selected",
            Style::default().fg(theme.text_muted),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(paragraph, inner);
}

fn process_body_lines(raw: &str, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut quote_buffer: Vec<String> = Vec::new();
    let mut in_signature = false;
    let mut signature_lines: Vec<String> = Vec::new();
    let mut consecutive_blanks: u32 = 0;

    for line in raw.lines() {
        // Signature detection
        if line == "-- " || line == "--" {
            flush_quotes(&mut quote_buffer, &mut lines, theme);
            in_signature = true;
            continue;
        }

        // Blank line collapsing
        if line.trim().is_empty() {
            if in_signature {
                signature_lines.push(String::new());
                continue;
            }
            flush_quotes(&mut quote_buffer, &mut lines, theme);
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                lines.push(Line::from(""));
            }
            continue;
        }
        consecutive_blanks = 0;

        if in_signature {
            signature_lines.push(line.to_string());
            continue;
        }

        // Quote detection
        if line.starts_with('>') {
            quote_buffer.push(line.to_string());
            continue;
        }

        // Regular line — flush any pending quotes first
        flush_quotes(&mut quote_buffer, &mut lines, theme);
        lines.push(Line::from(line.to_string()));
    }

    // Flush remaining
    flush_quotes(&mut quote_buffer, &mut lines, theme);

    // Collapsed signature
    if !signature_lines.is_empty() {
        let count = signature_lines.len();
        lines.push(Line::from(Span::styled(
            format!("-- signature ({} lines) --", count),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    lines
}

fn flush_quotes(buffer: &mut Vec<String>, lines: &mut Vec<Line<'static>>, theme: &Theme) {
    if buffer.is_empty() {
        return;
    }

    let quote_style = Style::default().fg(theme.quote_fg);

    if buffer.len() <= 3 {
        for line in buffer.drain(..) {
            let cleaned = line
                .trim_start_matches('>')
                .trim_start_matches(' ')
                .to_string();
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.accent_dim)),
                Span::styled(cleaned, quote_style),
            ]));
        }
    } else {
        for line in &buffer[..2] {
            let cleaned = line
                .trim_start_matches('>')
                .trim_start_matches(' ')
                .to_string();
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(theme.accent_dim)),
                Span::styled(cleaned, quote_style),
            ]));
        }
        let hidden = buffer.len() - 2;
        lines.push(Line::from(Span::styled(
            format!("  ┆ ... {hidden} more quoted lines ..."),
            Style::default()
                .fg(theme.quote_fg)
                .add_modifier(Modifier::ITALIC),
        )));
        buffer.clear();
    }
}
