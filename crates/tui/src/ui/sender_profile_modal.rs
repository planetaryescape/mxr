use super::centered_rect;
use crate::app::{SenderProfileModalState, SenderProfileTab};
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

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = match &state.email {
        Some(email) => {
            format!(" Sender · {email} · 1/2/3 tabs · j/k select · Enter open · Esc close ")
        }
        None => " Sender · 1/2/3 tabs · j/k select · Enter open · Esc close ".to_string(),
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

    lines.push(tab_line(state.active_tab, theme));
    lines.push(Line::from(""));

    match state.active_tab {
        SenderProfileTab::Overview => push_overview_lines(&mut lines, profile, theme, label_style),
        SenderProfileTab::Relationship => {
            push_relationship_lines(&mut lines, profile, theme, label_style);
        }
        SenderProfileTab::Messages => push_message_lines(&mut lines, state, theme, label_style),
    }

    lines
}

fn tab_line<'a>(active: SenderProfileTab, theme: &Theme) -> Line<'a> {
    let tab = |tab: SenderProfileTab, label: &'static str| {
        let style = if active == tab {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_muted)
        };
        Span::styled(label, style)
    };
    Line::from(vec![
        tab(SenderProfileTab::Overview, "[1 Overview]"),
        Span::raw("  "),
        tab(SenderProfileTab::Relationship, "[2 Relationship]"),
        Span::raw("  "),
        tab(SenderProfileTab::Messages, "[3 Messages]"),
    ])
}

fn push_overview_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    profile: &'a mxr_protocol::SenderProfileData,
    theme: &Theme,
    label_style: Style,
) {
    lines.push(Line::from(Span::styled(
        "Overview",
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )));

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
    if let Some(question) = &profile.unanswered_question {
        lines.push(Line::from(vec![
            Span::styled("Unanswered Q: ", label_style),
            Span::styled(
                format!("{}d · {}", question.days_waiting, question.subject),
                Style::default().fg(theme.warning),
            ),
        ]));
    }
    let reply_histogram = reply_histogram_summary(&profile.response_histogram);
    if !reply_histogram.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Reply latency: ", label_style),
            Span::raw(reply_histogram),
        ]));
    }
    if !profile.weekly_activity.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Weekly trend: ", label_style),
            Span::raw(activity_sparkline(&profile.weekly_activity)),
        ]));
    }
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
        "Press 2 for relationship identity; 3 for recent sender messages.",
        label_style,
    )));
}

fn activity_sparkline(weeks: &[mxr_protocol::SenderWeeklyActivityData]) -> String {
    const STEPS: &[u8] = b" .:-=+*#";
    let totals: Vec<u32> = weeks
        .iter()
        .map(|week| week.inbound_count + week.outbound_count)
        .collect();
    let max = totals.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return ".".repeat(weeks.len());
    }
    totals
        .into_iter()
        .map(|count| {
            let index = ((count as usize) * (STEPS.len() - 1)) / (max as usize);
            STEPS[index] as char
        })
        .collect()
}

fn reply_histogram_summary(buckets: &[mxr_core::types::ResponseTimeBucket]) -> String {
    buckets
        .iter()
        .filter(|bucket| bucket.count > 0)
        .map(|bucket| {
            format!(
                "{}:{}",
                reply_histogram_label(bucket.upper_bound_seconds),
                bucket.count
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn reply_histogram_label(upper_bound_seconds: u32) -> &'static str {
    match upper_bound_seconds {
        60 => "<1m",
        300 => "<5m",
        1_800 => "<30m",
        3_600 => "<1h",
        21_600 => "<6h",
        86_400 => "<1d",
        259_200 => "<3d",
        u32::MAX => ">=3d",
        _ => "?",
    }
}

fn push_relationship_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    profile: &'a mxr_protocol::SenderProfileData,
    theme: &Theme,
    label_style: Style,
) {
    lines.push(Line::from(Span::styled(
        "Relationship",
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )));

    if let Some(relationship) = &profile.relationship {
        if let Some(drift) = &relationship.drift {
            lines.push(Line::from(Span::styled(
                format!("Voice drift: {}", drift.reason),
                Style::default().fg(theme.warning),
            )));
        }
        if let Some(style) = &relationship.style {
            lines.push(Line::from(vec![
                Span::styled("Your voice: ", label_style),
                Span::raw(format!(
                    "{} · {:.1} words · {} samples",
                    formality_label(style.formality_score),
                    style.avg_sentence_len,
                    style.msg_count_used
                )),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Their voice: ", label_style),
                Span::raw(format!(
                    "{} · {:.1} words · {} samples",
                    formality_label(style.formality_score_theirs),
                    style.avg_sentence_len_theirs,
                    style.msg_count_used_theirs
                )),
            ]));
        }
        if let Some(summary) = &relationship.summary {
            lines.push(Line::from(vec![
                Span::styled("Summary: ", label_style),
                Span::raw(truncate(summary.text.trim(), 120)),
            ]));
            if !summary.known_topics.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Known topics: ", label_style),
                    Span::raw(truncate(&summary.known_topics.join(", "), 120)),
                ]));
            }
        }
        if !relationship.open_commitments.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Open commitments: ", label_style),
                Span::raw(relationship.open_commitments.len().to_string()),
            ]));
            for commitment in relationship.open_commitments.iter().take(3) {
                lines.push(Line::from(vec![
                    Span::styled("  - ", label_style),
                    Span::raw(truncate(&commitment.what, 96)),
                ]));
            }
        }
    } else if profile.is_list_sender {
        lines.push(Line::from(Span::styled(
            "List sender: relationship identity is intentionally skipped.",
            Style::default().fg(theme.warning),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No relationship profile yet. Rebuild after more local messages.",
            label_style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press 1 for volume/cadence; 3 for local message history.",
        label_style,
    )));
}

fn push_message_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    state: &'a SenderProfileModalState,
    theme: &Theme,
    label_style: Style,
) {
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

fn formality_label(score: f64) -> &'static str {
    if score < 0.4 {
        "casual"
    } else if score < 0.7 {
        "neutral"
    } else {
        "formal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use mxr_core::{types::ResponseTimeBucket, AccountId, MessageId, ThreadId};
    use mxr_protocol::{
        CommitmentData, CommitmentDirectionData, CommitmentStatusData,
        ContactRelationshipSummaryData, ContactStyleData, RelationshipDriftData,
        RelationshipProfileData, SenderEmailReferenceData, SenderProfileData,
        SenderUnansweredQuestionData, SenderWeeklyActivityData,
    };
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
            unanswered_question: Some(SenderUnansweredQuestionData {
                message_id: MessageId::new(),
                thread_id: ThreadId::new(),
                subject: "Can you send the signed copy?".into(),
                received_at: Utc.with_ymd_and_hms(2026, 4, 30, 9, 0, 0).unwrap(),
                days_waiting: 2,
            }),
            response_histogram: vec![
                ResponseTimeBucket {
                    upper_bound_seconds: 86_400,
                    count: 2,
                },
                ResponseTimeBucket {
                    upper_bound_seconds: u32::MAX,
                    count: 1,
                },
            ],
            weekly_activity: vec![
                SenderWeeklyActivityData {
                    week_start: Utc.with_ymd_and_hms(2026, 4, 20, 0, 0, 0).unwrap(),
                    inbound_count: 1,
                    outbound_count: 0,
                },
                SenderWeeklyActivityData {
                    week_start: Utc.with_ymd_and_hms(2026, 4, 27, 0, 0, 0).unwrap(),
                    inbound_count: 2,
                    outbound_count: 1,
                },
            ],
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
            snapshot.contains("Unanswered Q"),
            "question signal must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Reply latency"),
            "reply histogram must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Weekly trend"),
            "weekly activity sparkline must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("[1 Overview]"),
            "overview tab label must surface; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_recent_messages_on_messages_tab() {
        let mut state = SenderProfileModalState::default();
        state.open_loading("alice@example.com".into(), None);
        state.set_profile(Some(sample_profile()));
        state.select_tab(SenderProfileTab::Messages);

        let snapshot = render_to_string(100, 30, |frame| {
            draw(frame, Rect::new(0, 0, 100, 30), &state, &Theme::default());
        });

        assert!(
            snapshot.contains("Other emails from sender"),
            "messages tab heading must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Previous contract note"),
            "recent sender emails must surface; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_relationship_voice_topics_and_commitments() {
        let mut profile = sample_profile();
        let account_id = profile.account_id.clone();
        profile.relationship = Some(RelationshipProfileData {
            account_id: account_id.clone(),
            email: "alice@example.com".into(),
            style: Some(ContactStyleData {
                formality_score: 0.72,
                formality_score_theirs: 0.35,
                avg_sentence_len: 12.4,
                avg_sentence_len_theirs: 8.1,
                msg_count_used: 8,
                msg_count_used_theirs: 5,
                computed_at: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
                source_hash: "hash".into(),
            }),
            summary: Some(ContactRelationshipSummaryData {
                text: "Contract-heavy collaborator with recurring launch planning.".into(),
                model: "local".into(),
                known_topics: vec!["contract".into(), "launch".into()],
                computed_at: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
                source_hash: "summary-hash".into(),
            }),
            open_commitments: vec![CommitmentData {
                id: "commitment-1".into(),
                account_id,
                email: "alice@example.com".into(),
                thread_id: ThreadId::new(),
                direction: CommitmentDirectionData::Yours,
                status: CommitmentStatusData::Open,
                who_owes: "me".into(),
                what: "Send signed contract copy".into(),
                by_when: None,
                evidence_msg_id: MessageId::new(),
                extracted_at: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
            }],
            drift: Some(RelationshipDriftData {
                detected_at: Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap(),
                reason: "your formality changed".into(),
            }),
        });
        let mut state = SenderProfileModalState::default();
        state.open_loading("alice@example.com".into(), None);
        state.set_profile(Some(profile));
        state.select_tab(SenderProfileTab::Relationship);

        let snapshot = render_to_string(100, 36, |frame| {
            draw(frame, Rect::new(0, 0, 100, 36), &state, &Theme::default());
        });

        assert!(snapshot.contains("Relationship"), "got:\n{snapshot}");
        assert!(snapshot.contains("Voice drift"), "got:\n{snapshot}");
        assert!(snapshot.contains("Your voice"), "got:\n{snapshot}");
        assert!(snapshot.contains("Known topics"), "got:\n{snapshot}");
        assert!(
            snapshot.contains("Send signed contract copy"),
            "got:\n{snapshot}"
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
