//! `mxr desk later`: reply later, or wait for a reply, until a time
//! (`DeferThreads`). The phrase is resolved here, in local time, and the
//! daemon gets the instant, as every client sends it.

use crate::cli::OutputFormat;
use crate::commands::desk::plural;
use crate::commands::expect_response;
use crate::commands::time::resolve_time_arg;
use crate::ipc_client::IpcClient;
use crate::output::jsonl;
use mxr_core::id::ThreadId;
use mxr_core::natural_time::TimeChoice;
use mxr_protocol::*;

pub(crate) async fn run(
    client: &mut IpcClient,
    thread_ids: Vec<ThreadId>,
    at: &str,
    dry_run: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let resolution = resolve_time_arg(at, chrono::Utc::now())?;
    let when = resolution
        .choices
        .first()
        .map_or_else(|| resolution.at.to_rfc3339(), TimeChoice::summary);
    let resp = client
        .request(Request::DeferThreads {
            thread_ids,
            until: resolution.at,
            dry_run,
        })
        .await?;
    let deferred = expect_response(resp, |resp| match resp {
        Response::Ok {
            data:
                ResponseData::ThreadsDeferred {
                    items,
                    until,
                    dry_run,
                    mutation_id,
                    undo_unavailable,
                },
        } => Some(Deferred {
            items,
            until,
            dry_run,
            mutation_id,
            undo_unavailable,
        }),
        _ => None,
    })?;
    print!("{}", render(&deferred, &when, format)?);
    let failed = deferred
        .items
        .iter()
        .filter(|item| item.error.is_some())
        .count();
    if failed > 0 {
        anyhow::bail!(
            "{} of {} not deferred (each item says why)",
            failed,
            plural(deferred.items.len() as u32, "conversation", "conversations")
        );
    }
    Ok(())
}

struct Deferred {
    items: Vec<DeferredThreadData>,
    until: chrono::DateTime<chrono::Utc>,
    dry_run: bool,
    mutation_id: Option<String>,
    undo_unavailable: bool,
}

fn render(deferred: &Deferred, when: &str, format: OutputFormat) -> anyhow::Result<String> {
    let Deferred {
        items,
        until,
        dry_run,
        mutation_id,
        undo_unavailable,
    } = deferred;
    let mut out = String::new();
    match format {
        OutputFormat::Json => {
            out.push_str(&serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": dry_run,
                "until": until,
                "mutation_id": mutation_id,
                "undo_unavailable": undo_unavailable,
                "items": items,
            }))?);
            out.push('\n');
        }
        OutputFormat::Jsonl => {
            out.push_str(&jsonl(items)?);
            out.push('\n');
        }
        OutputFormat::Ids => {
            for item in items.iter().filter(|item| item.error.is_none()) {
                out.push_str(&format!("{}\n", item.thread_id));
            }
        }
        OutputFormat::Csv => {
            out.push_str("thread_id,kind,message_id,until,error\n");
            for item in items {
                out.push_str(&format!(
                    "{},{},{},{},{}\n",
                    item.thread_id,
                    item.kind.map(kind_key).unwrap_or_default(),
                    item.message_id
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    until.to_rfc3339(),
                    crate::commands::owed::csv_escape(item.error.as_deref().unwrap_or_default()),
                ));
            }
        }
        OutputFormat::Table => {
            let ok = items.iter().filter(|item| item.error.is_none()).count() as u32;
            let head = if *dry_run { "Would set" } else { "Set" };
            out.push_str(&format!(
                "{head} {} for {when}.\n",
                plural(ok, "conversation", "conversations")
            ));
            for item in items {
                out.push_str(&format!("  {}  {}\n", item.thread_id, describe(item)));
            }
            if let Some(id) = mutation_id {
                out.push_str(&format!("Undo with: mxr undo {id}\n"));
            } else if *undo_unavailable {
                out.push_str("Undo is not available for this run.\n");
            }
        }
    }
    Ok(out)
}

fn kind_key(kind: DeferKindData) -> &'static str {
    match kind {
        DeferKindData::ReplyLater => "reply_later",
        DeferKindData::Waiting => "waiting",
    }
}

fn describe(item: &DeferredThreadData) -> String {
    match (&item.error, item.kind) {
        (Some(error), _) => format!("not set: {error}"),
        (None, Some(DeferKindData::ReplyLater)) => {
            "reply later: back in You owe and the reply queue then".to_string()
        }
        (None, Some(DeferKindData::Waiting)) => {
            "waiting: back then if nobody has replied".to_string()
        }
        (None, None) => "nothing to set".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item(kind: Option<DeferKindData>, error: Option<&str>) -> DeferredThreadData {
        DeferredThreadData {
            thread_id: ThreadId::from_uuid(uuid::Uuid::nil()),
            account_id: None,
            kind,
            message_id: None,
            error: error.map(str::to_string),
        }
    }

    fn deferred(dry_run: bool, mutation_id: Option<&str>) -> Deferred {
        Deferred {
            items: vec![
                item(Some(DeferKindData::ReplyLater), None),
                item(Some(DeferKindData::Waiting), None),
                item(None, Some("conversation not found")),
            ],
            until: chrono::Utc
                .with_ymd_and_hms(2026, 10, 6, 8, 0, 0)
                .single()
                .unwrap(),
            dry_run,
            mutation_id: mutation_id.map(str::to_string),
            undo_unavailable: false,
        }
    }

    #[test]
    fn table_names_the_time_and_what_each_conversation_does() {
        let out = render(
            &deferred(false, Some("m-1")),
            "Tuesday 6 October, 09:00 (in 6 days)",
            OutputFormat::Table,
        )
        .unwrap();
        insta::assert_snapshot!(out);
    }

    #[test]
    fn dry_run_says_would() {
        let out = render(
            &deferred(true, None),
            "Tuesday 6 October, 09:00 (in 6 days)",
            OutputFormat::Table,
        )
        .unwrap();
        assert!(out.starts_with("Would set 2 conversations for Tuesday 6 October"));
        assert!(!out.contains("Undo with"));
    }
}
