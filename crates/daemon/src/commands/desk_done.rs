//! `mxr desk done`: the desk's Done over IPC (`ResolveDeskItems`).

use crate::cli::{DeskLaneArg, OutputFormat};
use crate::commands::desk::{lane_key, plural};
use crate::commands::expect_response;
use crate::ipc_client::IpcClient;
use crate::output::jsonl;
use mxr_core::id::ThreadId;
use mxr_protocol::*;

pub(crate) async fn run(
    client: &mut IpcClient,
    thread_ids: Vec<ThreadId>,
    lane: Option<DeskLaneArg>,
    promise: Option<String>,
    dry_run: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    if promise.is_some() && thread_ids.len() != 1 {
        anyhow::bail!("--promise takes exactly one thread id: the promise's conversation");
    }
    let lane = lane.map(|lane| match lane {
        DeskLaneArg::Owed => DeskLaneKind::Owed,
        DeskLaneArg::Due => DeskLaneKind::Due,
        DeskLaneArg::Waiting => DeskLaneKind::Waiting,
        DeskLaneArg::PeopleNew => DeskLaneKind::PeopleNew,
    });
    let items = thread_ids
        .into_iter()
        .map(|thread_id| DeskDoneItemData {
            thread_id,
            lane,
            commitment_id: promise.clone(),
        })
        .collect();
    let resp = client
        .request(Request::ResolveDeskItems { items, dry_run })
        .await?;
    let (items, dry_run, mutation_id, undo_unavailable) =
        expect_response(resp, |resp| match resp {
            Response::Ok {
                data:
                    ResponseData::DeskItemsResolved {
                        items,
                        dry_run,
                        mutation_id,
                        undo_unavailable,
                    },
            } => Some((items, dry_run, mutation_id, undo_unavailable)),
            _ => None,
        })?;
    print!(
        "{}",
        render(
            &items,
            dry_run,
            mutation_id.as_deref(),
            undo_unavailable,
            format
        )?
    );
    let failed = items.iter().filter(|item| item.error.is_some()).count();
    if failed > 0 {
        anyhow::bail!(
            "{} of {} not done (each item says why)",
            failed,
            plural(items.len() as u32, "conversation", "conversations")
        );
    }
    Ok(())
}

fn render(
    items: &[DeskDoneOutcomeData],
    dry_run: bool,
    mutation_id: Option<&str>,
    undo_unavailable: bool,
    format: OutputFormat,
) -> anyhow::Result<String> {
    let mut out = String::new();
    match format {
        OutputFormat::Json => {
            out.push_str(&serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": dry_run,
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
            out.push_str(
                "thread_id,lane,archived,marked_read,dismissed,resolved_commitment_id,error,reply_later_cleared\n",
            );
            for item in items {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    item.thread_id,
                    lane_key(item.lane),
                    item.archived,
                    item.marked_read,
                    item.dismissed,
                    item.resolved_commitment_id.as_deref().unwrap_or_default(),
                    crate::commands::owed::csv_escape(item.error.as_deref().unwrap_or_default()),
                    item.reply_later_cleared,
                ));
            }
        }
        OutputFormat::Table => {
            let done = items.iter().filter(|item| item.error.is_none()).count() as u32;
            let head = if dry_run { "Would put away" } else { "Done:" };
            out.push_str(&format!(
                "{head} {}.\n",
                plural(done, "conversation", "conversations")
            ));
            for item in items {
                out.push_str(&format!(
                    "  {}  {:<10}  {}\n",
                    item.thread_id,
                    lane_key(item.lane),
                    describe(item, dry_run)
                ));
            }
            if let Some(id) = mutation_id {
                out.push_str(&format!("Undo with: mxr undo {id}\n"));
            } else if undo_unavailable {
                out.push_str("Undo is not available for this run.\n");
            }
        }
    }
    Ok(out)
}

/// "archived 2, marked 1 read, off the desk until someone writes"; a dry
/// run says "would archive 2, mark 1 read, ..." because nothing changed.
fn describe(item: &DeskDoneOutcomeData, dry_run: bool) -> String {
    if let Some(error) = &item.error {
        return format!("not done: {error}");
    }
    let mut parts = Vec::new();
    if item.archived > 0 {
        let verb = if dry_run { "would archive" } else { "archived" };
        parts.push(format!("{verb} {}", item.archived));
    }
    if item.marked_read > 0 {
        let verb = match (dry_run, item.archived > 0) {
            (false, _) => "marked",
            (true, true) => "mark",
            (true, false) => "would mark",
        };
        parts.push(format!("{verb} {} read", item.marked_read));
    }
    if item.resolved_commitment_id.is_some() {
        parts.push("promise kept".to_string());
    }
    if item.reply_later_cleared > 0 {
        parts.push("out of reply later".to_string());
    }
    if item.reminders_cancelled > 0 {
        parts.push("won't come back on a timer".to_string());
    }
    if item.dismissed {
        parts.push("off the desk until someone writes".to_string());
    }
    if parts.is_empty() {
        "nothing left to change".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owed_item() -> DeskDoneOutcomeData {
        DeskDoneOutcomeData {
            thread_id: ThreadId::from_uuid(uuid::Uuid::nil()),
            account_id: None,
            lane: DeskLaneKind::Owed,
            archived: 5,
            marked_read: 1,
            dismissed: true,
            reply_later_cleared: 0,
            reminders_cancelled: 0,
            resolved_commitment_id: None,
            error: None,
        }
    }

    #[test]
    fn dry_run_table_says_what_would_change() {
        let out = render(&[owed_item()], true, None, false, OutputFormat::Table).unwrap();
        insta::assert_snapshot!(out);
    }

    #[test]
    fn real_run_table_says_what_changed() {
        let out = render(
            &[owed_item()],
            false,
            Some("01a0e733-f0fb-7510-9fb5-0462837a28ae"),
            false,
            OutputFormat::Table,
        )
        .unwrap();
        insta::assert_snapshot!(out);
    }
}
