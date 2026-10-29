//! Slice 2.3 of docs/reference/ai-email.md
//!
//! Renders a list of `OwedReplyRowData` rows ordered by overdue
//! score. The view is a pure render function; the surrounding
//! sidebar/keybinding hook-up is incremental TUI plumbing that
//! lives in `app/` once the lens state is wired through.

use super::account_mark::AccountMarks;
use super::desk_lens::short_duration;
use mxr_protocol::{OwedReplyRowData, PeopleSplitData};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    rows: &[OwedReplyRowData],
    people: Option<PeopleSplitData>,
    account_marks: &AccountMarks,
    theme: &crate::theme::Theme,
) {
    // Where the people who wrote to you but you don't write with are.
    let others = people.filter(|people| people.others > 0).map(|people| {
        let n = people.others;
        format!(
            "  {n} {} who wrote to you {} in Messages.",
            if n == 1 { "other" } else { "others" },
            if n == 1 { "is" } else { "are" }
        )
    });
    let block = Block::bordered()
        .title(" Owed Replies ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if rows.is_empty() {
        let mut lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Inbox zero on owed replies.",
                Style::default().fg(theme.success),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Nobody you write with is waiting on a reply.",
                Style::default().fg(theme.text_muted),
            )),
        ];
        if let Some(others) = others {
            lines.push(Line::from(Span::styled(
                others,
                Style::default().fg(theme.text_muted),
            )));
        }
        let msg = Paragraph::new(lines);
        frame.render_widget(msg, inner);
        return;
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("  Score  ", Style::default().fg(theme.text_muted)),
        Span::styled("Waiting  ", Style::default().fg(theme.text_muted)),
        Span::styled("Cadence  ", Style::default().fg(theme.text_muted)),
        Span::raw(" ".repeat(account_marks.column_width())),
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
        let mut spans = vec![
            Span::styled(
                format!("  {:>4.2}  ", row.overdue_score),
                Style::default().fg(score_color),
            ),
            Span::raw(format!("{:>7}  ", days_label(row.waiting_days))),
            Span::raw(format!("{:>7}  ", days_label(row.expected_days))),
        ];
        // Rows without an account (an older daemon) keep the chip column's
        // width, so the subjects still line up.
        match row
            .account_id
            .as_ref()
            .and_then(|account_id| account_marks.row_cell(account_id, theme))
        {
            Some(chip) => spans.extend(chip),
            None => spans.push(Span::raw(" ".repeat(account_marks.column_width()))),
        }
        spans.push(Span::styled(
            truncate(from, 24),
            Style::default().fg(theme.accent),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::raw(truncate(&row.subject, 60)));
        lines.push(Line::from(spans));
    }

    if let Some(others) = others {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            others,
            Style::default().fg(theme.text_muted),
        )));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The desk's "47m", "5h", "3d": paces are often under a day.
fn days_label(days: f64) -> String {
    short_duration((days * 86_400.0).round() as i64)
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
            usual_seconds: None,
            account_id: None,
        }
    }

    #[test]
    fn empty_state_shows_inbox_zero_copy() {
        let rendered = render_to_string(80, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 12),
                &[],
                None,
                &AccountMarks::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Inbox zero on owed replies."));
        assert!(rendered.contains("Owed Replies"));
        assert!(rendered.contains("Nobody you write with is waiting on a reply."));
        assert!(!rendered.contains("in Messages"));
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
                None,
                &AccountMarks::default(),
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
    fn paces_under_a_day_read_in_hours_and_minutes_not_zero_days() {
        let rows = vec![row("alice@example.com", "Status update?", 0.25, 0.0326)];
        let rendered = render_to_string(100, 6, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 6),
                &rows,
                None,
                &AccountMarks::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("6h"), "{rendered}");
        assert!(rendered.contains("46m"), "{rendered}");
        assert!(!rendered.contains("0.0d"), "{rendered}");
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
                None,
                &AccountMarks::default(),
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

    #[test]
    fn empty_lane_says_where_the_others_are() {
        let people = Some(PeopleSplitData {
            write_with: 0,
            others: 3,
        });
        let rendered = render_to_string(80, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 12),
                &[],
                people,
                &AccountMarks::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("3 others who wrote to you are in Messages."));
        assert!(!rendered.contains("Nothing waiting on you"));
    }

    fn mixed_marks(work: &mxr_core::AccountId, home: &mxr_core::AccountId) -> AccountMarks {
        crate::ui::account_mark::plain_marks_for_tests(&[(work, "Work", "W"), (home, "Home", "H")])
    }

    #[test]
    fn mixed_accounts_mark_each_row_with_its_own_chip() {
        let work = mxr_core::AccountId::new();
        let home = mxr_core::AccountId::new();
        let mut alice = row("alice@example.com", "Status update?", 5.0, 1.0);
        alice.account_id = Some(work.clone());
        let mut bob = row("bob@example.com", "Quick question", 2.0, 7.0);
        bob.account_id = Some(home.clone());
        let marks = mixed_marks(&work, &home);
        let rendered = render_to_string(100, 8, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 8),
                &[alice, bob],
                None,
                &marks,
                &crate::theme::Theme::default(),
            );
        });
        let alice_line = rendered
            .lines()
            .find(|line| line.contains("Status update?"))
            .expect("alice's row");
        let bob_line = rendered
            .lines()
            .find(|line| line.contains("Quick question"))
            .expect("bob's row");
        assert!(alice_line.contains("[W]"), "{rendered}");
        assert!(bob_line.contains("[H]"), "{rendered}");
        insta::assert_snapshot!("owed_lens_mixed_accounts", rendered);
    }

    #[test]
    fn a_single_account_view_shows_no_chip_column() {
        let rows = vec![row("alice@example.com", "Status update?", 5.0, 1.0)];
        let rendered = render_to_string(100, 6, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 6),
                &rows,
                None,
                &AccountMarks::default(),
                &crate::theme::Theme::default(),
            );
        });
        assert!(!rendered.contains('['), "{rendered}");
    }
}
