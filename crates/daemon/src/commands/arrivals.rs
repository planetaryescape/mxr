//! `mxr arrivals`, `mxr move` and `mxr corrections` (D119): where every
//! email that arrived went, moving one email or a sender, and the moves
//! made. The same daemon requests as Now's line and the clients' `X`/`K`.

use crate::cli::{ArrivalsAction, CorrectionsAction, OutputFormat};
use crate::commands::selection::parse_message_id;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use mxr_protocol::{
    ArrivalBucketData, ArrivalListData, ArrivalsData, CorrectionData, ModeKindData,
    MoveOutcomeData, Request, Response, ResponseData,
};
use std::fmt::Write as _;

pub async fn run_arrivals(
    action: Option<ArrivalsAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    match action {
        None => {
            // The CLI reads the line without starting a visit to Now, so
            // checking from a script never empties what Now will show.
            let arrivals = expect_response(
                client
                    .request(Request::GetArrivals {
                        account_id,
                        mark_seen: false,
                    })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::Arrivals { arrivals },
                    } => Some(arrivals),
                    _ => None,
                },
            )?;
            print!("{}", render_line(&arrivals, format)?);
        }
        Some(ArrivalsAction::List {
            mode,
            since,
            until,
            limit,
        }) => {
            let bucket = mode
                .as_deref()
                .map(|raw| {
                    ArrivalBucketData::parse(raw).ok_or_else(|| {
                        anyhow::anyhow!(
                            "unknown mode `{raw}`; use messages, todo, updates, reading, archive, screened_out, spam or sorting"
                        )
                    })
                })
                .transpose()?;
            let list = expect_response(
                client
                    .request(Request::ListArrivals {
                        account_id,
                        bucket,
                        since: since.as_deref().map(parse_time).transpose()?,
                        until: until.as_deref().map(parse_time).transpose()?,
                        limit,
                    })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::ArrivalList { list },
                    } => Some(list),
                    _ => None,
                },
            )?;
            print!("{}", render_list(&list, format)?);
        }
    }
    Ok(())
}

fn parse_time(raw: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|at| at.with_timezone(&chrono::Utc))
        .map_err(|error| anyhow::anyhow!("`{raw}` is not an RFC 3339 time: {error}"))
}

fn render_line(arrivals: &ArrivalsData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(arrivals)?),
        OutputFormat::Jsonl => {
            let mut out = String::new();
            for count in arrivals.counts.iter().chain(&arrivals.also) {
                let _ = writeln!(out, "{}", serde_json::to_string(count)?);
            }
            out
        }
        OutputFormat::Ids => {
            let mut out = String::new();
            for question in &arrivals.not_sure {
                let _ = writeln!(out, "{}", question.message_id);
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::from("bucket,count,also\n");
            for count in &arrivals.counts {
                let _ = writeln!(out, "{},{},false", count.bucket.id(), count.count);
            }
            for count in &arrivals.also {
                let _ = writeln!(out, "{},{},true", count.bucket.id(), count.count);
            }
            out
        }
        OutputFormat::Table => {
            let mut out = String::new();
            let _ = writeln!(out, "{}", arrivals.line);
            if let Some(line) = &arrivals.not_sure_line {
                let _ = writeln!(out, "\n{line}");
                for question in &arrivals.not_sure {
                    let _ = writeln!(
                        out,
                        "  {}  {}",
                        question.message_id,
                        terminal_text(&question.line)
                    );
                }
                let _ = writeln!(
                    out,
                    "  Answer with `mxr move <id> <mode> --not-sure`; keeping it where it is counts too."
                );
            }
            if let Some(track) = &arrivals.track_record {
                let _ = writeln!(out, "\n{track}");
            }
            out
        }
    })
}

fn render_list(list: &ArrivalListData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(list)?),
        OutputFormat::Jsonl => {
            let mut out = String::new();
            for item in &list.items {
                let _ = writeln!(out, "{}", serde_json::to_string(item)?);
            }
            out
        }
        OutputFormat::Ids => {
            let mut out = String::new();
            for item in &list.items {
                let _ = writeln!(out, "{}", item.message_id);
            }
            out
        }
        OutputFormat::Csv => {
            use super::owed::csv_escape;
            let mut out = String::from("message_id,first_seen_at,mode,arrived_in,sender,subject\n");
            for item in &list.items {
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{}",
                    item.message_id,
                    item.first_seen_at.to_rfc3339(),
                    item.bucket.id(),
                    item.arrived_in.id(),
                    csv_escape(&item.sender_email),
                    csv_escape(&item.subject)
                );
            }
            out
        }
        OutputFormat::Table => {
            let mut out = String::new();
            let shown = list.items.len();
            let _ = writeln!(
                out,
                "{} email{}{}",
                list.total,
                if list.total == 1 { "" } else { "s" },
                if (shown as u32) < list.total {
                    format!(", newest {shown} shown")
                } else {
                    String::new()
                }
            );
            for item in &list.items {
                let who = item.sender_name.as_deref().unwrap_or(&item.sender_email);
                let _ = writeln!(
                    out,
                    "{}  {}  {}  {}  [{}]",
                    item.message_id,
                    item.first_seen_at
                        .with_timezone(&chrono::Local)
                        .format("%a %H:%M"),
                    terminal_text(who),
                    terminal_text(&item.subject),
                    terminal_text(&item.chip)
                );
            }
            out
        }
    })
}

pub async fn run_move(
    message_id: String,
    mode: String,
    sender: bool,
    dry_run: bool,
    not_sure: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mode = ModeKindData::parse(&mode).ok_or_else(|| {
        anyhow::anyhow!("unknown mode `{mode}`; use messages, todo, updates, reading or archive")
    })?;
    let mut client = IpcClient::connect().await?;
    let outcome = expect_response(
        client
            .request(Request::MoveMessage {
                message_id: parse_message_id(&message_id)?,
                mode,
                sender,
                dry_run,
                source: not_sure.then(|| "not_sure".to_string()),
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::MessageMoved { outcome },
            } => Some(outcome),
            _ => None,
        },
    )?;
    print!("{}", render_move(&outcome, resolve_format(format))?);
    Ok(())
}

fn render_move(outcome: &MoveOutcomeData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(outcome)?),
        OutputFormat::Jsonl => format!("{}\n", serde_json::to_string(outcome)?),
        OutputFormat::Ids => outcome
            .correction_id
            .map(|id| format!("{id}\n"))
            .unwrap_or_default(),
        OutputFormat::Csv => format!(
            "message_id,from,to,sender,correction_id\n{},{},{},{},{}\n",
            outcome.message_id,
            outcome.from.id(),
            outcome.to.id(),
            outcome.sender,
            outcome
                .correction_id
                .map(|id| id.to_string())
                .unwrap_or_default()
        ),
        OutputFormat::Table => {
            let mut out = format!("{}\n", outcome.copy);
            if let Some(id) = outcome.correction_id {
                let _ = writeln!(out, "Undo: mxr corrections undo {id}");
                if outcome.ask_sender.is_some() {
                    let _ = writeln!(
                        out,
                        "Always for this sender: mxr move {} {} --sender",
                        outcome.message_id,
                        outcome.to.id()
                    );
                }
            }
            out
        }
    })
}

pub async fn run_corrections(
    action: Option<CorrectionsAction>,
    account: Option<String>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);
    let mut client = IpcClient::connect().await?;
    match action {
        Some(CorrectionsAction::Undo { id }) => {
            let copy = expect_response(
                client
                    .request(Request::UndoMove { correction_id: id })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::MoveUndone { copy, .. },
                    } => Some(copy),
                    _ => None,
                },
            )?;
            match format {
                OutputFormat::Json | OutputFormat::Jsonl => println!(
                    "{}",
                    serde_json::json!({ "correction_id": id, "copy": copy })
                ),
                _ => println!("{copy}"),
            }
        }
        None => {
            let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
            let corrections = expect_response(
                client
                    .request(Request::ListCorrections { account_id, limit })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::Corrections { corrections },
                    } => Some(corrections),
                    _ => None,
                },
            )?;
            print!("{}", render_corrections(&corrections, format)?);
        }
    }
    Ok(())
}

fn render_corrections(
    corrections: &[CorrectionData],
    format: OutputFormat,
) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(corrections)?),
        OutputFormat::Jsonl => {
            let mut out = String::new();
            for correction in corrections {
                let _ = writeln!(out, "{}", serde_json::to_string(correction)?);
            }
            out
        }
        OutputFormat::Ids => {
            let mut out = String::new();
            for correction in corrections {
                let _ = writeln!(out, "{}", correction.id);
            }
            out
        }
        OutputFormat::Csv => {
            let mut out = String::from(
                "id,created_at,scope,message_id,sender,from,to,rule,source,undone\n",
            );
            for c in corrections {
                let _ = writeln!(
                    out,
                    "{},{},{},{},{},{},{},{},{},{}",
                    c.id,
                    c.created_at.to_rfc3339(),
                    c.scope,
                    c.message_id
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    super::owed::csv_escape(&c.sender_email),
                    c.from_mode,
                    c.to_mode,
                    c.rule.as_deref().unwrap_or_default(),
                    c.source,
                    c.undone_at.is_some()
                );
            }
            out
        }
        OutputFormat::Table => {
            if corrections.is_empty() {
                return Ok("No moves yet. Move an email with `mxr move <id> <mode>`.\n".into());
            }
            let mut out = String::new();
            for c in corrections {
                let _ = writeln!(
                    out,
                    "{:>5}  {}  {:<6}  {}  {} -> {}  ({}){}",
                    c.id,
                    c.created_at
                        .with_timezone(&chrono::Local)
                        .format("%a %-d %b %H:%M"),
                    c.scope,
                    terminal_text(&c.sender_email),
                    c.from_mode,
                    c.to_mode,
                    c.source,
                    if c.undone_at.is_some() { ", undone" } else { "" }
                );
            }
            out
        }
    })
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests unwrap fixture values")]

    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::ArrivalCountData;

    fn line() -> ArrivalsData {
        let now = chrono::Utc::now();
        ArrivalsData {
            generated_at: now,
            since: now,
            until: now,
            since_label: "08:12".into(),
            total: 3,
            counts: vec![ArrivalCountData {
                bucket: ArrivalBucketData::Reading,
                count: 3,
                label: "3 Reading".into(),
            }],
            also: Vec::new(),
            line: "Since 08:12: 3 arrived. 3 Reading.".into(),
            clear_line: None,
            latest_at: None,
            not_sure: Vec::new(),
            not_sure_line: None,
            not_sure_hint: None,
            track_record: Some("Last week mxr sorted 3 emails; you moved 1.".into()),
            never_bury: String::new(),
        }
    }

    #[test]
    fn the_table_prints_the_line_and_the_track_record() {
        let out = render_line(&line(), OutputFormat::Table).unwrap();
        assert!(out.starts_with("Since 08:12: 3 arrived. 3 Reading.\n"));
        assert!(out.contains("you moved 1"));
        let csv = render_line(&line(), OutputFormat::Csv).unwrap();
        assert_eq!(csv, "bucket,count,also\nreading,3,false\n");
    }

    #[test]
    fn a_move_says_how_to_undo_and_to_apply_to_the_sender() {
        let outcome = MoveOutcomeData {
            account_id: AccountId::new(),
            message_id: MessageId::new(),
            thread_id: ThreadId::new(),
            sender_email: "maya@example.com".into(),
            from: ArrivalBucketData::Updates,
            to: ModeKindData::Reading,
            sender: false,
            dry_run: false,
            copy: "Moved to Reading.".into(),
            ask_sender: Some("Always for this sender? (K)".into()),
            hint: None,
            correction_id: Some(7),
            aspect_id: None,
        };
        let out = render_move(&outcome, OutputFormat::Table).unwrap();
        assert!(out.contains("Moved to Reading."));
        assert!(out.contains("mxr corrections undo 7"));
        assert!(out.contains("--sender"));
        assert_eq!(render_move(&outcome, OutputFormat::Ids).unwrap(), "7\n");
    }
}
