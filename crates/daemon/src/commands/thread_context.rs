//! `mxr briefing context <thread-id>`: the reader's context block on the
//! command line. Facts come first from `GetThreadContext`; the gist and the
//! ask come from `GetThreadGist` unless `--no-ai`.

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use chrono::{DateTime, Local, Utc};
use mxr_core::ThreadId;
use mxr_protocol::{
    AiLocalityData, AiProvenanceData, AiSourceData, CommitmentDirectionData, Request, Response,
    ResponseData, ThreadContextData, ThreadGistData, ThreadGistStatusData,
};

pub async fn run(
    client: &mut IpcClient,
    thread_id: ThreadId,
    with_ai: bool,
    refresh: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let context = match client
        .request(Request::GetThreadContext {
            thread_id: thread_id.clone(),
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::ThreadContext { context },
        } => context,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response to GetThreadContext"),
    };
    let gist = if with_ai {
        match client
            .request(Request::GetThreadGist { thread_id, refresh })
            .await?
        {
            Response::Ok {
                data: ResponseData::ThreadGist { gist },
            } => Some(gist),
            Response::Error { message, .. } => anyhow::bail!(message),
            _ => anyhow::bail!("Unexpected response to GetThreadGist"),
        }
    } else {
        None
    };

    let payload = serde_json::json!({ "context": context, "gist": gist });
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&payload)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&payload)?),
        _ => print!("{}", render(&context, gist.as_ref(), Utc::now())),
    }
    Ok(())
}

/// The human view: one labelled line per fact, the way the reader shows it.
pub(crate) fn render(
    context: &ThreadContextData,
    gist: Option<&ThreadGistData>,
    now: DateTime<Utc>,
) -> String {
    let mut out = String::new();
    let mut line = |label: &str, text: &str| out.push_str(&format!("{label:<10}{text}\n"));

    if let Some(gist) = gist.filter(|gist| gist.status == ThreadGistStatusData::Ready) {
        if let Some(text) = &gist.gist {
            line("Gist", text);
        }
        match &gist.ask {
            Some(ask) => {
                line("Asks you", &ask.summary);
                if let Some(quote) = &ask.quote {
                    line("", &format!("\"{}\"", quote.text));
                }
            }
            None => line("Asks you", "nothing"),
        }
    }

    if let Some(person) = &context.counterparty {
        let name = person.display_name.as_deref().unwrap_or(&person.email);
        line("With", &relationship_line(name, person, now));
    }
    if let Some(owed) = &context.owed_reply {
        line(
            "You owe",
            &format!("a reply since {}", short_date(owed.since, now)),
        );
    }
    for (index, commitment) in context.commitments.iter().enumerate() {
        let who = match commitment.direction {
            CommitmentDirectionData::Yours => "you".to_string(),
            CommitmentDirectionData::Theirs => context
                .counterparty
                .as_ref()
                .and_then(|person| person.display_name.clone())
                .unwrap_or_else(|| commitment.who_owes.clone()),
        };
        let due = commitment
            .by_when
            .map(|when| format!(", due {}", short_date(when, now)))
            .unwrap_or_default();
        line(
            if index == 0 { "Promises" } else { "" },
            &format!("{who}: {}{due}", commitment.what),
        );
    }

    if let Some(gist) = gist {
        match (gist.status, &gist.provenance) {
            (ThreadGistStatusData::Ready, Some(provenance)) => {
                line("AI", &provenance_line(provenance));
            }
            (ThreadGistStatusData::Disabled, _) => {}
            (_, _) => line(
                "AI",
                &format!(
                    "no gist: {}",
                    gist.reason.as_deref().unwrap_or("the model gave no answer")
                ),
            ),
        }
    }
    out
}

fn relationship_line(
    name: &str,
    person: &mxr_protocol::ThreadCounterpartyData,
    now: DateTime<Utc>,
) -> String {
    let total = person.messages_from_them + person.messages_from_you;
    let mut parts = vec![format!(
        "{name} <{}>, {} ({} from them, {} from you)",
        person.email,
        plural(total, "email"),
        person.messages_from_them,
        person.messages_from_you
    )];
    if let Some(seconds) = person.your_reply_p50_seconds {
        parts.push(format!("you usually reply within {}", duration(seconds)));
    }
    if let Some(seconds) = person.their_reply_p50_seconds {
        parts.push(format!("they usually reply within {}", duration(seconds)));
    }
    parts.push(match person.last_contact_elsewhere_at {
        Some(when) => format!("last spoke {}", short_date(when, now)),
        None => "first conversation".to_string(),
    });
    if person.bulk_sender {
        parts.push("bulk sender".to_string());
    }
    parts.join(" · ")
}

pub(crate) fn provenance_line(provenance: &AiProvenanceData) -> String {
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

fn plural(count: u32, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// "35m", "4h", "2d": a median reply time at a glance.
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

fn short_date(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let local = when.with_timezone(&Local);
    if local.date_naive() == now.with_timezone(&Local).date_naive() {
        format!("today {}", local.format("%H:%M"))
    } else if local.format("%Y").to_string() == now.with_timezone(&Local).format("%Y").to_string() {
        local.format("%a %-d %b").to_string()
    } else {
        local.format("%-d %b %Y").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId};
    use mxr_protocol::{
        CommitmentData, CommitmentStatusData, OwedReplyHereData, ThreadAskData,
        ThreadCounterpartyData, VerifiedQuoteData,
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
            commitments: vec![CommitmentData {
                id: "c1".into(),
                account_id,
                email: "maya@example.com".into(),
                thread_id,
                direction: CommitmentDirectionData::Yours,
                status: CommitmentStatusData::Open,
                who_owes: "you".into(),
                what: "send the runbook link".into(),
                by_when: None,
                evidence_msg_id: MessageId::new(),
                extracted_at: Utc::now(),
            }],
        }
    }

    #[test]
    fn facts_only_output_names_the_person_the_owed_reply_and_promises() {
        let text = render(&facts(), None, Utc::now());
        assert!(text.contains(
            "With      Maya Ortiz <maya@example.com>, 41 emails (23 from them, 18 from you) · you usually reply within 4h · first conversation"
        ), "{text}");
        assert!(text.contains("You owe   a reply since today"), "{text}");
        assert!(
            text.contains("Promises  you: send the runbook link"),
            "{text}"
        );
        assert!(!text.contains("Gist"));
        assert!(!text.contains('—'), "no em dashes in copy");
    }

    #[test]
    fn ready_gist_shows_the_ask_its_quote_and_provenance() {
        let context = facts();
        let gist = ThreadGistData {
            thread_id: context.thread_id.clone(),
            status: ThreadGistStatusData::Ready,
            gist: Some("Canary stays at 5%.".into()),
            ask: Some(ThreadAskData {
                summary: "confirm who owns the rollout check".into(),
                quote: Some(VerifiedQuoteData {
                    message_id: MessageId::new(),
                    text: "Can you confirm who owns it?".into(),
                }),
            }),
            provenance: Some(AiProvenanceData {
                model: "qwen2.5:7b".into(),
                locality: AiLocalityData::Local,
                sources: vec![AiSourceData::ThisThread],
            }),
            reason: None,
            generated_at: Some(Utc::now()),
            from_cache: true,
        };
        let text = render(&context, Some(&gist), Utc::now());
        assert!(text.starts_with("Gist      Canary stays at 5%.\nAsks you  confirm who owns the rollout check\n          \"Can you confirm who owns it?\"\n"), "{text}");
        assert!(
            text.ends_with("AI        local model qwen2.5:7b · from this thread only\n"),
            "{text}"
        );
    }

    #[test]
    fn disabled_model_adds_nothing() {
        let context = facts();
        let gist = ThreadGistData {
            thread_id: context.thread_id.clone(),
            status: ThreadGistStatusData::Disabled,
            gist: None,
            ask: None,
            provenance: None,
            reason: Some("No language model is configured.".into()),
            generated_at: None,
            from_cache: false,
        };
        assert_eq!(
            render(&context, Some(&gist), Utc::now()),
            render(&context, None, Utc::now())
        );
    }

    #[test]
    fn durations_read_at_a_glance() {
        assert_eq!(duration(20), "1m");
        assert_eq!(duration(35 * 60), "35m");
        assert_eq!(duration(4 * 3600), "4h");
        assert_eq!(duration(3 * 86_400), "3d");
    }
}
