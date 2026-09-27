//! `mxr reading`, `mxr paper-trail`, `mxr sweep`, `mxr pin`/`unpin`,
//! `mxr why` and `mxr sender kind`: mail that isn't from people, where it
//! lives, why, and how to clear it.

use crate::cli::{OutputFormat, PlaceArg, PlaceArgs, SenderAction, SenderKindArg};
use crate::commands::desk::plural;
use crate::commands::owed::{csv_escape, truncate};
use crate::commands::selection::parse_message_id;
use crate::commands::{resolve_account, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use mxr_protocol::*;
use std::io::{IsTerminal, Write};

/// How often `mxr sweep --yes` checks on its archive job.
const JOB_POLL: std::time::Duration = std::time::Duration::from_millis(300);

const fn place_data(place: PlaceArg) -> MailPlaceData {
    match place {
        PlaceArg::Reading => MailPlaceData::Reading,
        PlaceArg::PaperTrail => MailPlaceData::PaperTrail,
    }
}

const fn place_title(place: MailPlaceData) -> &'static str {
    match place {
        MailPlaceData::Reading => "Reading",
        MailPlaceData::PaperTrail => "Paper trail",
    }
}

pub(crate) const fn kind_label(kind: SenderKindData) -> &'static str {
    match kind {
        SenderKindData::People => "people",
        SenderKindData::Reading => "reading",
        SenderKindData::PaperTrail => "paper-trail",
        SenderKindData::ScreenedOut => "screened-out",
    }
}

fn expect_ok(resp: Response) -> anyhow::Result<ResponseData> {
    match resp {
        Response::Ok { data } => Ok(data),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

pub async fn list(place: MailPlaceData, args: PlaceArgs) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, args.account.as_deref()).await?;
    let data = expect_ok(
        client
            .request(Request::ListPlace {
                place,
                account_id,
                sender_email: args.sender,
                limit: args.limit,
                offset: args.offset,
                messages_per_bundle: args.messages,
                message_offset: 0,
            })
            .await?,
    )?;
    print!("{}", render_place(&data, resolve_format(args.format))?);
    Ok(())
}

fn render_place(data: &ResponseData, fmt: OutputFormat) -> anyhow::Result<String> {
    let ResponseData::Place {
        place,
        bundles,
        total_bundles,
        total_messages,
        ..
    } = data
    else {
        anyhow::bail!("Unexpected response");
    };
    let mut out = String::new();
    match fmt {
        OutputFormat::Json => {
            out.push_str(&serde_json::to_string_pretty(data)?);
            out.push('\n');
        }
        OutputFormat::Jsonl => {
            for bundle in bundles {
                out.push_str(&serde_json::to_string(bundle)?);
                out.push('\n');
            }
        }
        OutputFormat::Ids => {
            for message in bundles.iter().flat_map(|b| &b.messages) {
                out.push_str(&format!("{}\n", message.message_id));
            }
        }
        OutputFormat::Csv => {
            out.push_str("account_id,sender_email,sender_name,kind,rule,reason,message_count,unread_count,pinned_count,newest_at,newest_subject\n");
            for b in bundles {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{}\n",
                    b.account_id,
                    csv_escape(&b.sender_email),
                    csv_escape(b.sender_name.as_deref().unwrap_or_default()),
                    kind_label(b.kind.kind),
                    serde_json::to_value(b.kind.rule)?
                        .as_str()
                        .unwrap_or_default(),
                    csv_escape(&b.kind.reason),
                    b.message_count,
                    b.unread_count,
                    b.pinned_count,
                    b.newest_at.to_rfc3339(),
                    csv_escape(&b.newest_subject),
                ));
            }
        }
        OutputFormat::Table => {
            if bundles.is_empty() {
                out.push_str(&format!("{} is clear.\n", place_title(*place)));
                return Ok(out);
            }
            out.push_str(&format!(
                "{} \u{b7} {} from {}\n",
                place_title(*place),
                plural(*total_messages, "message", "messages"),
                plural(*total_bundles, "sender", "senders"),
            ));
            for b in bundles {
                let who = b.sender_name.as_deref().unwrap_or(&b.sender_email);
                out.push_str(&format!(
                    "\n{:<30}  {:>4}  {}\n",
                    truncate(&terminal_text(who), 30),
                    b.message_count,
                    b.newest_at.format("%Y-%m-%d"),
                ));
                out.push_str(&format!(
                    "  here because: {}\n",
                    terminal_text(&b.kind.reason)
                ));
                for m in &b.messages {
                    out.push_str(&format!(
                        "  {} {:<64}  {}\n",
                        if m.pinned { "\u{2022}" } else { " " },
                        truncate(&terminal_text(&m.subject), 64),
                        m.message_id,
                    ));
                }
                let hidden = b.message_count as usize - b.messages.len();
                if hidden > 0 {
                    out.push_str(&format!("    and {hidden} more\n"));
                }
            }
            let shown = bundles.len() as u32;
            if shown < *total_bundles {
                out.push_str(&format!(
                    "\nShowing {shown} of {total_bundles} senders (--offset for more).\n"
                ));
            }
        }
    }
    Ok(out)
}

pub async fn sweep(
    place: PlaceArg,
    sender: Option<String>,
    account: Option<String>,
    dry_run: bool,
    yes: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let place = place_data(place);
    let fmt = resolve_format(format);
    let request = |dry_run: bool, preview_token: Option<String>| Request::SweepPlace {
        place,
        account_id: account_id.clone(),
        sender_email: sender.clone(),
        dry_run,
        preview_token,
    };
    // The preview fixes what the sweep may touch: the sweep archives only
    // what it listed, so mail that arrives or moves in meanwhile waits.
    let (preview, _) = swept(expect_ok(client.request(request(true, None)).await?)?)?;
    if dry_run || preview.count == 0 {
        print!("{}", render_sweep(&preview, None, true, fmt)?);
        return Ok(());
    }
    if !yes {
        if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
            anyhow::bail!(
                "Confirmation required to sweep. Re-run with --yes or inspect with --dry-run."
            );
        }
        print!(
            "{}",
            render_sweep(&preview, None, true, OutputFormat::Table)?
        );
        print!("\nArchive them? [y/N] ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Nothing archived.");
            return Ok(());
        }
    }
    let (preview, job) = swept(expect_ok(
        client
            .request(request(false, preview.preview_token.clone()))
            .await?,
    )?)?;
    let job = match job {
        Some(job) => Some(wait_for_job(&mut client, job).await?),
        None => None,
    };
    print!("{}", render_sweep(&preview, job.as_ref(), false, fmt)?);
    if let Some(error) = job.as_ref().and_then(|job| job.error.as_ref()) {
        anyhow::bail!("sweep stopped: {error}");
    }
    Ok(())
}

fn swept(data: ResponseData) -> anyhow::Result<(SweepPreviewData, Option<JobData>)> {
    match data {
        ResponseData::PlaceSwept { preview, job, .. } => Ok((preview, job)),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn wait_for_job(client: &mut IpcClient, job: JobData) -> anyhow::Result<JobData> {
    let mut job = job;
    while matches!(job.status, JobStatusData::Queued | JobStatusData::Running) {
        tokio::time::sleep(JOB_POLL).await;
        job = match expect_ok(
            client
                .request(Request::GetJob {
                    job_id: job.job_id.clone(),
                })
                .await?,
        )? {
            ResponseData::Job { job } => job,
            _ => anyhow::bail!("Unexpected response"),
        };
    }
    Ok(job)
}

fn render_sweep(
    preview: &SweepPreviewData,
    job: Option<&JobData>,
    dry_run: bool,
    fmt: OutputFormat,
) -> anyhow::Result<String> {
    let archived = job.map_or(0, |job| job.progress.succeeded);
    let undo_ids: Vec<String> = job.map(|job| job.undo_ids.clone()).unwrap_or_default();
    let mut out = String::new();
    match fmt {
        OutputFormat::Json | OutputFormat::Jsonl => {
            let value = serde_json::json!({
                "dry_run": dry_run,
                "preview": preview,
                "archived": archived,
                "job": job,
            });
            out.push_str(&if fmt == OutputFormat::Json {
                serde_json::to_string_pretty(&value)?
            } else {
                serde_json::to_string(&value)?
            });
            out.push('\n');
        }
        OutputFormat::Ids => {
            for id in &undo_ids {
                out.push_str(&format!("{id}\n"));
            }
        }
        OutputFormat::Csv => {
            out.push_str("sender_email,count\n");
            for sender in &preview.senders {
                out.push_str(&format!(
                    "{},{}\n",
                    csv_escape(&sender.sender_email),
                    sender.count
                ));
            }
        }
        OutputFormat::Table => {
            let place = place_title(preview.place);
            if preview.count == 0 {
                out.push_str(&format!("Nothing to sweep in {place}.\n"));
            } else if dry_run {
                out.push_str(&format!(
                    "Would archive {} from {place}",
                    plural(preview.count, "message", "messages")
                ));
            } else {
                out.push_str(&format!(
                    "Archived {} from {place}",
                    plural(archived, "message", "messages")
                ));
            }
            if preview.count > 0 {
                if preview.pinned_excluded > 0 {
                    out.push_str(&format!(
                        "; {} stays",
                        plural(preview.pinned_excluded, "pinned message", "pinned messages")
                    ));
                }
                out.push_str(".\n");
                for sender in preview.senders.iter().take(5) {
                    let who = sender
                        .sender_name
                        .as_deref()
                        .unwrap_or(&sender.sender_email);
                    out.push_str(&format!(
                        "  {:<30}  {:>4}\n",
                        truncate(&terminal_text(who), 30),
                        sender.count
                    ));
                }
                if preview.senders.len() > 5 {
                    out.push_str(&format!(
                        "  and {} more senders\n",
                        preview.senders.len() - 5
                    ));
                }
                for subject in &preview.sample_subjects {
                    out.push_str(&format!("    {}\n", truncate(&terminal_text(subject), 70)));
                }
            }
            // One undo per archive chunk; `mxr undo` takes one id at a time.
            if !undo_ids.is_empty() {
                out.push_str("Undo with:\n");
                for id in &undo_ids {
                    out.push_str(&format!("  mxr undo {id}\n"));
                }
            }
        }
    }
    Ok(out)
}

pub async fn pin(
    message_ids: Vec<String>,
    pinned: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let ids = message_ids
        .iter()
        .map(|id| parse_message_id(id))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut client = IpcClient::connect().await?;
    let changed = match expect_ok(
        client
            .request(Request::PinMessages {
                message_ids: ids,
                pinned,
            })
            .await?,
    )? {
        ResponseData::MessagesPinned { changed, .. } => changed,
        _ => anyhow::bail!("Unexpected response"),
    };
    match resolve_format(format) {
        OutputFormat::Json | OutputFormat::Jsonl => println!(
            "{}",
            serde_json::json!({ "pinned": pinned, "changed": changed })
        ),
        _ => println!(
            "{} {}.",
            if pinned { "Pinned" } else { "Unpinned" },
            plural(changed, "message", "messages")
        ),
    }
    Ok(())
}

pub async fn why(message_id: String, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let message_id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let data = expect_ok(
        client
            .request(Request::GetMessageKind { message_id })
            .await?,
    )?;
    let ResponseData::MessageKind {
        sender_email,
        mail_kind,
        ..
    } = &data
    else {
        anyhow::bail!("Unexpected response");
    };
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&data)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&data)?),
        _ => println!(
            "{}: {} ({})",
            terminal_text(sender_email),
            kind_label(mail_kind.kind),
            terminal_text(&mail_kind.reason)
        ),
    }
    Ok(())
}

pub async fn sender_action(action: SenderAction) -> anyhow::Result<()> {
    let SenderAction::Kind {
        email,
        kind,
        account,
        format,
    } = action;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let kind = match kind {
        SenderKindArg::People => Some(SenderKindData::People),
        SenderKindArg::Reading => Some(SenderKindData::Reading),
        SenderKindArg::PaperTrail => Some(SenderKindData::PaperTrail),
        SenderKindArg::ScreenedOut => Some(SenderKindData::ScreenedOut),
        SenderKindArg::Auto => None,
    };
    let data = expect_ok(
        client
            .request(Request::SetSenderKind {
                account_id,
                sender_email: email,
                kind,
            })
            .await?,
    )?;
    let ResponseData::SenderKindSet {
        sender_email,
        sender_kind,
        previous,
        ..
    } = &data
    else {
        anyhow::bail!("Unexpected response");
    };
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&data)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&data)?),
        _ => {
            let now = sender_kind.map_or("automatic", kind_label);
            let before = previous.map_or("automatic", kind_label);
            println!(
                "{} is now {now} (was {before}).",
                terminal_text(sender_email)
            );
        }
    }
    Ok(())
}
