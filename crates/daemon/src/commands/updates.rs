//! `mxr updates`: notifications as a briefing by source, gathered at fixed
//! cuts and let go in one command.
//!
//! The daemon builds the digest, its lines and why lines; this prints them
//! as a table, or passes the JSON through for scripts and agents. Letting
//! go previews first and then commits with the preview's selection token,
//! so the run acts on exactly the set it printed.

use crate::cli::{OutputFormat, UpdatesAction};
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};
use mxr_core::id::AccountId;
use mxr_protocol::{
    Request, Response, ResponseData, UpdateLineData, UpdateSourceChangeData,
    UpdateSourceSettingData, UpdatesDigestData, UpdatesLetGoData,
};
use std::fmt::Write as _;

pub async fn run(
    action: Option<UpdatesAction>,
    cut: Option<String>,
    expired: bool,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let format = resolve_format(format);
    let cut = cut.as_deref().map(parse_cut).transpose()?;
    match action {
        None => {
            let digest = digest(&mut client, account_id, cut, expired).await?;
            print!("{}", render_digest(&digest, format)?);
        }
        Some(UpdatesAction::LetGo { source, dry_run }) => {
            let preview = let_go(
                &mut client,
                account_id.clone(),
                cut,
                source.clone(),
                None,
                true,
            )
            .await?;
            let result = if dry_run || preview.message_ids.is_empty() {
                preview
            } else {
                let token = Some(preview.selection_token.clone());
                let_go(&mut client, account_id, cut, source, token, false).await?
            };
            print!("{}", render_let_go(&result, format)?);
        }
        Some(UpdatesAction::Source {
            source,
            setting,
            dry_run,
        }) => {
            let Some(setting) = UpdateSourceSettingData::parse(&setting) else {
                anyhow::bail!(
                    "Unknown setting \"{setting}\". Use every-digest, changes-only, muted or breakthrough."
                );
            };
            let change = expect_response(
                client
                    .request(Request::SetUpdateSource {
                        account_id,
                        source,
                        setting,
                        dry_run,
                    })
                    .await?,
                |response| match response {
                    Response::Ok {
                        data: ResponseData::UpdateSource { change },
                    } => Some(change),
                    _ => None,
                },
            )?;
            print!("{}", render_source(&change, format)?);
        }
    }
    Ok(())
}

/// "08:00" is the latest cut at that time (today, or yesterday when it
/// hasn't come yet); anything else is RFC3339.
fn parse_cut(value: &str) -> anyhow::Result<DateTime<Utc>> {
    if let Ok(time) = NaiveTime::parse_from_str(value.trim(), "%H:%M") {
        let now = Local::now();
        let today = now.date_naive().and_time(time);
        let at = Local
            .from_local_datetime(&today)
            .earliest()
            .ok_or_else(|| anyhow::anyhow!("{value} doesn't exist today in your time zone"))?;
        let at = if at > now { at - Duration::days(1) } else { at };
        return Ok(at.with_timezone(&Utc));
    }
    DateTime::parse_from_rfc3339(value.trim())
        .map(|at| at.with_timezone(&Utc))
        .map_err(|_| anyhow::anyhow!("--cut takes a time like 08:00 or an RFC3339 timestamp"))
}

async fn digest(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    cut: Option<DateTime<Utc>>,
    expired: bool,
) -> anyhow::Result<UpdatesDigestData> {
    expect_response(
        client
            .request(Request::GetUpdatesDigest {
                account_id,
                cut,
                mark_seen: false,
                expired,
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::UpdatesDigest { digest },
            } => Some(digest),
            _ => None,
        },
    )
}

async fn let_go(
    client: &mut IpcClient,
    account_id: Option<AccountId>,
    cut: Option<DateTime<Utc>>,
    source_key: Option<String>,
    selection_token: Option<String>,
    dry_run: bool,
) -> anyhow::Result<UpdatesLetGoData> {
    expect_response(
        client
            .request(Request::LetGoDigest {
                account_id,
                cut,
                source_key,
                selection_token,
                dry_run,
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::UpdatesLetGo { result },
            } => Some(result),
            _ => None,
        },
    )
}

fn json_line<T: serde::Serialize>(value: &T, section: Option<&str>) -> anyhow::Result<String> {
    let mut value = serde_json::to_value(value)?;
    if let (Some(section), Some(object)) = (section, value.as_object_mut()) {
        object.insert("section".into(), section.into());
    }
    Ok(serde_json::to_string(&value)?)
}

fn render_digest(digest: &UpdatesDigestData, format: OutputFormat) -> anyhow::Result<String> {
    let sections: [(&str, &[UpdateLineData]); 4] = [
        ("needs_a_look", &digest.needs_a_look),
        ("changed", &digest.changed),
        ("routine", &digest.routine),
        ("since", &digest.since.lines),
    ];
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(digest)?),
        OutputFormat::Jsonl => {
            let mut out = String::new();
            for (section, lines) in sections {
                for line in lines {
                    let _ = writeln!(out, "{}", json_line(line, Some(section))?);
                }
            }
            for expired in &digest.expired {
                let _ = writeln!(out, "{}", json_line(expired, Some("expired"))?);
            }
            out
        }
        // Thread ids, so they feed `mxr modes why` and `mxr modes done`.
        OutputFormat::Ids => {
            let mut out = String::new();
            let mut seen = std::collections::HashSet::new();
            for (_, lines) in sections {
                for thread in lines.iter().flat_map(|line| &line.thread_ids) {
                    if seen.insert(thread.clone()) {
                        let _ = writeln!(out, "{thread}");
                    }
                }
            }
            out
        }
        OutputFormat::Csv => {
            use super::owed::csv_escape;
            let mut out = String::from("section,source,fact,count,delta,in_todo\n");
            for (section, lines) in sections {
                for line in lines {
                    let _ = writeln!(
                        out,
                        "{section},{},{},{},{},{}",
                        csv_escape(&line.source_name),
                        csv_escape(&line.fact),
                        line.count,
                        csv_escape(line.delta.as_ref().map_or("", |d| d.text.as_str())),
                        line.todo_id.is_some()
                    );
                }
            }
            out
        }
        OutputFormat::Table => table(digest),
    })
}

fn line_text(line: &UpdateLineData) -> String {
    let mut text = terminal_text(&line.fact).into_owned();
    if let Some(delta) = &line.delta {
        let _ = write!(text, "  {}", delta.text);
    }
    if let Some(tracker) = line.tracker.as_ref().and_then(|t| t.detail.as_ref()) {
        let _ = write!(text, " · {}", terminal_text(tracker));
    }
    if let Some(time) = &line.time_label {
        let _ = write!(text, "  {time}");
    }
    if let Some(in_todo) = &line.in_todo {
        let _ = write!(text, "  ({in_todo})");
    }
    text
}

fn table(digest: &UpdatesDigestData) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} · {} · {} from {}",
        digest.cut.title,
        digest.cut.label,
        plural(digest.message_count, "update", "updates"),
        plural(digest.source_count, "source", "sources")
    );
    if !digest.headline.is_empty() {
        let _ = writeln!(out, "{}", digest.headline);
    }
    if let Some(empty) = &digest.empty_state {
        let _ = writeln!(out, "\n{empty}");
    }
    for (title, lines) in [
        ("NEEDS A LOOK", &digest.needs_a_look),
        ("CHANGED", &digest.changed),
        ("ROUTINE", &digest.routine),
    ] {
        if lines.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\n{title}");
        for line in lines {
            let mark = match title {
                "NEEDS A LOOK" => "!",
                _ => " ",
            };
            let count = if line.count > 1 {
                format!("  ({})", line.count)
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "{mark} {:<20}  {}{count}",
                terminal_text(&super::owed::truncate(&line.source_name, 20)),
                line_text(line)
            );
            if let Some(suggestion) = &line.suggestion {
                let _ = writeln!(
                    out,
                    "    {suggestion} `mxr updates source {} muted`",
                    line.source_key
                );
            }
        }
    }
    if let Some(hidden) = &digest.hidden_line {
        let _ = writeln!(out, "\n{hidden}");
    }
    if digest.since.message_count > 0 {
        let names: Vec<String> = digest
            .since
            .lines
            .iter()
            .map(|line| match line.count {
                1 => terminal_text(&line.source_name).into_owned(),
                n => format!("{} ({n})", terminal_text(&line.source_name)),
            })
            .collect();
        let _ = writeln!(
            out,
            "\n-- {}: {} so far: {}",
            digest.since.label,
            digest.since.message_count,
            names.join(", ")
        );
    }
    if let Some(expired) = &digest.expired_line {
        let _ = writeln!(out, "\n{expired} `mxr updates --expired` lists them.");
    }
    for expired in &digest.expired {
        let _ = writeln!(
            out,
            "  expired {}  {}  {}",
            expired
                .expired_at
                .with_timezone(&Local)
                .format("%a %-d %b %H:%M"),
            terminal_text(&expired.source_name),
            terminal_text(&expired.fact)
        );
    }
    if let Some(let_go) = &digest.let_go_line {
        let _ = writeln!(out, "\n{let_go} `mxr updates let-go`");
    }
    out
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn render_let_go(result: &UpdatesLetGoData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(result)?),
        OutputFormat::Jsonl => format!("{}\n", json_line(result, None)?),
        OutputFormat::Ids => result
            .thread_ids
            .iter()
            .map(|thread| format!("{thread}\n"))
            .collect(),
        OutputFormat::Csv | OutputFormat::Table => {
            let mut out = String::new();
            let prefix = if result.dry_run { "Would " } else { "" };
            let line = if result.dry_run {
                result.line.replacen("Let go", "let go", 1)
            } else {
                result.line.clone()
            };
            let _ = writeln!(out, "{prefix}{line}");
            for item in result.items.iter().filter(|item| item.error.is_some()) {
                let _ = writeln!(
                    out,
                    "  {}: {}",
                    item.thread_id,
                    item.error.as_deref().unwrap_or_default()
                );
            }
            if let Some(id) = &result.mutation_id {
                let _ = writeln!(out, "Undo with: mxr undo {id}");
            } else if result.undo_unavailable {
                let _ = writeln!(out, "Undo is unavailable for this run.");
            }
            out
        }
    })
}

fn render_source(change: &UpdateSourceChangeData, format: OutputFormat) -> anyhow::Result<String> {
    Ok(match format {
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(change)?),
        OutputFormat::Jsonl => format!("{}\n", json_line(change, None)?),
        _ => {
            let prefix = if change.dry_run { "Would set " } else { "" };
            let undo = if change.dry_run {
                String::new()
            } else {
                format!(
                    " Undo with: mxr updates source {} {}",
                    change.source_key,
                    change.prior.as_str().replace('_', "-")
                )
            };
            format!("{prefix}{}{undo}\n", change.copy)
        }
    })
}
