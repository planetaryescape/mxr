//! `mxr briefing gists <thread-id>...` and the gist line `mxr desk --gists`
//! shows: what a conversation is about and what it asks of you, from
//! `GetThreadGists`.

use crate::cli::OutputFormat;
use crate::commands::owed::csv_escape;
use crate::ipc_client::IpcClient;
use crate::output::terminal_text;
use mxr_core::ThreadId;
use mxr_protocol::{
    GistModelData, Request, Response, ResponseData, ThreadAskData, ThreadGistBatchData,
    ThreadGistData, ThreadGistSkipReasonData,
};
use mxr_tui::thread_context_rows::provenance_text;

pub async fn run(
    client: &mut IpcClient,
    thread_ids: Vec<ThreadId>,
    generate: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let batch = fetch(client, thread_ids.clone(), generate).await?;
    print!("{}", render(&thread_ids, &batch, format)?);
    Ok(())
}

pub(crate) async fn fetch(
    client: &mut IpcClient,
    thread_ids: Vec<ThreadId>,
    generate: bool,
) -> anyhow::Result<ThreadGistBatchData> {
    match client
        .request(Request::GetThreadGists {
            thread_ids,
            generate,
        })
        .await?
    {
        Response::Ok {
            data: ResponseData::ThreadGists { batch },
        } => Ok(batch),
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response to GetThreadGists"),
    }
}

/// "asks: confirm the owner", the ask as list rows show it: the model's
/// summary; a verified quote, when there is one, is in the JSON.
pub(crate) fn ask_text(ask: &ThreadAskData) -> String {
    format!("asks: {}", ask.summary)
}

/// "asks: what they want · what it's about", the line a list row shows,
/// ask first as in the web and TUI rows.
pub(crate) fn gist_line(gist: &ThreadGistData) -> String {
    let about = gist.gist.as_deref().unwrap_or_default();
    match &gist.ask {
        Some(ask) => format!("{} \u{b7} {about}", ask_text(ask)),
        None => about.to_string(),
    }
}

/// Where a line came from, in the reader's words.
fn source_label(gist: &ThreadGistData) -> Option<String> {
    gist.provenance.as_ref().map(provenance_text)
}

fn render(
    thread_ids: &[ThreadId],
    batch: &ThreadGistBatchData,
    format: OutputFormat,
) -> anyhow::Result<String> {
    let mut out = String::new();
    match format {
        OutputFormat::Json => {
            out.push_str(&serde_json::to_string_pretty(batch)?);
            out.push('\n');
        }
        OutputFormat::Jsonl => {
            for gist in &batch.gists {
                out.push_str(&serde_json::to_string(gist)?);
                out.push('\n');
            }
        }
        OutputFormat::Ids => {
            for gist in &batch.gists {
                out.push_str(&format!("{}\n", gist.thread_id));
            }
        }
        OutputFormat::Csv => {
            out.push_str("thread_id,gist,ask,ask_quote,source\n");
            for gist in &batch.gists {
                let ask = gist.ask.as_ref();
                out.push_str(&format!(
                    "{},{},{},{},{}\n",
                    gist.thread_id,
                    csv_escape(gist.gist.as_deref().unwrap_or_default()),
                    csv_escape(ask.map(|ask| ask.summary.as_str()).unwrap_or_default()),
                    csv_escape(
                        ask.and_then(|ask| ask.quote.as_ref())
                            .map(|quote| quote.text.as_str())
                            .unwrap_or_default()
                    ),
                    csv_escape(&source_label(gist).unwrap_or_default()),
                ));
            }
        }
        OutputFormat::Table => out.push_str(&table(thread_ids, batch)),
    }
    Ok(out)
}

fn table(thread_ids: &[ThreadId], batch: &ThreadGistBatchData) -> String {
    match batch.model {
        GistModelData::Disabled => {
            return "No language model is configured, so there are no gists.\n".to_string()
        }
        GistModelData::Blocked if batch.gists.is_empty() => {
            return "Privacy settings keep conversations from the configured model.\n".to_string()
        }
        GistModelData::Blocked | GistModelData::Available => {}
    }
    let mut out = String::new();
    for thread_id in thread_ids {
        let line = if let Some(gist) = batch.gists.iter().find(|g| &g.thread_id == thread_id) {
            let source = source_label(gist)
                .map(|label| format!("  ({label})"))
                .unwrap_or_default();
            format!("{}{source}", terminal_text(&gist_line(gist)))
        } else if batch.queued.contains(thread_id) {
            "queued for the model".to_string()
        } else if batch.in_flight.contains(thread_id) {
            "being written by the model".to_string()
        } else if let Some(skip) = batch.skipped.iter().find(|s| &s.thread_id == thread_id) {
            match skip.reason {
                ThreadGistSkipReasonData::NotPeople => "not from a person, no gist".to_string(),
                ThreadGistSkipReasonData::NotFound => "no such conversation".to_string(),
                ThreadGistSkipReasonData::RecentlyFailed => {
                    "the model failed on it recently; tried again later".to_string()
                }
                ThreadGistSkipReasonData::QueueFull => "the queue is full; ask again".to_string(),
                ThreadGistSkipReasonData::NotGenerated => {
                    "no gist yet (--generate queues one)".to_string()
                }
            }
        } else {
            continue;
        };
        out.push_str(&format!("{thread_id}  {line}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{
        AiLocalityData, AiProvenanceData, AiSourceData, ThreadGistSkipData, ThreadGistStatusData,
    };

    fn gist(thread_id: &ThreadId, ask: bool) -> ThreadGistData {
        ThreadGistData {
            thread_id: thread_id.clone(),
            status: ThreadGistStatusData::Ready,
            gist: Some("Canary stays at 5% until the dashboard is quiet.".into()),
            ask: ask.then(|| ThreadAskData {
                summary: "confirm who owns the rollout check".into(),
                quote: None,
            }),
            provenance: Some(AiProvenanceData {
                model: "gemma4".into(),
                locality: AiLocalityData::Local,
                sources: vec![AiSourceData::ThisThread],
            }),
            reason: None,
            generated_at: None,
            from_cache: true,
            newest_message_id: None,
        }
    }

    #[test]
    fn table_lists_each_conversation_in_request_order() {
        let (a, b, c, d) = (
            ThreadId::new(),
            ThreadId::new(),
            ThreadId::new(),
            ThreadId::new(),
        );
        let batch = ThreadGistBatchData {
            model: GistModelData::Available,
            gists: vec![gist(&b, true), gist(&a, false)],
            queued: vec![c.clone()],
            in_flight: vec![],
            skipped: vec![ThreadGistSkipData {
                thread_id: d.clone(),
                reason: ThreadGistSkipReasonData::NotPeople,
            }],
        };
        let text = table(&[a.clone(), b.clone(), c.clone(), d.clone()], &batch);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines,
            vec![
                format!("{a}  Canary stays at 5% until the dashboard is quiet.  (local model gemma4 \u{b7} from this thread only)"),
                format!("{b}  asks: confirm who owns the rollout check \u{b7} Canary stays at 5% until the dashboard is quiet.  (local model gemma4 \u{b7} from this thread only)"),
                format!("{c}  queued for the model"),
                format!("{d}  not from a person, no gist"),
            ]
        );
    }

    #[test]
    fn no_model_says_so_once() {
        let batch = ThreadGistBatchData {
            model: GistModelData::Disabled,
            gists: vec![],
            queued: vec![],
            in_flight: vec![],
            skipped: vec![],
        };
        assert_eq!(
            table(&[ThreadId::new()], &batch),
            "No language model is configured, so there are no gists.\n"
        );
    }
}
