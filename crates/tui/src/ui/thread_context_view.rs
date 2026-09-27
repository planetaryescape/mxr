//! The thread's context above the messages, from the daemon's
//! `GetThreadContext` and `GetThreadGist`: the gist and the ask, how you
//! know the person, whether you owe a reply, and open promises. Same
//! content as the web reader's context block and `mxr briefing context`.

use crate::theme::Theme;
use chrono::{DateTime, Local, Utc};
use mxr_protocol::{
    AiLocalityData, AiSourceData, CommitmentDirectionData, ThreadContextData, ThreadGistData,
    ThreadGistStatusData,
};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Label column width, so the values line up like `mxr briefing context`.
const LABEL_WIDTH: usize = 10;

pub fn context_lines(
    context: Option<&ThreadContextData>,
    gist: Option<&ThreadGistData>,
    theme: &Theme,
    now: DateTime<Utc>,
) -> Vec<Line<'static>> {
    let label = |text: &str, style: Style| {
        Span::styled(
            format!("{text:<LABEL_WIDTH$}"),
            style.add_modifier(Modifier::BOLD),
        )
    };
    let value = |text: String| Span::styled(text, Style::default().fg(theme.text_primary));
    let muted = |text: String| Span::styled(text, Style::default().fg(theme.text_muted));
    let mut lines = Vec::new();

    if let Some(gist) = gist.filter(|gist| gist.status == ThreadGistStatusData::Ready) {
        if let Some(text) = &gist.gist {
            lines.push(Line::from(vec![
                label("Gist", Style::default().fg(theme.accent)),
                value(text.clone()),
            ]));
        }
        match &gist.ask {
            Some(ask) => {
                lines.push(Line::from(vec![
                    label("Asks you", Style::default().fg(theme.warning)),
                    value(ask.summary.clone()),
                ]));
                if let Some(quote) = &ask.quote {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(LABEL_WIDTH)),
                        Span::styled(
                            format!("\"{}\"", quote.text),
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }
            }
            None => lines.push(Line::from(vec![
                label("Asks you", Style::default().fg(theme.text_muted)),
                muted("nothing".into()),
            ])),
        }
    }

    if let Some(context) = context {
        let mut facts = Vec::new();
        if let Some(person) = &context.counterparty {
            let name = person
                .display_name
                .as_deref()
                .and_then(|name| name.split_whitespace().next())
                .unwrap_or(&person.email)
                .to_string();
            if person.bulk_sender {
                facts.push(format!(
                    "bulk mail from {name}: {}",
                    plural(person.messages_from_them, "email")
                ));
            } else {
                facts.push(format!(
                    "you and {name}: {}",
                    plural(
                        person.messages_from_them + person.messages_from_you,
                        "email"
                    )
                ));
                if let Some(seconds) = person.your_reply_p50_seconds {
                    facts.push(format!("you usually reply within {}", duration(seconds)));
                }
                if let Some(seconds) = person.their_reply_p50_seconds {
                    facts.push(format!(
                        "{name} usually replies within {}",
                        duration(seconds)
                    ));
                }
                facts.push(match person.last_contact_elsewhere_at {
                    Some(when) => format!("last spoke {}", day(when, now)),
                    None => "your first conversation".to_string(),
                });
            }
        }
        if let Some(owed) = &context.owed_reply {
            facts.push(format!("you owe a reply since {}", day(owed.since, now)));
        }
        if !facts.is_empty() {
            lines.push(Line::from(vec![
                label("With", Style::default().fg(theme.text_secondary)),
                muted(facts.join(" · ")),
            ]));
        }
        let their_name = context
            .counterparty
            .as_ref()
            .and_then(|person| person.display_name.as_deref())
            .and_then(|name| name.split_whitespace().next())
            .map(str::to_string);
        for (index, commitment) in context.commitments.iter().enumerate() {
            let who = match commitment.direction {
                CommitmentDirectionData::Yours => "You".to_string(),
                CommitmentDirectionData::Theirs => their_name
                    .clone()
                    .unwrap_or_else(|| commitment.who_owes.clone()),
            };
            let due = commitment
                .by_when
                .map(|when| format!(", due {}", day(when, now)))
                .unwrap_or_default();
            lines.push(Line::from(vec![
                label(
                    if index == 0 { "Promises" } else { "" },
                    Style::default().fg(theme.text_secondary),
                ),
                value(format!("{who} promised: {}{due}", commitment.what)),
            ]));
        }
    }

    if let Some(provenance) = gist
        .filter(|gist| gist.status == ThreadGistStatusData::Ready)
        .and_then(|gist| gist.provenance.as_ref())
    {
        let place = match provenance.locality {
            AiLocalityData::Local => "local model",
            AiLocalityData::Cloud => "cloud model",
        };
        let sources = if provenance
            .sources
            .contains(&AiSourceData::RelationshipHistory)
        {
            "from this thread and your history with them"
        } else {
            "from this thread only"
        };
        lines.push(Line::from(vec![
            label("AI", Style::default().fg(theme.text_muted)),
            muted(format!("{place} {} · {sources}", provenance.model)),
        ]));
    }
    lines
}

fn plural(count: u32, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn duration(seconds: u32) -> String {
    let minutes = seconds.div_ceil(60).max(1);
    if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 48 * 60 {
        format!("{}h", minutes.div_ceil(60))
    } else {
        format!("{}d", minutes.div_ceil(24 * 60))
    }
}

fn day(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let date = when.with_timezone(&Local).date_naive();
    let today = now.with_timezone(&Local).date_naive();
    match (today - date).num_days() {
        0 => "today".into(),
        1 => "yesterday".into(),
        2..=6 => when.with_timezone(&Local).format("%a").to_string(),
        _ if date.format("%Y").to_string() == today.format("%Y").to_string() => {
            when.with_timezone(&Local).format("%-d %b").to_string()
        }
        _ => when.with_timezone(&Local).format("%-d %b %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{
        AiProvenanceData, OwedReplyHereData, ThreadAskData, ThreadCounterpartyData,
        VerifiedQuoteData,
    };

    fn text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn facts() -> ThreadContextData {
        ThreadContextData {
            thread_id: ThreadId::new(),
            account_id: AccountId::new(),
            counterparty: Some(ThreadCounterpartyData {
                email: "maya@example.com".into(),
                display_name: Some("Maya Ortiz".into()),
                messages_from_them: 23,
                messages_from_you: 18,
                your_reply_p50_seconds: Some(4 * 3600),
                your_reply_samples: 9,
                their_reply_p50_seconds: None,
                their_reply_samples: 0,
                last_contact_elsewhere_at: None,
                bulk_sender: false,
            }),
            owed_reply: Some(OwedReplyHereData {
                message_id: MessageId::new(),
                since: Utc::now(),
            }),
            commitments: vec![],
        }
    }

    #[test]
    fn facts_render_without_a_model() {
        let lines = context_lines(Some(&facts()), None, &Theme::default(), Utc::now());
        assert_eq!(
            text(&lines),
            "With      you and Maya: 41 emails · you usually reply within 4h · your first conversation · you owe a reply since today"
        );
    }

    #[test]
    fn a_ready_gist_leads_with_the_ask_and_ends_with_its_source() {
        let context = facts();
        let gist = ThreadGistData {
            thread_id: context.thread_id.clone(),
            status: ThreadGistStatusData::Ready,
            gist: Some("Canary stays at 5%.".into()),
            ask: Some(ThreadAskData {
                summary: "confirm the owner".into(),
                quote: Some(VerifiedQuoteData {
                    message_id: MessageId::new(),
                    text: "Who owns it?".into(),
                }),
            }),
            provenance: Some(AiProvenanceData {
                model: "qwen2.5".into(),
                locality: AiLocalityData::Cloud,
                sources: vec![AiSourceData::ThisThread],
            }),
            reason: None,
            generated_at: None,
            from_cache: true,
        };
        let rendered = text(&context_lines(
            Some(&context),
            Some(&gist),
            &Theme::default(),
            Utc::now(),
        ));
        let lines: Vec<_> = rendered.lines().collect();
        assert_eq!(lines[0], "Gist      Canary stays at 5%.");
        assert_eq!(lines[1], "Asks you  confirm the owner");
        assert_eq!(lines[2], "          \"Who owns it?\"");
        assert_eq!(
            *lines.last().unwrap(),
            "AI        cloud model qwen2.5 · from this thread only"
        );
        assert!(!rendered.contains('—'));
    }
}
