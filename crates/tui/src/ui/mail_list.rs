use crate::app::ActivePane;
use chrono::{Datelike, Local, Utc};
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, MessageFlags};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    envelopes: &[Envelope],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
    title: &str,
) {
    draw_with_selection(
        frame,
        area,
        envelopes,
        selected_index,
        scroll_offset,
        active_pane,
        title,
        &HashSet::new(),
    );
}

pub fn draw_with_selection(
    frame: &mut Frame,
    area: Rect,
    envelopes: &[Envelope],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
    title: &str,
    selected_set: &HashSet<MessageId>,
) {
    let is_focused = *active_pane == ActivePane::MailList;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;

    // Responsive: hide columns when narrow
    let compact = inner_width < 60;
    let ultra_compact = inner_width < 40;

    let items: Vec<ListItem> = envelopes
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(i, env)| {
            let is_selected = i == selected_index;
            let is_unread = !env.flags.contains(MessageFlags::READ);
            let is_starred = env.flags.contains(MessageFlags::STARRED);
            let is_answered = env.flags.contains(MessageFlags::ANSWERED);
            let is_in_set = selected_set.contains(&env.id);

            let mut spans: Vec<Span> = Vec::new();

            // Selection indicator
            if !selected_set.is_empty() {
                spans.push(if is_in_set {
                    Span::styled("*", Style::default().fg(Color::Magenta).bold())
                } else {
                    Span::raw(" ")
                });
            }

            // Line number — hide when ultra compact
            if !ultra_compact {
                spans.push(Span::styled(
                    format!("{:>3} ", i + 1),
                    Style::default().fg(Color::Rgb(80, 80, 80)),
                ));
            }

            // Flags: always show
            spans.push(if is_unread {
                Span::styled("N", Style::default().fg(Color::Cyan).bold())
            } else {
                Span::raw(" ")
            });
            spans.push(if is_starred {
                Span::styled("★", Style::default().fg(Color::Yellow))
            } else {
                Span::raw(" ")
            });
            if !compact {
                spans.push(if is_answered {
                    Span::styled("A", Style::default().fg(Color::Blue))
                } else {
                    Span::raw(" ")
                });
            }

            // From
            let from_raw = env.from.name.as_deref().unwrap_or(&env.from.email);
            let from_width = if ultra_compact {
                inner_width / 3
            } else if compact {
                inner_width / 4
            } else {
                18
            };
            let from_truncated = truncate_str(from_raw, from_width);
            spans.push(Span::raw(format!(
                " {:<w$}",
                from_truncated,
                w = from_width
            )));

            // Subject: flexible width
            let date_str = format_date(&env.date);
            let right_str = if compact {
                date_str.clone()
            } else {
                let size_str = format_size(env.size_bytes);
                format!("{} {}", date_str, size_str)
            };
            let right_width = right_str.len() + 1;

            let used = if ultra_compact {
                2 + 1
            } else if compact {
                4 + 1 + 2
            } else {
                4 + 1 + 3
            };
            let subject_width = inner_width.saturating_sub(used + from_width + right_width);
            let subject_truncated = truncate_str(&env.subject, subject_width);
            spans.push(Span::raw(format!(
                " {:<w$}",
                subject_truncated,
                w = subject_width
            )));

            // Date (+ size) — right side
            spans.push(Span::styled(
                format!(" {}", right_str),
                Style::default().fg(Color::Rgb(100, 100, 110)),
            ));

            let line = Line::from(spans);

            let base_style = if is_selected {
                Style::default().bg(Color::Rgb(50, 50, 60)).fg(Color::White)
            } else if is_in_set {
                Style::default().bg(Color::Rgb(40, 30, 50)).fg(Color::White)
            } else if is_unread {
                Style::default().fg(Color::White).bold()
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(line).style(base_style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn format_date(date: &chrono::DateTime<Utc>) -> String {
    let local = date.with_timezone(&Local);
    let now = Local::now();

    if local.date_naive() == now.date_naive() {
        local.format("%I:%M%p").to_string()
    } else if local.year() == now.year() {
        local.format("%b %d").to_string()
    } else {
        local.format("%m/%d/%y").to_string()
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("({:.0}M)", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("({:.0}K)", bytes as f64 / 1024.0)
    } else {
        format!("({}B)", bytes)
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}
