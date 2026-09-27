//! The words for a thread's context, shared by the terminal clients: the TUI
//! styles these rows and `mxr briefing context` prints them, so both say the
//! same thing. They live here, not in `mxr-protocol`, because they describe
//! a screen. (The web reader states the same facts from its own
//! TypeScript copy, `contextFormat.ts`, laid out for the page.)

use chrono::{DateTime, Local, Utc};
use mxr_protocol::{
    AiLocalityData, AiProvenanceData, AiSourceData, ThreadContextData, ThreadCounterpartyData,
    ThreadGistData, ThreadGistStatusData,
};

/// What a row is, so a client can style it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextRowKind {
    Gist,
    Ask,
    /// The ask's verified quote, under the ask.
    Quote,
    /// No ask ("nothing"), or a gist that isn't there.
    Quiet,
    Fact,
    Promise,
    /// Which model wrote the gist and what it read.
    Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextRow {
    /// "Gist", "Asks you", "With", "You owe", "Promises", "AI", or empty for
    /// a continuation row.
    pub label: &'static str,
    pub kind: ContextRowKind,
    pub text: String,
}

/// Rows for a thread's context, gist first. A gist that isn't `ready`
/// contributes nothing, except a failure or privacy block, which says so on
/// the `AI` row; no model at all adds nothing.
pub fn context_rows(
    context: Option<&ThreadContextData>,
    gist: Option<&ThreadGistData>,
    now: DateTime<Utc>,
) -> Vec<ContextRow> {
    let mut rows = Vec::new();
    let mut push = |label, kind, text: String| rows.push(ContextRow { label, kind, text });
    let ready = gist.filter(|gist| gist.status == ThreadGistStatusData::Ready);

    if let Some(gist) = ready {
        if let Some(text) = &gist.gist {
            push("Gist", ContextRowKind::Gist, text.clone());
        }
        match &gist.ask {
            Some(ask) => {
                push(
                    "Asks you",
                    ContextRowKind::Ask,
                    format!("to {}", ask.summary),
                );
                if let Some(quote) = &ask.quote {
                    push("", ContextRowKind::Quote, format!("\"{}\"", quote.text));
                }
            }
            None => push("Asks you", ContextRowKind::Quiet, "nothing".into()),
        }
    }

    if let Some(context) = context {
        if let Some(person) = &context.counterparty {
            push("With", ContextRowKind::Fact, relationship(person, now));
        }
        if let Some(owed) = &context.owed_reply {
            push(
                "You owe",
                ContextRowKind::Fact,
                format!("a reply since {}", day_label(owed.since, now)),
            );
        }
        for (index, promise) in context.promises.iter().enumerate() {
            let commitment = &promise.commitment;
            let who = owner_label(&promise.owner);
            let due = commitment
                .by_when
                .map(|when| format!(", due {}", day_label(when, now)))
                .unwrap_or_default();
            push(
                if index == 0 { "Promises" } else { "" },
                ContextRowKind::Promise,
                format!("{who} promised: {}{due}", commitment.what),
            );
        }
    }

    match gist {
        Some(gist) if gist.status == ThreadGistStatusData::Ready => {
            if let Some(provenance) = &gist.provenance {
                push("AI", ContextRowKind::Source, provenance_text(provenance));
            }
        }
        Some(gist) if gist.status != ThreadGistStatusData::Disabled => push(
            "AI",
            ContextRowKind::Source,
            format!(
                "no gist: {}",
                gist.reason.as_deref().unwrap_or("the model gave no answer")
            ),
        ),
        _ => {}
    }
    rows
}

/// "Maya (maya@example.com): 41 emails · you usually reply within 4h ·
/// last spoke 12 Sep".
fn relationship(person: &ThreadCounterpartyData, now: DateTime<Utc>) -> String {
    let name = first_name(person);
    let who = if name == person.email {
        name.clone()
    } else {
        format!("{name} ({})", person.email)
    };
    if person.bulk_sender {
        return format!(
            "bulk mail from {who}: {}",
            plural(person.messages_from_them, "email")
        );
    }
    let mut parts = vec![format!(
        "{who}: {}",
        plural(
            person
                .messages_from_them
                .saturating_add(person.messages_from_you),
            "email"
        )
    )];
    if let Some(seconds) = person.your_reply_p50_seconds {
        parts.push(format!("you usually reply within {}", reply_time(seconds)));
    }
    if let Some(seconds) = person.their_reply_p50_seconds {
        parts.push(format!(
            "{name} usually replies within {}",
            reply_time(seconds)
        ));
    }
    parts.push(match person.last_contact_elsewhere_at {
        Some(when) => format!("last spoke {}", day_label(when, now)),
        None => "your first conversation".to_string(),
    });
    parts.join(" · ")
}

/// "local model qwen2.5 · from this thread only".
fn provenance_text(provenance: &AiProvenanceData) -> String {
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
    format!("{place} {} · {sources}", provenance.model)
}

/// "Maya" from "Maya Ortiz"; the address when there is no name.
fn first_name(person: &ThreadCounterpartyData) -> String {
    person
        .display_name
        .as_deref()
        .map_or_else(|| person.email.clone(), first_word)
}

/// How a promise's owner reads: "You", "Alice" from "Alice Park", or an
/// address kept whole. The daemon already resolved who it is.
fn owner_label(owner: &str) -> String {
    if owner.contains('@') {
        return owner.to_string();
    }
    let word = first_word(owner);
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// "Maya" from "Maya Ortiz"; an address stays whole.
fn first_word(name: &str) -> String {
    if name.contains('@') {
        return name.to_string();
    }
    name.split_whitespace().next().unwrap_or(name).to_string()
}

fn plural(count: u32, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// A median reply time at a glance: "35m", "4h", "2d".
fn reply_time(seconds: u32) -> String {
    let minutes = seconds.div_ceil(60).max(1);
    if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 48 * 60 {
        format!("{}h", minutes.div_ceil(60))
    } else {
        format!("{}d", minutes.div_ceil(24 * 60))
    }
}

/// "today", "yesterday", "Thu", "12 Sep", "12 Sep 2025", in local time.
fn day_label(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let local = when.with_timezone(&Local);
    let today = now.with_timezone(&Local).date_naive();
    match (today - local.date_naive()).num_days() {
        0 => "today".into(),
        1 => "yesterday".into(),
        2..=6 => local.format("%a").to_string(),
        _ if local.date_naive().format("%Y").to_string() == today.format("%Y").to_string() => {
            local.format("%-d %b").to_string()
        }
        _ => local.format("%-d %b %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{
        CommitmentData, CommitmentDirectionData, CommitmentStatusData, OwedReplyHereData,
        ThreadAskData, ThreadPromiseData, VerifiedQuoteData,
    };

    fn facts() -> ThreadContextData {
        let thread_id = ThreadId::new();
        let account_id = AccountId::new();
        ThreadContextData {
            thread_id: thread_id.clone(),
            account_id: account_id.clone(),
            counterparty: Some(ThreadCounterpartyData {
                email: "maya@example.com".into(),
                display_name: Some("Maya Ortiz".into()),
                messages_from_them: 23,
                messages_from_you: 18,
                your_reply_p50_seconds: Some(4 * 3600 - 100),
                your_reply_samples: 12,
                their_reply_p50_seconds: None,
                their_reply_samples: 0,
                last_contact_elsewhere_at: None,
                bulk_sender: false,
            }),
            owed_reply: Some(OwedReplyHereData {
                message_id: MessageId::new(),
                since: Utc::now(),
            }),
            // A group thread: Maya is the main counterparty, but the two
            // promises are Alice's and Bob's.
            promises: [
                ("Alice Park", "alice@example.com", "share the dashboard"),
                ("bob@example.com", "bob@example.com", "send the logs"),
            ]
            .into_iter()
            .map(|(owner, email, what)| ThreadPromiseData {
                owner: owner.into(),
                commitment: CommitmentData {
                    id: what.into(),
                    account_id: account_id.clone(),
                    email: email.into(),
                    thread_id: thread_id.clone(),
                    direction: CommitmentDirectionData::Theirs,
                    status: CommitmentStatusData::Open,
                    who_owes: email.into(),
                    what: what.into(),
                    by_when: None,
                    evidence_msg_id: MessageId::new(),
                    extracted_at: Utc::now(),
                },
            })
            .collect(),
        }
    }

    fn gist(status: ThreadGistStatusData) -> ThreadGistData {
        ThreadGistData {
            thread_id: ThreadId::new(),
            status,
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
                locality: AiLocalityData::Local,
                sources: vec![AiSourceData::ThisThread, AiSourceData::RelationshipHistory],
            }),
            reason: Some("timed out".into()),
            generated_at: None,
            from_cache: false,
        }
    }

    fn texts(rows: &[ContextRow]) -> Vec<String> {
        rows.iter()
            .map(|row| format!("{}|{}", row.label, row.text))
            .collect()
    }

    #[test]
    fn facts_only_rows_name_the_person_the_owed_reply_and_promises() {
        let rows = context_rows(Some(&facts()), None, Utc::now());
        assert_eq!(
            texts(&rows),
            vec![
                "With|Maya (maya@example.com): 41 emails · you usually reply within 4h · your first conversation",
                "You owe|a reply since today",
                "Promises|Alice promised: share the dashboard",
                "|bob@example.com promised: send the logs",
            ]
        );
    }

    #[test]
    fn a_ready_gist_leads_and_its_source_closes() {
        let rows = context_rows(
            Some(&facts()),
            Some(&gist(ThreadGistStatusData::Ready)),
            Utc::now(),
        );
        let texts = texts(&rows);
        assert_eq!(texts[0], "Gist|Canary stays at 5%.");
        assert_eq!(texts[1], "Asks you|to confirm the owner");
        assert_eq!(texts[2], "|\"Who owns it?\"");
        assert_eq!(
            texts.last().map(String::as_str).unwrap_or_default(),
            "AI|local model qwen2.5 · from this thread and your history with them"
        );
        assert!(!texts.concat().contains('—'), "no em dashes in copy");
    }

    #[test]
    fn no_model_adds_nothing_and_a_failure_says_why() {
        let facts = facts();
        let plain = context_rows(Some(&facts), None, Utc::now());
        assert_eq!(
            context_rows(
                Some(&facts),
                Some(&gist(ThreadGistStatusData::Disabled)),
                Utc::now()
            ),
            plain
        );
        let failed = context_rows(
            Some(&facts),
            Some(&gist(ThreadGistStatusData::Failed)),
            Utc::now(),
        );
        assert_eq!(
            texts(&failed).last().map(String::as_str),
            Some("AI|no gist: timed out")
        );
    }

    #[test]
    fn reply_times_and_days_read_at_a_glance() {
        assert_eq!(reply_time(20), "1m");
        assert_eq!(reply_time(35 * 60), "35m");
        assert_eq!(reply_time(4 * 3600), "4h");
        assert_eq!(reply_time(3 * 86_400), "3d");
        let now = Utc::now();
        assert_eq!(day_label(now, now), "today");
        assert_eq!(day_label(now - chrono::Duration::days(1), now), "yesterday");
    }
}
