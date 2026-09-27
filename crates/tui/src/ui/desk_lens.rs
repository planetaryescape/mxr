//! The desk lens: what needs you, not what arrived.
//!
//! Renders the lanes from `Request::GetDesk` (You owe, Due, Waiting on,
//! New from people) with one cursor across all of them, and a single line
//! of counts for everything else. Pure render; wiring lives in `app/`.

use mxr_protocol::{DeskElsewhereData, DeskLaneData, DeskLaneKind, DeskRowData};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, DeskPageState};

pub struct DeskView<'a> {
    pub desk: &'a DeskPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
}

fn lane_title(kind: DeskLaneKind) -> &'static str {
    match kind {
        DeskLaneKind::Owed => "You owe",
        DeskLaneKind::Due => "Due",
        DeskLaneKind::Waiting => "Waiting on",
        DeskLaneKind::PeopleNew => "New from people",
    }
}

pub fn draw(frame: &mut Frame, area: Rect, view: &DeskView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let block = Block::bordered()
        .title(" Desk ")
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_line = 0usize;
    let mut row_index = 0usize;

    if !view.desk.loaded {
        lines.push(Line::from(Span::styled(
            "  Loading the desk…",
            Style::default().fg(theme.text_muted),
        )));
    } else if view.desk.row_count() == 0 {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Nothing needs you right now.",
            Style::default().fg(theme.text_secondary),
        )));
    }

    for (kind, lane) in &view.desk.lanes {
        if lane.rows.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(lane_header(*kind, lane, theme));
        for row in &lane.rows {
            let selected = row_index == view.selected_index;
            if selected {
                selected_line = lines.len();
            }
            lines.push(row_line(*kind, row, selected, width, theme));
            row_index += 1;
        }
        let hidden = (lane.total as usize).saturating_sub(lane.rows.len());
        if hidden > 0 {
            lines.push(Line::from(Span::styled(
                format!("  and {hidden} more"),
                Style::default().fg(theme.text_muted),
            )));
        }
    }

    // Keep the cursor on screen; the footer takes the last line.
    let footer_height = 1u16;
    let body_height = inner.height.saturating_sub(footer_height) as usize;
    let scroll = selected_line.saturating_sub(body_height.saturating_sub(2));
    let body_area = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(footer_height),
        height: footer_height,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            elsewhere_line(&view.desk.elsewhere),
            Style::default().fg(theme.text_muted),
        ))),
        footer_area,
    );
}

fn lane_header(
    kind: DeskLaneKind,
    lane: &DeskLaneData,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {}", lane_title(kind).to_uppercase()),
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(
            format!(" {}", lane.total),
            Style::default().fg(theme.accent),
        ),
    ])
}

fn row_line(
    kind: DeskLaneKind,
    row: &DeskRowData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let who = row
        .counterparty_name
        .clone()
        .unwrap_or_else(|| row.counterparty_email.clone());
    let who = if kind == DeskLaneKind::Due {
        format!("to {who}")
    } else {
        who
    };
    let what = if row.subject.is_empty() {
        row.reason.clone()
    } else {
        format!("{} \u{b7} {}", row.subject, row.reason)
    };
    let age = age_cell(row);
    let marker = if selected { "\u{258c} " } else { "  " };
    let who_width = 20usize;
    // marker (2) + who + gap (2) + what + gap (2) + age
    let what_width = width
        .saturating_sub(2 + who_width + 2 + 2 + age.chars().count())
        .max(8);
    let age_style = if row.overdue {
        Style::default().fg(theme.warning)
    } else {
        Style::default().fg(theme.text_muted)
    };
    let mut spans = vec![
        Span::raw(marker),
        Span::styled(
            format!("{:<who_width$}  ", truncate(&who, who_width)),
            Style::default().fg(theme.text_primary),
        ),
        Span::styled(
            format!("{:<what_width$}  ", truncate(&what, what_width)),
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(age, age_style),
    ];
    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
    Line::from(spans)
}

/// "2d", "in 2d" for a promise not yet due, plus " · usually 4h".
pub(crate) fn age_cell(row: &DeskRowData) -> String {
    let age = if row.age_seconds < 0 {
        format!("in {}", short_duration(-row.age_seconds))
    } else {
        short_duration(row.age_seconds)
    };
    match row.usual_seconds {
        Some(usual) if row.lane != DeskLaneKind::Due => {
            format!("{age} \u{b7} usually {}", short_duration(usual))
        }
        _ => age,
    }
}

fn short_duration(seconds: i64) -> String {
    match seconds.max(0) {
        s if s < 3_600 => format!("{}m", (s / 60).max(1)),
        s if s < 86_400 => format!("{}h", s / 3_600),
        s if s < 14 * 86_400 => format!("{}d", s / 86_400),
        s => format!("{}w", s / (7 * 86_400)),
    }
}

/// "Everything else: Reading 7 · Deliveries 1", only the non-zero parts.
pub(crate) fn elsewhere_line(elsewhere: &DeskElsewhereData) -> String {
    let parts: Vec<String> = [
        ("Reading", elsewhere.reading),
        ("Paper trail", elsewhere.paper_trail),
        ("Deliveries", elsewhere.deliveries),
        ("Invites", elsewhere.invites),
        ("Screener", elsewhere.screener),
    ]
    .iter()
    .filter(|(_, count)| *count > 0)
    .map(|(label, count)| format!("{label} {count}"))
    .collect();
    if parts.is_empty() {
        "  enter open".to_string()
    } else {
        format!("  Everything else: {}", parts.join(" \u{b7} "))
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
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
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_test_support::render_to_string;

    pub(crate) fn row(lane: DeskLaneKind, name: &str, age: i64, usual: Option<i64>) -> DeskRowData {
        DeskRowData {
            lane,
            account_id: AccountId::new(),
            thread_id: ThreadId::new(),
            message_id: MessageId::new(),
            message_ids: vec![],
            counterparty_email: format!("{}@example.com", name.to_lowercase()),
            counterparty_name: Some(name.to_string()),
            subject: "Launch checklist".into(),
            reason: "replied to your message".into(),
            since: chrono::Utc::now(),
            age_seconds: age,
            usual_seconds: usual,
            usual_samples: 3,
            overdue: usual.is_some_and(|u| age > u),
            unread: true,
            commitment_id: None,
        }
    }

    fn desk(owed: Vec<DeskRowData>, due: Vec<DeskRowData>) -> DeskPageState {
        let lane = |rows: Vec<DeskRowData>| DeskLaneData {
            total: rows.len() as u32,
            rows,
        };
        DeskPageState {
            lanes: vec![
                (DeskLaneKind::Owed, lane(owed)),
                (DeskLaneKind::Due, lane(due)),
                (DeskLaneKind::Waiting, DeskLaneData::default()),
                (DeskLaneKind::PeopleNew, DeskLaneData::default()),
            ],
            elsewhere: DeskElsewhereData {
                reading: 7,
                screener: 2,
                ..Default::default()
            },
            loaded: true,
        }
    }

    fn render(desk: &DeskPageState) -> String {
        render_to_string(110, 16, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 110, 16),
                &DeskView {
                    desk,
                    selected_index: 0,
                    active_pane: &ActivePane::MailList,
                },
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn lanes_render_with_reason_pace_and_elsewhere() {
        let rendered = render(&desk(
            vec![row(DeskLaneKind::Owed, "Maya", 2 * 86_400, Some(4 * 3_600))],
            vec![row(DeskLaneKind::Due, "Nora", -2 * 86_400, None)],
        ));
        assert!(rendered.contains("YOU OWE 1"), "{rendered}");
        assert!(rendered.contains("DUE 1"));
        assert!(!rendered.contains("WAITING ON"), "empty lanes are skipped");
        assert!(rendered.contains("Launch checklist \u{b7} replied to your message"));
        assert!(rendered.contains("2d \u{b7} usually 4h"));
        assert!(rendered.contains("to Nora"));
        assert!(rendered.contains("in 2d"));
        assert!(rendered.contains("Everything else: Reading 7 \u{b7} Screener 2"));
        assert!(!rendered.contains('\u{2014}'), "no em dashes");
    }

    #[test]
    fn empty_desk_is_calm() {
        let rendered = render(&desk(vec![], vec![]));
        assert!(rendered.contains("Nothing needs you right now."));
        assert!(!rendered.contains("YOU OWE"));
    }

    #[test]
    fn age_cell_formats_future_and_pace() {
        assert_eq!(age_cell(&row(DeskLaneKind::Owed, "A", 90, None)), "1m");
        assert_eq!(
            age_cell(&row(DeskLaneKind::Due, "A", -3 * 3_600, None)),
            "in 3h"
        );
        assert_eq!(
            age_cell(&row(DeskLaneKind::Waiting, "A", 5 * 86_400, Some(7_200))),
            "5d \u{b7} usually 2h"
        );
    }
}
