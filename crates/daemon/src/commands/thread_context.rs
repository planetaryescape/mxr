//! `mxr briefing context <thread-id>`: the reader's context block on the
//! command line. Facts come first from `GetThreadContext`; the gist and the
//! ask come from `GetThreadGist` unless `--no-ai`.

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use chrono::{DateTime, Utc};
use mxr_core::ThreadId;
use mxr_protocol::{Request, Response, ResponseData, ThreadContextData, ThreadGistData};
use mxr_tui::thread_context_rows::context_rows;

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

/// The human view: one labelled line per row, in the words the TUI uses.
pub(crate) fn render(
    context: &ThreadContextData,
    gist: Option<&ThreadGistData>,
    now: DateTime<Utc>,
) -> String {
    context_rows(Some(context), gist, now)
        .into_iter()
        .map(|row| format!("{:<10}{}\n", row.label, row.text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId};
    use mxr_protocol::{
        AiLocalityData, AiProvenanceData, AiSourceData, ThreadAskData, ThreadCounterpartyData,
        ThreadGistStatusData, VerifiedQuoteData,
    };

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
                your_reply_samples: 12,
                their_reply_p50_seconds: None,
                their_reply_samples: 0,
                last_contact_elsewhere_at: None,
                bulk_sender: false,
            }),
            owed_reply: None,
            promises: vec![],
        }
    }

    #[test]
    fn rows_print_as_aligned_labelled_lines() {
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
            newest_message_id: None,
        };
        assert_eq!(
            render(&context, Some(&gist), Utc::now()),
            "Gist      Canary stays at 5%.\n\
             Asks you  to confirm who owns the rollout check\n          \"Can you confirm who owns it?\"\n\
             With      Maya (maya@example.com): 41 emails · you usually reply within 4h · your first conversation\n\
             AI        local model qwen2.5:7b · from this thread only\n"
        );
    }

    #[test]
    fn group_thread_promises_keep_their_own_owners() {
        let mut context = facts();
        let promise = |owner: &str, email: &str, what: &str| mxr_protocol::ThreadPromiseData {
            owner: owner.into(),
            commitment: mxr_protocol::CommitmentData {
                id: what.into(),
                account_id: context.account_id.clone(),
                email: email.into(),
                thread_id: context.thread_id.clone(),
                direction: mxr_protocol::CommitmentDirectionData::Theirs,
                status: mxr_protocol::CommitmentStatusData::Open,
                who_owes: email.into(),
                what: what.into(),
                by_when: None,
                evidence_msg_id: MessageId::new(),
                extracted_at: Utc::now(),
            },
        };
        let promises = vec![
            promise("Alice Park", "alice@example.com", "share the dashboard"),
            promise("Bob Stone", "bob@example.com", "send the logs"),
        ];
        context.promises = promises;
        let text = render(&context, None, Utc::now());
        assert!(
            text.contains("Promises  Alice promised: share the dashboard\n"),
            "{text}"
        );
        assert!(
            text.contains("          Bob promised: send the logs\n"),
            "{text}"
        );
        assert!(!text.contains("Maya promised"), "{text}");
    }
}
