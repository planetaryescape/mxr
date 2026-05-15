//! Slice 2.3 of docs/ai-email/02-follow-up-work.md.
//!
//! Renders a list of `OwedReplyRowData` rows ordered by overdue
//! score. The view is a pure render function; the surrounding
//! sidebar/keybinding hook-up is incremental TUI plumbing that
//! lives in `app/` once the lens state is wired through.

use mxr_protocol::OwedReplyRowData;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, rows: &[OwedReplyRowData], theme: &crate::theme::Theme) {
    let block = Block::bordered()
        .title(" Owed Replies ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if rows.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Inbox zero on owed replies.",
                Style::default().fg(theme.success),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Nothing waiting on you right now.",
                Style::default().fg(theme.text_muted),
            )),
        ]);
        frame.render_widget(msg, inner);
        return;
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("  Score  ", Style::default().fg(theme.text_muted)),
        Span::styled("Waiting  ", Style::default().fg(theme.text_muted)),
        Span::styled("Cadence  ", Style::default().fg(theme.text_muted)),
        Span::styled("From / Subject", Style::default().fg(theme.text_muted)),
    ])];

    for row in rows {
        let score_color = if row.overdue_score >= 2.0 {
            theme.error
        } else if row.overdue_score >= 1.0 {
            theme.warning
        } else {
            theme.text_muted
        };
        let from = row.from_name.as_deref().unwrap_or(&row.from_email);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:>4.2}  ", row.overdue_score),
                Style::default().fg(score_color),
            ),
            Span::raw(format!("{:>5.1}d  ", row.waiting_days)),
            Span::raw(format!("{:>5.1}d  ", row.expected_days)),
            Span::styled(truncate(from, 24), Style::default().fg(theme.accent)),
            Span::raw("  "),
            Span::raw(truncate(&row.subject, 60)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_test_support::render_to_string;

    fn row(from: &str, subject: &str, waiting: f64, expected: f64) -> OwedReplyRowData {
        OwedReplyRowData {
            thread_id: mxr_core::ThreadId::new(),
            latest_inbound_msg_id: mxr_core::MessageId::new(),
            from_email: from.to_string(),
            from_name: None,
            subject: subject.to_string(),
            latest_inbound_at: Utc::now() - chrono::Duration::days(waiting as i64),
            waiting_days: waiting,
            expected_days: expected,
            overdue_score: waiting / expected,
        }
    }

    #[test]
    fn empty_state_shows_inbox_zero_copy() {
        let rendered = render_to_string(80, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 12),
                &[],
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Inbox zero on owed replies."));
        assert!(rendered.contains("Owed Replies"));
        assert!(rendered.contains("Nothing waiting on you"));
    }

    #[test]
    fn populated_state_renders_subject_and_overdue_score() {
        let rows = vec![
            row("alice@example.com", "Status update?", 5.0, 1.0),
            row("bob@example.com", "Quick question", 2.0, 7.0),
        ];
        let rendered = render_to_string(100, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 12),
                &rows,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Status update?"));
        assert!(rendered.contains("Quick question"));
        assert!(rendered.contains("alice@example.com"));
        assert!(rendered.contains("bob@example.com"));
        // Alice's overdue score is 5.0 (very overdue); Bob's ~0.29.
        assert!(rendered.contains("5.00") || rendered.contains("5.0"));
    }

    #[test]
    fn rows_preserve_input_order() {
        // The lens does not re-sort; ordering is the caller's job.
        // This test pins that contract: row[0] renders before row[1].
        let rows = vec![
            row("zzz@example.com", "FIRST", 1.0, 1.0),
            row("aaa@example.com", "SECOND", 99.0, 1.0),
        ];
        let rendered = render_to_string(100, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 12),
                &rows,
                &crate::theme::Theme::default(),
            );
        });
        let first = rendered.find("FIRST").expect("FIRST should appear");
        let second = rendered.find("SECOND").expect("SECOND should appear");
        assert!(
            first < second,
            "lens must preserve caller's order; got first={first} second={second}"
        );
    }
}
