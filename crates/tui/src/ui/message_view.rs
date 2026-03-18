use crate::app::ActivePane;
use mxr_core::types::Envelope;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    body_text: Option<&str>,
    envelope: Option<&Envelope>,
    scroll_offset: u16,
    active_pane: &ActivePane,
) {
    let is_focused = *active_pane == ActivePane::MessageView;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Message ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(env) = envelope {
        let from = env.from.name.as_deref().unwrap_or(&env.from.email);
        lines.push(Line::from(vec![
            Span::styled("From: ", Style::default().bold()),
            Span::raw(format!("{} <{}>", from, env.from.email)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Date: ", Style::default().bold()),
            Span::raw(env.date.format("%Y-%m-%d %H:%M").to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Subject: ", Style::default().bold()),
            Span::raw(env.subject.clone()),
        ]));
        lines.push(Line::from(""));
    }

    if let Some(body) = body_text {
        lines.extend(process_body_lines(body));
    } else {
        lines.push(Line::from(Span::styled(
            "Loading...",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    frame.render_widget(paragraph, inner);
}

fn process_body_lines(raw: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut quote_buffer: Vec<String> = Vec::new();
    let mut in_signature = false;
    let mut consecutive_blanks: u32 = 0;

    for line in raw.lines() {
        // Signature detection
        if line == "-- " || line == "--" {
            flush_quotes(&mut quote_buffer, &mut lines);
            in_signature = true;
            continue;
        }

        // Blank line collapsing
        if line.trim().is_empty() {
            flush_quotes(&mut quote_buffer, &mut lines);
            consecutive_blanks += 1;
            if consecutive_blanks <= 2 {
                lines.push(Line::from(""));
            }
            continue;
        }
        consecutive_blanks = 0;

        if in_signature {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        // Quote detection
        if line.starts_with('>') {
            quote_buffer.push(line.to_string());
            continue;
        }

        // Regular line — flush any pending quotes first
        flush_quotes(&mut quote_buffer, &mut lines);
        lines.push(Line::from(line.to_string()));
    }

    // Flush remaining
    flush_quotes(&mut quote_buffer, &mut lines);
    lines
}

fn flush_quotes(buffer: &mut Vec<String>, lines: &mut Vec<Line<'static>>) {
    if buffer.is_empty() {
        return;
    }

    let quote_style = Style::default().fg(Color::DarkGray);

    if buffer.len() <= 3 {
        for line in buffer.drain(..) {
            let cleaned = line
                .trim_start_matches('>')
                .trim_start_matches(' ')
                .to_string();
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(Color::Blue)),
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
                Span::styled("│ ", Style::default().fg(Color::Blue)),
                Span::styled(cleaned, quote_style),
            ]));
        }
        let hidden = buffer.len() - 2;
        lines.push(Line::from(Span::styled(
            format!("  ┆ ... {hidden} more quoted lines ..."),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
        buffer.clear();
    }
}
