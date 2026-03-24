use crate::mxr_core::types::Envelope;
use crate::mxr_tui::app::{ActivePane, AttachmentSummary, BodyViewState};
use crate::mxr_tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone)]
pub struct ThreadMessageBlock {
    pub envelope: Envelope,
    pub body_state: BodyViewState,
    pub labels: Vec<String>,
    pub attachments: Vec<AttachmentSummary>,
    pub selected: bool,
    pub bulk_selected: bool,
    pub has_unsubscribe: bool,
    pub signature_expanded: bool,
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

        // Selection + label chips
        if message.bulk_selected || !message.labels.is_empty() {
            let mut chips: Vec<Span> = Vec::new();
            if message.bulk_selected {
                chips.push(Span::styled(
                    " Selected ",
                    Style::default()
                        .bg(theme.selection_bg)
                        .fg(theme.selection_fg)
                        .add_modifier(Modifier::BOLD),
                ));
                chips.push(Span::raw(" "));
            }
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

        if message.has_unsubscribe {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<label_width$}", "List:"), label_style),
                Span::styled(
                    " unsubscribe ",
                    Style::default().bg(theme.warning).fg(Color::Black).bold(),
                ),
            ]));
        }

        // Attachments
        if !message.attachments.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                format!("{:<label_width$}", "Attach:"),
                label_style,
            )]));
            for attachment in &message.attachments {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(label_width)),
                    Span::styled(
                        &attachment.filename,
                        Style::default().fg(theme.success).bold(),
                    ),
                    Span::styled(
                        format!(" ({})", human_size(attachment.size_bytes)),
                        Style::default().fg(theme.text_muted),
                    ),
                ]));
            }
        }
        lines.push(Line::from(""));

        match &message.body_state {
            BodyViewState::Ready { rendered, .. } => {
                lines.extend(process_body_lines(
                    rendered,
                    theme,
                    message.signature_expanded,
                ));
            }
            BodyViewState::Loading { preview } => {
                if let Some(preview) = preview {
                    lines.extend(process_body_lines(
                        preview,
                        theme,
                        message.signature_expanded,
                    ));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    "Loading...",
                    Style::default().fg(theme.text_muted),
                )));
            }
            BodyViewState::Empty { preview } => {
                if let Some(preview) = preview {
                    lines.extend(process_body_lines(
                        preview,
                        theme,
                        message.signature_expanded,
                    ));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    "(no body available)",
                    Style::default().fg(theme.text_muted),
                )));
            }
            BodyViewState::Error {
                message: err_msg,
                preview,
            } => {
                if let Some(preview) = preview {
                    lines.extend(process_body_lines(
                        preview,
                        theme,
                        message.signature_expanded,
                    ));
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!("Error: {err_msg}"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_core::id::{AccountId, MessageId, ThreadId};
    use crate::mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};
    use crate::mxr_tui::app::BodySource;
    use chrono::{TimeZone, Utc};
    use mxr_test_support::render_to_string;

    fn envelope() -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "msg-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Selection".into(),
            date: Utc.with_ymd_and_hms(2024, 3, 15, 9, 30, 0).unwrap(),
            flags: MessageFlags::READ,
            snippet: "snippet".into(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        }
    }

    #[test]
    fn selected_messages_render_visible_chip() {
        let block = ThreadMessageBlock {
            envelope: envelope(),
            body_state: BodyViewState::Ready {
                raw: "hello".into(),
                rendered: "hello".into(),
                source: BodySource::Plain,
            },
            labels: vec!["INBOX".into()],
            attachments: vec![],
            selected: true,
            bulk_selected: true,
            has_unsubscribe: false,
            signature_expanded: false,
        };

        let snapshot = render_to_string(70, 18, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 70, 18),
                &[block],
                0,
                &ActivePane::MessageView,
                &Theme::default(),
            );
        });

        assert!(snapshot.contains("Selected"));
    }
}

fn process_body_lines(raw: &str, theme: &Theme, signature_expanded: bool) -> Vec<Line<'static>> {
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
        lines.push(style_line_with_links(line, theme));
    }

    // Flush remaining
    flush_quotes(&mut quote_buffer, &mut lines, theme);

    if !signature_lines.is_empty() {
        if signature_expanded {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "-- signature --",
                Style::default()
                    .fg(theme.signature_fg)
                    .add_modifier(Modifier::ITALIC),
            )));
            for line in signature_lines {
                lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(theme.signature_fg),
                )));
            }
        } else {
            let count = signature_lines.len();
            lines.push(Line::from(Span::styled(
                format!("-- signature ({} lines, press S to expand) --", count),
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }

    lines
}

fn human_size(size_bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;

    if size_bytes >= MB {
        format!("{:.1} MB", size_bytes as f64 / MB as f64)
    } else if size_bytes >= KB {
        format!("{:.1} KB", size_bytes as f64 / KB as f64)
    } else {
        format!("{size_bytes} B")
    }
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

/// Split a line into spans, highlighting URLs in link_fg with underline.
fn style_line_with_links(line: &str, theme: &Theme) -> Line<'static> {
    let link_style = Style::default()
        .fg(theme.link_fg)
        .add_modifier(Modifier::UNDERLINED);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("http://").or_else(|| rest.find("https://")) {
        // Text before the URL
        if start > 0 {
            spans.push(Span::raw(rest[..start].to_string()));
        }

        // Find end of URL (whitespace, angle bracket, or end of string)
        let url_rest = &rest[start..];
        let end = url_rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == ')' || c == ']' || c == '"')
            .unwrap_or(url_rest.len());

        let url = &url_rest[..end];
        // Strip trailing punctuation that's probably not part of the URL
        let url_trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        let trimmed_len = url_trimmed.len();

        spans.push(Span::styled(url_trimmed.to_string(), link_style));

        // Any trailing punctuation goes back as plain text
        if trimmed_len < end {
            spans.push(Span::raw(url_rest[trimmed_len..end].to_string()));
        }

        rest = &rest[start + end..];
    }

    // Remaining text after last URL
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }

    if spans.is_empty() {
        Line::from(line.to_string())
    } else {
        Line::from(spans)
    }
}
