//! `mxr modes`: how each mode explains itself, from the daemon's one copy
//! table, and each hint's seen state.

use crate::cli::{ModesAction, OutputFormat};
use crate::commands::desk::parse_thread_ids;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format, terminal_text};
use mxr_protocol::{
    ModeDoneOutcomeData, ModeDoneSenderData, ModeGuideData, ModeKindData, RailData, RailStatusData,
    Request, Response, ResponseData, ThreadModesData,
};
use std::fmt::Write as _;

pub async fn run(action: ModesAction, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let format = resolve_format(format);
    let request = match action {
        ModesAction::Explain { mode } => Request::GetModeGuide { mode },
        ModesAction::Hint { id, show } => Request::SetHintSeen {
            hint: id,
            seen: !show,
        },
        ModesAction::Rail { account } => {
            let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
            return rail(&mut client, account_id, format).await;
        }
        ModesAction::Why { message_id, thread } => {
            return why(&mut client, message_id, thread, format).await;
        }
        ModesAction::Done {
            thread_ids,
            mode,
            todo_ids,
            sender,
            account,
            dry_run,
        } => {
            let sender = match (sender, account) {
                (Some(sender_email), Some(account)) => Some(ModeDoneSenderData {
                    account_id: account
                        .parse()
                        .map_err(|_| anyhow::anyhow!("not an account id: {account}"))?,
                    sender_email,
                }),
                _ => None,
            };
            let request = DoneArgs {
                thread_ids: &thread_ids,
                mode: &mode,
                todo_ids,
                sender,
                dry_run,
            };
            return done(&mut client, request, format).await;
        }
    };
    let guides = expect_response(client.request(request).await?, |response| match response {
        Response::Ok {
            data: ResponseData::ModeGuides { guides },
        } => Some(guides),
        _ => None,
    })?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&guides)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&guides)?),
        _ => print!(
            "{}",
            guides.iter().map(guide_text).collect::<Vec<_>>().join("\n")
        ),
    }
    Ok(())
}

async fn rail(
    client: &mut IpcClient,
    account_id: Option<mxr_core::id::AccountId>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let rail = expect_response(
        client.request(Request::GetRail { account_id }).await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::Rail { rail },
            } => Some(rail),
            _ => None,
        },
    )?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rail)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&rail.entries)?),
        _ => print!("{}", rail_text(&rail)),
    }
    Ok(())
}

fn rail_text(rail: &RailData) -> String {
    let mut out = String::new();
    let mut group = "";
    for entry in &rail.entries {
        if !group.is_empty() && group != entry.group {
            let _ = writeln!(out, "  ----------");
        }
        group = &entry.group;
        let count = entry
            .badge
            .or(entry.count)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let mut note = match (entry.status, &entry.early_note) {
            (RailStatusData::Early, Some(note)) => format!("  {note}"),
            _ => String::new(),
        };
        if let Some(quiet) = entry.quiet.filter(|n| *n > 0) {
            let _ = write!(note, " Quiet: {quiet}.");
        }
        let _ = writeln!(
            out,
            "  {:<9} {:<4} {:>4}{note}",
            entry.name, entry.key, count
        );
    }
    let more: Vec<String> = rail
        .more
        .iter()
        .map(|link| match &link.key {
            Some(key) => format!("{} ({key})", link.name),
            None => link.name.clone(),
        })
        .collect();
    let _ = writeln!(out, "  More: {}", more.join(", "));
    out
}

async fn why(
    client: &mut IpcClient,
    message_id: Option<String>,
    thread: Option<String>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let message_id = message_id
        .map(|id| {
            id.parse()
                .map_err(|_| anyhow::anyhow!("not a message id: {id}"))
        })
        .transpose()?;
    let thread_id = thread
        .map(|id| parse_thread_ids(&[id]).map(|mut ids| ids.remove(0)))
        .transpose()?;
    let threads = expect_response(
        client
            .request(Request::GetModeMembership {
                message_id,
                thread_id,
                thread_ids: Vec::new(),
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::ModeMembership { threads },
            } => Some(threads),
            _ => None,
        },
    )?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&threads)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&threads)?),
        _ => print!(
            "{}",
            threads.iter().map(membership_text).collect::<String>()
        ),
    }
    Ok(())
}

fn membership_text(thread: &ThreadModesData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", terminal_text(&thread.subject));
    if thread.modes.is_empty() {
        let _ = writeln!(out, "  In no mode. Inbox still shows it.");
    }
    for entry in &thread.modes {
        let early = if entry.early { "  (early version)" } else { "" };
        let _ = writeln!(out, "  {:<9} {}{early}", entry.name, entry.key);
        let _ = writeln!(out, "    {}", terminal_text(&entry.reason));
    }
    if !thread.done_in.is_empty() {
        let names: Vec<&str> = thread.done_in.iter().map(|mode| mode.name()).collect();
        let _ = writeln!(out, "  Done in: {}", names.join(", "));
    }
    if let Some(question) = &thread.new_sender {
        let _ = writeln!(
            out,
            "  {} Answer with `mxr sender kind {} KIND`.",
            question.question,
            terminal_text(&question.sender_email)
        );
    }
    out
}

struct DoneArgs<'a> {
    thread_ids: &'a [String],
    mode: &'a str,
    todo_ids: Vec<String>,
    sender: Option<ModeDoneSenderData>,
    dry_run: bool,
}

async fn done(
    client: &mut IpcClient,
    args: DoneArgs<'_>,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let mode = ModeKindData::parse(args.mode).ok_or_else(|| {
        anyhow::anyhow!(
            "no mode \"{}\": use messages, todo, updates or reading",
            args.mode
        )
    })?;
    let (items, dry_run, mutation_id, undo_unavailable) = expect_response(
        client
            .request(Request::SetModeDone {
                thread_ids: parse_thread_ids(args.thread_ids)?,
                mode,
                dry_run: args.dry_run,
                todo_ids: args.todo_ids,
                sender: args.sender,
            })
            .await?,
        |response| match response {
            Response::Ok {
                data:
                    ResponseData::ModeDone {
                        items,
                        dry_run,
                        mutation_id,
                        undo_unavailable,
                    },
            } => Some((items, dry_run, mutation_id, undo_unavailable)),
            _ => None,
        },
    )?;
    match format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": dry_run,
                "items": items,
                "mutation_id": mutation_id,
                "undo_unavailable": undo_unavailable,
            }))?
        ),
        OutputFormat::Jsonl => println!("{}", jsonl(&items)?),
        _ => print!(
            "{}",
            done_text(&items, dry_run, mutation_id.as_deref(), undo_unavailable)
        ),
    }
    Ok(())
}

fn done_text(
    items: &[ModeDoneOutcomeData],
    dry_run: bool,
    mutation_id: Option<&str>,
    undo_unavailable: bool,
) -> String {
    let mut out = String::new();
    for item in items {
        match &item.error {
            Some(error) => {
                let _ = writeln!(out, "{}: not done: {error}", item.thread_id);
            }
            None if dry_run => {
                let _ = writeln!(out, "{}: would be: {}", item.thread_id, item.copy);
            }
            None => {
                let _ = writeln!(out, "{}: {}", item.thread_id, item.copy);
            }
        }
    }
    if let Some(id) = mutation_id {
        let _ = writeln!(out, "Undo with: mxr undo {id}");
    } else if undo_unavailable {
        let _ = writeln!(out, "Undo is not available for this run.");
    }
    out
}

fn keys_line(keys: &[mxr_protocol::ModeKeyData]) -> String {
    keys.iter()
        .map(|key| format!("{} {}", key.key, key.verb))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn guide_text(guide: &ModeGuideData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}: {}", guide.name, guide.header);
    let _ = writeln!(out, "{}", guide.lands_here);
    let _ = writeln!(out, "\n{}", guide.about);
    let _ = writeln!(out, "\nKeys: {}", keys_line(&guide.keys));
    let _ = writeln!(
        out,
        "\nHints, each at its element the first time it's needed:"
    );
    for hint in &guide.hints {
        let _ = writeln!(
            out,
            "  {} ({}): {}{}",
            hint.id,
            hint.anchor,
            hint.text,
            if hint.seen {
                " [dismissed; mxr modes hint ID --show brings it back]"
            } else {
                ""
            }
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_leads_with_the_job_and_prints_keys_with_their_verbs() {
        let text = guide_text(&mxr_protocol::TODO_GUIDE.to_data(|_| None));
        assert!(text.starts_with("To do: Things email asked you to do, ordered by when to act."));
        assert!(text.contains("Enter do it · e tick off · Z schedule"));
        assert!(text.contains("todo.runway (The first runway bar): The bar fills"));
        assert!(!text.contains("dismissed"));
    }
}
