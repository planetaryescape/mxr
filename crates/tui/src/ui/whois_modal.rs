//! Slice 6.1 (C2.9): renders the whois modal — explains an entity
//! using local evidence (sender_profile + relationship for emails,
//! lexical-search citations for terms, candidate list for ambiguous).

use crate::app::WhoisModalState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &WhoisModalState, theme: &crate::theme::Theme) {
    if !state.visible {
        return;
    }
    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);

    let title = match state.query.as_deref() {
        Some(q) => format!(" Whois: {q} "),
        None => " Whois ".to_string(),
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line<'_>> = Vec::new();

    if state.loading {
        lines.push(Line::from(Span::styled(
            "  loading…",
            Style::default().fg(theme.text_muted),
        )));
    } else if let Some(err) = state.error.as_deref() {
        lines.push(Line::from(Span::styled(
            format!("  error: {err}"),
            Style::default().fg(theme.error),
        )));
    } else if let Some(entity) = state.entity.as_ref() {
        lines.push(Line::from(format!(
            "  {} ({})",
            entity.canonical_name, entity.kind
        )));
        lines.push(Line::from(""));
        for line in entity.summary.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
        if !entity.candidates.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Candidates:",
                Style::default().fg(theme.text_muted),
            )));
            for c in &entity.candidates {
                lines.push(Line::from(format!(
                    "    {} ({}, {} mentions)",
                    c.value, c.kind, c.mention_count
                )));
            }
        }
        if !entity.citations.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Citations:",
                Style::default().fg(theme.text_muted),
            )));
            for c in &entity.citations {
                lines.push(Line::from(format!("    msg={} \"{}\"", c.msg_id, c.quote)));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  (no data)",
            Style::default().fg(theme.text_muted),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Esc] close",
        Style::default().fg(theme.text_muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
    use mxr_protocol::{EntityCandidateData, EntityExplanationData, WhoisCitationData};
    use mxr_test_support::render_to_string;

    fn person_entity() -> EntityExplanationData {
        EntityExplanationData {
            canonical_name: "alice@example.com".into(),
            kind: "person".into(),
            summary: "Alice Smith — 42 inbound, 31 outbound.".into(),
            first_seen_at: None,
            last_seen_at: None,
            topics: vec![],
            citations: vec![],
            candidates: vec![],
        }
    }

    #[test]
    fn loading_state_shows_indicator() {
        let mut state = WhoisModalState::default();
        state.open_loading("alice@example.com".into());
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("loading"));
        assert!(rendered.contains("Whois: alice@example.com"));
    }

    #[test]
    fn person_kind_renders_summary() {
        let mut state = WhoisModalState::default();
        state.open_loading("alice@example.com".into());
        state.set_entity(person_entity());
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("alice@example.com (person)"));
        assert!(rendered.contains("42 inbound"));
        insta::assert_snapshot!("whois_modal_person", rendered);
    }

    #[test]
    fn ambiguous_kind_renders_candidate_list() {
        let mut state = WhoisModalState::default();
        state.open_loading("fizzbuzz".into());
        state.set_entity(EntityExplanationData {
            canonical_name: "fizzbuzz".into(),
            kind: "ambiguous".into(),
            summary: "Multiple senders match.".into(),
            first_seen_at: None,
            last_seen_at: None,
            topics: vec![],
            citations: vec![],
            candidates: vec![
                EntityCandidateData {
                    kind: "person".into(),
                    value: "alice@example.com".into(),
                    display_name: None,
                    mention_count: 3,
                },
                EntityCandidateData {
                    kind: "person".into(),
                    value: "bob@example.com".into(),
                    display_name: None,
                    mention_count: 2,
                },
            ],
        });
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("ambiguous"));
        assert!(rendered.contains("alice@example.com"));
        assert!(rendered.contains("bob@example.com"));
        assert!(rendered.contains("Candidates"));
        insta::assert_snapshot!("whois_modal_ambiguous", rendered);
    }

    #[test]
    fn term_kind_with_citations_renders_them() {
        let mut state = WhoisModalState::default();
        state.open_loading("Project Apollo".into());
        state.set_entity(EntityExplanationData {
            canonical_name: "Project Apollo".into(),
            kind: "term".into(),
            summary: "Most likely match: alice@example.com (3 mentions).".into(),
            first_seen_at: None,
            last_seen_at: None,
            topics: vec![],
            citations: vec![WhoisCitationData {
                msg_id: "msg-7".into(),
                quote: "Project Apollo update".into(),
            }],
            candidates: vec![],
        });
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Project Apollo (term)"));
        assert!(rendered.contains("msg=msg-7"));
        insta::assert_snapshot!("whois_modal_term", rendered);
    }

    #[test]
    fn unknown_kind_renders_no_evidence_summary() {
        let mut state = WhoisModalState::default();
        state.open_loading("ghost-term".into());
        state.set_entity(EntityExplanationData {
            canonical_name: "ghost-term".into(),
            kind: "unknown".into(),
            summary: "No local evidence found.".into(),
            first_seen_at: None,
            last_seen_at: None,
            topics: vec![],
            citations: vec![],
            candidates: vec![],
        });
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("ghost-term (unknown)"));
        assert!(rendered.contains("No local evidence"));
        insta::assert_snapshot!("whois_modal_unknown", rendered);
    }

    #[test]
    fn invisible_modal_renders_nothing() {
        let state = WhoisModalState::default();
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(!rendered.contains("Whois"));
    }
}
