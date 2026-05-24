//! Calendar-invites lens: a dedicated list of every detected calendar
//! invite across accounts, with inline RSVP. Entries are loaded via
//! `Request::ListInvites` (the dedicated `calendar_invites` store table),
//! so rows are event-centric — summary, when, organizer, and the viewer's
//! RSVP status — rather than plain message envelopes.
//!
//! This is the groundwork surface for future calendar/event features.

use mxr_core::types::CalendarPartstat;
use mxr_protocol::CalendarInviteData;
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::ActivePane;

pub struct CalendarInvitesView<'a> {
    pub entries: &'a [CalendarInviteData],
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: &'a ActivePane,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: &CalendarInvitesView<'_>,
    theme: &crate::theme::Theme,
) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let block = Block::bordered()
        .title(format!(" Calendar Invites ({}) ", view.entries.len()))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view.entries.is_empty() {
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No calendar invites yet.",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Invites detected in synced mail will appear here.",
                Style::default().fg(theme.text_muted),
            )),
        ]);
        frame.render_widget(msg, inner);
        return;
    }

    // Reserve the last row for the keybinding hint footer.
    let footer_height = 1u16;
    let list_height = inner.height.saturating_sub(footer_height) as usize;

    let mut lines = vec![Line::from(vec![
        Span::styled("  When             ", Style::default().fg(theme.text_muted)),
        Span::styled(
            "Event                          ",
            Style::default().fg(theme.text_muted),
        ),
        Span::styled(
            "Organizer            ",
            Style::default().fg(theme.text_muted),
        ),
        Span::styled("RSVP", Style::default().fg(theme.text_muted)),
    ])];

    // Window the rows around the selection (header consumes one line).
    let visible_rows = list_height.saturating_sub(1).max(1);
    let start = view.scroll_offset.min(view.entries.len().saturating_sub(1));
    let end = (start + visible_rows).min(view.entries.len());

    for (idx, invite) in view.entries.iter().enumerate().take(end).skip(start) {
        let selected = idx == view.selected_index;
        let cancelled = is_cancelled(invite);
        let meta = &invite.metadata;

        let when = format_when(meta.starts_at.as_deref());
        let summary = meta
            .summary
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("(no title)");
        let organizer = meta.organizer.as_ref().map_or_else(
            || "—".to_string(),
            |p| p.name.clone().unwrap_or_else(|| p.email.clone()),
        );
        let (rsvp, rsvp_color) = rsvp_label(invite, theme);

        let marker = if selected { "▌ " } else { "  " };
        let summary_style = if cancelled {
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::CROSSED_OUT)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let mut spans = vec![
            Span::raw(marker),
            Span::styled(
                format!("{:<16} ", truncate(&when, 16)),
                Style::default().fg(theme.accent),
            ),
            Span::styled(format!("{:<30} ", truncate(summary, 30)), summary_style),
            Span::styled(
                format!("{:<20} ", truncate(&organizer, 20)),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled(rsvp, Style::default().fg(rsvp_color)),
        ];
        if selected {
            for span in &mut spans {
                span.style = span.style.add_modifier(Modifier::REVERSED);
            }
        }
        lines.push(Line::from(spans));
    }

    let footer = Line::from(Span::styled(
        "  a accept · t tentative · d decline · A/T/D w/ comment · enter open",
        Style::default().fg(theme.text_muted),
    ));

    let body_area = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(footer_height),
        height: footer_height,
        ..inner
    };
    frame.render_widget(Paragraph::new(lines), body_area);
    frame.render_widget(Paragraph::new(footer), footer_area);
}

fn is_cancelled(invite: &CalendarInviteData) -> bool {
    let meta = &invite.metadata;
    meta.status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED"))
        || meta
            .method
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("CANCEL"))
}

fn rsvp_label(invite: &CalendarInviteData, theme: &crate::theme::Theme) -> (String, Color) {
    if is_cancelled(invite) {
        return ("Cancelled".to_string(), theme.error);
    }
    match invite.metadata.viewer_partstat {
        Some(CalendarPartstat::Accepted) => ("Accepted".to_string(), theme.success),
        Some(CalendarPartstat::Declined) => ("Declined".to_string(), theme.error),
        Some(CalendarPartstat::Tentative) => ("Tentative".to_string(), theme.warning),
        Some(CalendarPartstat::Delegated) => ("Delegated".to_string(), theme.text_muted),
        Some(CalendarPartstat::NeedsAction) => ("Needs action".to_string(), theme.warning),
        None => ("—".to_string(), theme.text_muted),
    }
}

/// Best-effort friendly rendering of an iCalendar DTSTART value. Falls back
/// to the raw string (truncated) when the value isn't a recognized form.
fn format_when(starts_at: Option<&str>) -> String {
    let Some(raw) = starts_at.map(str::trim).filter(|s| !s.is_empty()) else {
        return "—".to_string();
    };
    let cleaned = raw.trim_end_matches('Z');
    // iCal DATE-TIME: YYYYMMDDTHHMMSS
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(cleaned, "%Y%m%dT%H%M%S") {
        return dt.format("%a %b %-d %H:%M").to_string();
    }
    // ISO 8601-ish: YYYY-MM-DDTHH:MM:SS
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%dT%H:%M:%S") {
        return dt.format("%a %b %-d %H:%M").to_string();
    }
    // iCal DATE: YYYYMMDD (all-day)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(cleaned, "%Y%m%d") {
        return date.format("%a %b %-d").to_string();
    }
    truncate(raw, 16)
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
    use mxr_core::id::{AccountId, CalendarInviteId, MessageId};
    use mxr_core::types::{CalendarMetadata, CalendarPerson};
    use mxr_test_support::render_to_string;

    fn invite(
        summary: &str,
        starts_at: Option<&str>,
        partstat: Option<CalendarPartstat>,
    ) -> CalendarInviteData {
        CalendarInviteData {
            id: CalendarInviteId::new(),
            account_id: AccountId::new(),
            message_id: MessageId::new(),
            metadata: CalendarMetadata {
                summary: Some(summary.to_string()),
                starts_at: starts_at.map(str::to_string),
                organizer: Some(CalendarPerson {
                    email: "alice@example.com".into(),
                    name: Some("Alice".into()),
                    uri: None,
                }),
                viewer_partstat: partstat,
                ..Default::default()
            },
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn empty_state_renders_placeholder() {
        let rendered = render_to_string(90, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 90, 12),
                &CalendarInvitesView {
                    entries: &[],
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                },
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("No calendar invites yet."));
        assert!(rendered.contains("Calendar Invites (0)"));
    }

    #[test]
    fn populated_state_renders_summary_and_rsvp() {
        let entries = vec![
            invite(
                "Sprint planning",
                Some("20260526T140000Z"),
                Some(CalendarPartstat::Accepted),
            ),
            invite("1:1 w/ Bob", Some("20260527T093000Z"), None),
        ];
        let rendered = render_to_string(100, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 100, 12),
                &CalendarInvitesView {
                    entries: &entries,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                },
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Sprint planning"));
        assert!(rendered.contains("1:1 w/ Bob"));
        assert!(rendered.contains("Accepted"));
        assert!(rendered.contains("Alice"));
        // DTSTART formatted to a friendly form.
        assert!(rendered.contains("14:00"));
    }

    #[test]
    fn format_when_handles_ical_datetime_and_fallback() {
        assert_eq!(format_when(None), "—");
        assert!(format_when(Some("20260526T140000Z")).contains("14:00"));
        assert_eq!(format_when(Some("not-a-date")), "not-a-date");
    }
}
