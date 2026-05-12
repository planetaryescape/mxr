use crate::app::SenderProfileModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 70;
const MODAL_HEIGHT_PERCENT: u16 = 78;

pub fn draw(frame: &mut Frame, area: Rect, state: &SenderProfileModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(area, MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT);
    Clear.render(modal_area, frame.buffer_mut());

    let title = match &state.email {
        Some(email) => format!(" Sender · {email} · j/k select · Enter open · Esc close "),
        None => " Sender · j/k select · Enter open · Esc close ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area).inner(Margin::new(1, 1));
    frame.render_widget(block, modal_area);

    if let Some(message) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to load sender profile: {message}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Loading sender profile...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let lines = match &state.profile {
        None => vec![Line::from(Span::styled(
            "Sender unknown — no contact data yet.\n\nTry `mxr sync` to populate the contacts \
             table, or `mxr sender <email>` from the CLI.",
            Style::default().fg(theme.text_muted),
        ))],
        Some(profile) => profile_lines(state, profile, theme),
    };

    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(theme.text_primary))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn profile_lines<'a>(
    state: &'a SenderProfileModalState,
    profile: &'a mxr_protocol::SenderProfileData,
    theme: &Theme,
) -> Vec<Line<'a>> {
    let label_style = Style::default().fg(theme.text_muted);
    let mut lines = Vec::new();

    if let Some(name) = &profile.display_name {
        lines.push(Line::from(vec![
            Span::styled("Name: ", label_style),
            Span::raw(name.clone()),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("Volume: ", label_style),
        Span::raw(format!(
            "{} inbound · {} outbound · {} replied",
            profile.total_inbound, profile.total_outbound, profile.replied_count
        )),
    ]));

    if let Some(p50) = profile.cadence_days_p50 {
        lines.push(Line::from(vec![
            Span::styled("Cadence p50: ", label_style),
            Span::raw(format!("{p50:.1} days")),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("Open threads: ", label_style),
        Span::raw(profile.open_thread_count.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Storage: ", label_style),
        Span::raw(format!(
            "{} in · {} out · {} attachments",
            human_bytes(profile.inbound_storage_bytes),
            human_bytes(profile.outbound_storage_bytes),
            profile.attachment_count
        )),
    ]));

    lines.push(Line::from(vec![
        Span::styled("First seen: ", label_style),
        Span::raw(profile.first_seen_at.format("%Y-%m-%d").to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Last seen:  ", label_style),
        Span::raw(profile.last_seen_at.format("%Y-%m-%d").to_string()),
    ]));
    if let Some(last_inbound) = profile.last_inbound_at {
        lines.push(Line::from(vec![
            Span::styled("Last from:  ", label_style),
            Span::raw(last_inbound.format("%Y-%m-%d").to_string()),
        ]));
    }
    if let Some(last_outbound) = profile.last_outbound_at {
        lines.push(Line::from(vec![
            Span::styled("Last to:    ", label_style),
            Span::raw(last_outbound.format("%Y-%m-%d").to_string()),
        ]));
    }

    if profile.is_list_sender {
        let list_id = profile.list_id.as_deref().unwrap_or("(no List-ID header)");
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("List sender — List-ID: {list_id}"),
            Style::default().fg(theme.warning),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Other emails from sender",
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )));
    let recent_messages = state.recent_messages();
    if recent_messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "No other local emails from this sender yet.",
            label_style,
        )));
    } else {
        for (index, message) in recent_messages.iter().enumerate() {
            let marker = if index == state.selected_recent_index {
                "› "
            } else {
                "  "
            };
            let row_style = if index == state.selected_recent_index {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            lines.push(
                Line::from(vec![
                    Span::styled(marker, row_style),
                    Span::styled(message.subject.trim().to_string(), row_style),
                    Span::styled(
                        format!(" · {}", message.date.format("%Y-%m-%d %H:%M")),
                        label_style,
                    ),
                    Span::styled(
                        if message.has_attachments {
                            " · attachment"
                        } else {
                            ""
                        },
                        label_style,
                    ),
                ])
                .style(row_style),
            );
            if !message.snippet.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("    ", label_style),
                    Span::styled(truncate(&message.snippet, 96), label_style),
                ]));
            }
        }
    }

    lines
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
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
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use mxr_core::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{SenderEmailReferenceData, SenderProfileData};
    use mxr_test_support::render_to_string;

    fn sample_profile() -> SenderProfileData {
        SenderProfileData {
            account_id: AccountId::new(),
            email: "alice@example.com".into(),
            display_name: Some("Alice Example".into()),
            first_seen_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            last_seen_at: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            last_inbound_at: Some(Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap()),
            last_outbound_at: Some(Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).unwrap()),
            total_inbound: 47,
            total_outbound: 22,
            replied_count: 21,
            cadence_days_p50: Some(3.5),
            is_list_sender: false,
            list_id: None,
            open_thread_count: 2,
            inbound_storage_bytes: 1_048_576,
            outbound_storage_bytes: 262_144,
            attachment_count: 3,
            attachment_bytes: 512_000,
            recent_messages: vec![SenderEmailReferenceData {
                message_id: MessageId::new(),
                thread_id: ThreadId::new(),
                subject: "Previous contract note".into(),
                snippet: "Can you send the signed copy?".into(),
                from_name: Some("Alice Example".into()),
                from_email: "alice@example.com".into(),
                date: Utc.with_ymd_and_hms(2026, 4, 30, 9, 0, 0).unwrap(),
                direction: "inbound".into(),
                has_attachments: true,
            }],
            relationship: None,
        }
    }

    #[test]
    fn loading_state_shows_placeholder() {
        let mut state = SenderProfileModalState::default();
        state.open_loading("alice@example.com".into(), None);
        let snapshot = render_to_string(80, 30, |frame| {
            draw(frame, Rect::new(0, 0, 80, 30), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Loading sender profile..."),
            "loading placeholder must appear; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("alice@example.com"),
            "loading title must show queried email; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_aggregates_when_profile_present() {
        let mut state = SenderProfileModalState::default();
        state.open_loading("alice@example.com".into(), None);
        state.set_profile(Some(sample_profile()));
        let snapshot = render_to_string(80, 30, |frame| {
            draw(frame, Rect::new(0, 0, 80, 30), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("47 inbound"),
            "inbound count must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("3.5 days"),
            "cadence p50 must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("2"),
            "open thread count must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Previous contract note"),
            "recent sender emails must surface; got:\n{snapshot}",
        );
    }

    #[test]
    fn unknown_sender_shows_empty_message() {
        let mut state = SenderProfileModalState::default();
        state.open_loading("nobody@example.com".into(), None);
        state.set_profile(None);
        let snapshot = render_to_string(80, 18, |frame| {
            draw(frame, Rect::new(0, 0, 80, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Sender unknown"),
            "unknown senders must surface a hint; got:\n{snapshot}",
        );
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
