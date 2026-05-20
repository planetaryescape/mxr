#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]
//! `mxr activity` CLI subcommand. See `docs/activity-log.md`.
//!
//! The CLI is a thin wrapper around the IPC verbs added in Phase 3. Most
//! flags map 1:1 to fields of `ActivityFilter`; the parsing helpers in
//! this file translate the human-friendly arg shapes into protocol types.

use std::io::{self, IsTerminal, Read, Write};

use chrono::{DateTime, Utc};
use mxr_protocol::{
    ActivityCursor, ActivityEntry, ActivityExportFormat, ActivityFilter, ActivityStatBucket,
    ActivityStatGroupBy, ActivityTier, ClientKind, Request, Response, ResponseData,
    SavedActivityFilterEntry,
};

use crate::cli::{
    ActivityAction, ActivityClearWindow, ActivityExportFormatArg, ActivityFilterArgs,
    ActivityGroupByArg, ActivitySavedAction, ActivitySourceArg, ActivityTierArg, OutputFormat,
};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};

pub async fn run(action: ActivityAction) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        ActivityAction::List {
            filter,
            limit,
            cursor,
            format,
        } => run_list(&mut client, filter, limit, cursor, format).await,
        ActivityAction::Stats {
            filter,
            group_by,
            format,
        } => run_stats(&mut client, filter, group_by, format).await,
        ActivityAction::Top {
            filter,
            limit,
            format,
        } => run_top(&mut client, filter, limit, format).await,
        ActivityAction::Export {
            filter,
            format,
            out,
        } => run_export(&mut client, filter, format, out).await,
        ActivityAction::Prune {
            before,
            tier,
            dry_run,
            yes,
        } => run_prune(&mut client, before, tier, dry_run, yes).await,
        ActivityAction::Redact {
            ids,
            filter,
            dry_run,
            yes,
        } => run_redact(&mut client, ids, filter, dry_run, yes).await,
        ActivityAction::Clear {
            window,
            include_important,
            dry_run,
            yes,
        } => run_clear(&mut client, window, include_important, dry_run, yes).await,
        ActivityAction::Pause { for_, quiet } => run_pause(&mut client, for_, quiet).await,
        ActivityAction::Resume => run_resume(&mut client).await,
        ActivityAction::Status { format } => run_status(&mut client, format).await,
        ActivityAction::Saved { action } => run_saved(&mut client, action).await,
        ActivityAction::Tail {
            filter,
            lines,
            interval,
            format,
        } => run_tail(&mut client, filter, lines, interval, format).await,
        ActivityAction::Recall {
            phrase,
            limit,
            format,
        } => run_recall(&mut client, phrase, limit, format).await,
        ActivityAction::Replay {
            since,
            limit,
            format,
        } => run_replay(&mut client, since, limit, format).await,
    }
}

// ===== conversions =====

fn source_arg_to_proto(s: ActivitySourceArg) -> ClientKind {
    match s {
        ActivitySourceArg::Tui => ClientKind::Tui,
        ActivitySourceArg::Cli => ClientKind::Cli,
        ActivitySourceArg::Web => ClientKind::Web,
        ActivitySourceArg::Daemon => ClientKind::Daemon,
    }
}

fn tier_arg_to_proto(t: ActivityTierArg) -> ActivityTier {
    match t {
        ActivityTierArg::Ephemeral => ActivityTier::Ephemeral,
        ActivityTierArg::Standard => ActivityTier::Standard,
        ActivityTierArg::Important => ActivityTier::Important,
    }
}

fn group_arg_to_proto(g: ActivityGroupByArg) -> ActivityStatGroupBy {
    match g {
        ActivityGroupByArg::Action => ActivityStatGroupBy::Action,
        ActivityGroupByArg::Day => ActivityStatGroupBy::Day,
        ActivityGroupByArg::Source => ActivityStatGroupBy::Source,
        ActivityGroupByArg::TargetKind => ActivityStatGroupBy::TargetKind,
        ActivityGroupByArg::Hour => ActivityStatGroupBy::Hour,
    }
}

fn export_arg_to_proto(f: ActivityExportFormatArg) -> ActivityExportFormat {
    match f {
        ActivityExportFormatArg::Csv => ActivityExportFormat::Csv,
        ActivityExportFormatArg::Json => ActivityExportFormat::Json,
        ActivityExportFormatArg::Ndjson => ActivityExportFormat::Ndjson,
    }
}

fn args_to_filter(args: ActivityFilterArgs) -> anyhow::Result<ActivityFilter> {
    let since = args.since.as_deref().map(parse_time_input).transpose()?;
    let until = args.until.as_deref().map(parse_time_input).transpose()?;
    Ok(ActivityFilter {
        since,
        until,
        account_id: args.account,
        sources: args.source.into_iter().map(source_arg_to_proto).collect(),
        actions: args.action,
        action_prefix: args.prefix,
        target_kind: args.target_kind,
        target_id: args.target_id,
        tiers: args.tier.into_iter().map(tier_arg_to_proto).collect(),
        query: args.query,
        include_redacted: args.include_redacted,
    })
}

/// Parse either a relative duration (`1h`, `3d`, `2w`) or an ISO date/time.
/// Returns unix milliseconds.
pub fn parse_time_input(s: &str) -> anyhow::Result<i64> {
    // Try relative duration first.
    if let Some(ms) = parse_duration_ms(s) {
        let now = Utc::now().timestamp_millis();
        return Ok(now - ms);
    }
    // Try ISO 8601 date (YYYY-MM-DD)
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = naive_date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("invalid date"))?;
        return Ok(dt.and_utc().timestamp_millis());
    }
    // Try ISO 8601 datetime (YYYY-MM-DDTHH:MM:SS[Z])
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc).timestamp_millis());
    }
    anyhow::bail!(
        "could not parse time '{s}'. Try '1h', '3d', '2026-05-01', or '2026-05-01T09:00:00Z'."
    )
}

/// Returns elapsed milliseconds for a relative duration string. Returns
/// `None` for unrecognized input so callers can fall through to date parsing.
fn parse_duration_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_part, unit) = s.split_at(s.len() - 1);
    let n: i64 = num_part.parse().ok()?;
    let ms = match unit {
        "s" => n * 1_000,
        "m" => n * 60_000,
        "h" => n * 3_600_000,
        "d" => n * 86_400_000,
        "w" => n * 7 * 86_400_000,
        _ => return None,
    };
    Some(ms)
}

fn parse_cursor(s: &str) -> anyhow::Result<ActivityCursor> {
    let (ts_s, id_s) = s
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("cursor must be `TS,ID`"))?;
    Ok(ActivityCursor {
        ts: ts_s.trim().parse()?,
        id: id_s.trim().parse()?,
    })
}

// ===== list =====

async fn run_list(
    client: &mut IpcClient,
    filter: ActivityFilterArgs,
    limit: u32,
    cursor: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let filter = args_to_filter(filter)?;
    let cursor = cursor.as_deref().map(parse_cursor).transpose()?;
    let resp = client
        .request(Request::ListActivity {
            filter,
            limit,
            cursor,
        })
        .await?;
    let (entries, next_cursor) = super::expect_response(resp, |r| match r {
        Response::Ok {
            data:
                ResponseData::ActivityEntries {
                    entries,
                    next_cursor,
                },
        } => Some((entries, next_cursor)),
        _ => None,
    })?;

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "entries": entries,
                "next_cursor": next_cursor,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Jsonl => println!("{}", jsonl(&entries)?),
        _ => render_list_table(&entries, next_cursor),
    }
    Ok(())
}

fn render_list_table(entries: &[ActivityEntry], next_cursor: Option<ActivityCursor>) {
    if entries.is_empty() {
        println!("No activity in this window.");
        return;
    }
    println!(
        "{:<19} {:<4} {:<22} {:<24} CONTEXT",
        "TIMESTAMP", "SRC", "ACTION", "TARGET"
    );
    println!("{}", "-".repeat(110));
    for e in entries {
        let ts = chrono::DateTime::from_timestamp_millis(e.ts)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| e.ts.to_string());
        let target = format!(
            "{}{}",
            e.target_kind.as_deref().unwrap_or("-"),
            e.target_id
                .as_ref()
                .map(|t| format!(":{}", &t[..t.len().min(12)]))
                .unwrap_or_default()
        );
        let target: String = target.chars().take(24).collect();
        let context = match (e.redacted, &e.context) {
            (true, _) => "(redacted)".to_string(),
            (false, Some(c)) => {
                let s = serde_json::to_string(c).unwrap_or_default();
                s.chars().take(60).collect()
            }
            (false, None) => "".to_string(),
        };
        println!(
            "{:<19} {:<4} {:<22} {:<24} {}",
            ts,
            e.source.as_str(),
            e.action,
            target,
            context
        );
    }
    println!("\n{} rows", entries.len());
    if let Some(c) = next_cursor {
        println!("Next: --cursor {},{}", c.ts, c.id);
    }
}

// ===== stats =====

async fn run_stats(
    client: &mut IpcClient,
    filter: ActivityFilterArgs,
    group_by: ActivityGroupByArg,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let f = args_to_filter(filter)?;
    let since = f.since.unwrap_or_else(|| {
        Utc::now().timestamp_millis() - 7 * 86_400_000 // default 7d
    });
    let until = f.until.unwrap_or_else(|| Utc::now().timestamp_millis());

    let resp = client
        .request(Request::ActivityStats {
            since,
            until,
            group_by: group_arg_to_proto(group_by.clone()),
        })
        .await?;
    let buckets = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityStatBuckets { buckets },
        } => Some(buckets),
        _ => None,
    })?;

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&buckets)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&buckets)?),
        _ => render_stats_table(&buckets, &group_by),
    }
    Ok(())
}

fn render_stats_table(buckets: &[ActivityStatBucket], group_by: &ActivityGroupByArg) {
    if buckets.is_empty() {
        println!("No activity in this window.");
        return;
    }
    let key_label = match group_by {
        ActivityGroupByArg::Action => "ACTION",
        ActivityGroupByArg::Day => "DAY",
        ActivityGroupByArg::Source => "SOURCE",
        ActivityGroupByArg::TargetKind => "TARGET",
        ActivityGroupByArg::Hour => "HOUR",
    };
    println!("{:<20} {:>8}", key_label, "COUNT");
    println!("{}", "-".repeat(30));
    for b in buckets {
        println!("{:<20} {:>8}", b.key, b.count);
    }
}

// ===== top =====

async fn run_top(
    client: &mut IpcClient,
    filter: ActivityFilterArgs,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    // top is just stats group=action capped to `limit`.
    let f = args_to_filter(filter)?;
    let since = f
        .since
        .unwrap_or_else(|| Utc::now().timestamp_millis() - 7 * 86_400_000);
    let until = f.until.unwrap_or_else(|| Utc::now().timestamp_millis());

    let resp = client
        .request(Request::ActivityStats {
            since,
            until,
            group_by: ActivityStatGroupBy::Action,
        })
        .await?;
    let mut buckets = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityStatBuckets { buckets },
        } => Some(buckets),
        _ => None,
    })?;
    buckets.truncate(limit as usize);

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&buckets)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&buckets)?),
        _ => render_stats_table(&buckets, &ActivityGroupByArg::Action),
    }
    Ok(())
}

// ===== export =====

async fn run_export(
    client: &mut IpcClient,
    filter: ActivityFilterArgs,
    format: ActivityExportFormatArg,
    out: Option<String>,
) -> anyhow::Result<()> {
    let filter = args_to_filter(filter)?;
    let resp = client
        .request(Request::ExportActivity {
            filter,
            format: export_arg_to_proto(format.clone()),
            path: out.clone().filter(|s| s != "-"),
        })
        .await?;
    let (count, body, path, size_bytes) = super::expect_response(resp, |r| match r {
        Response::Ok {
            data:
                ResponseData::ActivityExportResult {
                    count,
                    body,
                    path,
                    size_bytes,
                    ..
                },
        } => Some((count, body, path, size_bytes)),
        _ => None,
    })?;

    match (body, path) {
        (Some(b), None) => {
            // inline → stdout
            let mut stdout = io::stdout().lock();
            stdout.write_all(b.as_bytes())?;
            if !b.ends_with('\n') {
                writeln!(stdout)?;
            }
        }
        (None, Some(p)) => {
            eprintln!("Wrote {count} rows ({size_bytes} bytes) to {p}");
        }
        _ => eprintln!("Wrote {count} rows ({size_bytes} bytes)"),
    }
    Ok(())
}

// ===== prune =====

async fn run_prune(
    client: &mut IpcClient,
    before: String,
    tier: Option<ActivityTierArg>,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let before_ts = parse_time_input(&before)?;
    if !dry_run && !yes && !confirm("Hard-delete activity rows older than the cutoff?")? {
        eprintln!("Aborted.");
        return Ok(());
    }
    let resp = client
        .request(Request::PruneActivity {
            before_ts,
            tier: tier.map(tier_arg_to_proto),
            dry_run,
        })
        .await?;
    let (count, dry) = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityAffected { count, dry_run },
        } => Some((count, dry_run)),
        _ => None,
    })?;
    if dry {
        println!("Would delete {count} rows.");
    } else {
        println!("Deleted {count} rows.");
    }
    Ok(())
}

// ===== redact =====

async fn run_redact(
    client: &mut IpcClient,
    ids: Vec<i64>,
    filter: ActivityFilterArgs,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let has_filter = filter.since.is_some()
        || filter.until.is_some()
        || !filter.source.is_empty()
        || !filter.action.is_empty()
        || filter.prefix.is_some()
        || filter.target_kind.is_some()
        || filter.target_id.is_some()
        || !filter.tier.is_empty()
        || filter.account.is_some()
        || filter.query.is_some();
    if ids.is_empty() && !has_filter {
        anyhow::bail!("`redact` requires `--ids` or filter flags.");
    }
    if !ids.is_empty() && has_filter {
        anyhow::bail!("`redact` takes `--ids` OR a filter, not both.");
    }
    if !dry_run && !yes && !confirm("Tombstone matching activity rows (irreversible)?")? {
        eprintln!("Aborted.");
        return Ok(());
    }
    let filter_proto = if has_filter {
        Some(args_to_filter(filter)?)
    } else {
        None
    };
    let resp = client
        .request(Request::RedactActivity {
            ids,
            filter: filter_proto,
            dry_run,
        })
        .await?;
    let (count, dry) = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityAffected { count, dry_run },
        } => Some((count, dry_run)),
        _ => None,
    })?;
    if dry {
        println!("Would tombstone {count} rows.");
    } else {
        println!("Tombstoned {count} rows.");
    }
    Ok(())
}

// ===== clear =====

async fn run_clear(
    client: &mut IpcClient,
    window: ActivityClearWindow,
    include_important: bool,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let since = match window {
        ActivityClearWindow::LastHour => Some(Utc::now().timestamp_millis() - 3_600_000),
        ActivityClearWindow::LastDay => Some(Utc::now().timestamp_millis() - 86_400_000),
        ActivityClearWindow::LastWeek => Some(Utc::now().timestamp_millis() - 7 * 86_400_000),
        ActivityClearWindow::LastMonth => Some(Utc::now().timestamp_millis() - 30 * 86_400_000),
        ActivityClearWindow::All => None,
    };
    let tiers = if include_important {
        vec![]
    } else {
        vec![ActivityTier::Ephemeral, ActivityTier::Standard]
    };
    let filter = ActivityFilter {
        since,
        tiers,
        ..Default::default()
    };
    if !dry_run && !yes && !confirm("Tombstone matching activity rows (irreversible)?")? {
        eprintln!("Aborted.");
        return Ok(());
    }
    let resp = client
        .request(Request::RedactActivity {
            ids: vec![],
            filter: Some(filter),
            dry_run,
        })
        .await?;
    let (count, dry) = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityAffected { count, dry_run },
        } => Some((count, dry_run)),
        _ => None,
    })?;
    if dry {
        println!("Would tombstone {count} rows.");
    } else {
        println!("Tombstoned {count} rows.");
    }
    Ok(())
}

// ===== pause / resume =====

async fn run_pause(
    client: &mut IpcClient,
    for_: Option<String>,
    quiet: bool,
) -> anyhow::Result<()> {
    let until_ts = match for_.as_deref() {
        Some(d) => {
            let dur =
                parse_duration_ms(d).ok_or_else(|| anyhow::anyhow!("invalid duration '{d}'"))?;
            if dur <= 0 {
                anyhow::bail!("`--for` must be a positive duration");
            }
            Some(Utc::now().timestamp_millis() + dur)
        }
        None => None,
    };
    let resp = client.request(Request::PauseActivity { until_ts }).await?;
    super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Acknowledged,
        } => Some(()),
        _ => None,
    })?;
    if !quiet {
        match until_ts {
            Some(t) => {
                let when = chrono::DateTime::from_timestamp_millis(t)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default();
                println!("Activity recording paused until {when}.");
            }
            None => println!(
                "Activity recording paused indefinitely. Run `mxr activity resume` to resume."
            ),
        }
    }
    Ok(())
}

async fn run_resume(client: &mut IpcClient) -> anyhow::Result<()> {
    let resp = client.request(Request::ResumeActivity).await?;
    super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::Acknowledged,
        } => Some(()),
        _ => None,
    })?;
    println!("Activity recording resumed.");
    Ok(())
}

// ===== status =====

async fn run_status(client: &mut IpcClient, format: Option<OutputFormat>) -> anyhow::Result<()> {
    // Count via last 30d so the user can see a "total recent rows" number.
    let now = Utc::now().timestamp_millis();
    let since = now - 30 * 86_400_000;
    let filter = ActivityFilter {
        since: Some(since),
        include_redacted: true,
        ..Default::default()
    };
    let count_resp = client.request(Request::CountActivity { filter }).await?;
    let count = super::expect_response(count_resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityCount { count },
        } => Some(count),
        _ => None,
    })?;

    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "rows_last_30d": count,
                }))?
            );
        }
        _ => {
            println!("Activity rows in the last 30d: {count}");
            println!("(Use `mxr activity list` to browse, `pause`/`resume` to control recording.)");
        }
    }
    Ok(())
}

// ===== saved filters (Phase 8) =====

async fn run_saved(client: &mut IpcClient, action: ActivitySavedAction) -> anyhow::Result<()> {
    match action {
        ActivitySavedAction::List { format } => {
            let resp = client.request(Request::ListSavedActivityFilters).await?;
            let entries = super::expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::SavedActivityFilters { entries },
                } => Some(entries),
                _ => None,
            })?;
            let fmt = resolve_format(format);
            match fmt {
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entries)?),
                OutputFormat::Jsonl => println!("{}", jsonl(&entries)?),
                _ => render_saved_table(&entries),
            }
        }
        ActivitySavedAction::Save { slug, name, filter } => {
            let filter = args_to_filter(filter)?;
            let resp = client
                .request(Request::UpsertSavedActivityFilter {
                    slug: slug.clone(),
                    name,
                    filter,
                })
                .await?;
            super::expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::Acknowledged,
                } => Some(()),
                _ => None,
            })?;
            println!("Saved filter '{slug}'.");
        }
        ActivitySavedAction::Delete { slug } => {
            let resp = client
                .request(Request::DeleteSavedActivityFilter { slug: slug.clone() })
                .await?;
            let count = super::expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::ActivityAffected { count, .. },
                } => Some(count),
                _ => None,
            })?;
            if count == 0 {
                println!("No filter named '{slug}'.");
            } else {
                println!("Deleted filter '{slug}'.");
            }
        }
        ActivitySavedAction::Open {
            slug,
            limit,
            format,
        } => {
            let resp = client
                .request(Request::GetSavedActivityFilter { slug: slug.clone() })
                .await?;
            let entry = super::expect_response(resp, |r| match r {
                Response::Ok {
                    data: ResponseData::SavedActivityFilterDetail { entry },
                } => Some(entry),
                _ => None,
            })?;
            let Some(entry) = entry else {
                anyhow::bail!("no saved filter with slug '{slug}'");
            };
            let resp = client
                .request(Request::ListActivity {
                    filter: entry.filter,
                    limit,
                    cursor: None,
                })
                .await?;
            let (entries, next_cursor) = super::expect_response(resp, |r| match r {
                Response::Ok {
                    data:
                        ResponseData::ActivityEntries {
                            entries,
                            next_cursor,
                        },
                } => Some((entries, next_cursor)),
                _ => None,
            })?;
            let fmt = resolve_format(format);
            match fmt {
                OutputFormat::Json => {
                    let payload = serde_json::json!({
                        "entries": entries,
                        "next_cursor": next_cursor,
                    });
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                }
                OutputFormat::Jsonl => println!("{}", jsonl(&entries)?),
                _ => render_list_table(&entries, next_cursor),
            }
        }
    }
    Ok(())
}

fn render_saved_table(entries: &[SavedActivityFilterEntry]) {
    if entries.is_empty() {
        println!("No saved filters. Save one with `mxr activity saved save <slug> --name <NAME> --prefix mail.`.");
        return;
    }
    println!("{:<20} {:<30} {:<20}", "SLUG", "NAME", "LAST_USED");
    println!("{}", "-".repeat(76));
    for e in entries {
        let last = e
            .last_used_at
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".to_string());
        println!("{:<20} {:<30} {:<20}", e.slug, e.name, last);
    }
}

// ===== tail (Phase 4 deferred) =====

async fn run_tail(
    client: &mut IpcClient,
    filter: ActivityFilterArgs,
    lines: u32,
    interval_secs: u64,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    let base_filter = args_to_filter(filter)?;

    // Initial backfill — most recent `lines` rows.
    let resp = client
        .request(Request::ListActivity {
            filter: base_filter.clone(),
            limit: lines,
            cursor: None,
        })
        .await?;
    let mut entries = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityEntries { entries, .. },
        } => Some(entries),
        _ => None,
    })?;
    // Render oldest-first so the screen reads top→bottom.
    entries.reverse();
    for e in &entries {
        emit_tail_row(e, &fmt)?;
    }
    let mut last_seen_ts = entries.last().map(|e| e.ts).unwrap_or(0);

    // Follow loop.
    let interval = std::time::Duration::from_secs(interval_secs.max(1));
    loop {
        tokio::time::sleep(interval).await;
        let mut filter = base_filter.clone();
        filter.since = Some(last_seen_ts + 1);
        let resp = client
            .request(Request::ListActivity {
                filter,
                limit: 100,
                cursor: None,
            })
            .await?;
        let mut new_entries = super::expect_response(resp, |r| match r {
            Response::Ok {
                data: ResponseData::ActivityEntries { entries, .. },
            } => Some(entries),
            _ => None,
        })?;
        if !new_entries.is_empty() {
            new_entries.reverse();
            for e in &new_entries {
                emit_tail_row(e, &fmt)?;
            }
            if let Some(last) = new_entries.last() {
                last_seen_ts = last.ts;
            }
        }
    }
}

fn emit_tail_row(e: &ActivityEntry, fmt: &OutputFormat) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json | OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(e)?);
        }
        _ => {
            let ts = chrono::DateTime::from_timestamp_millis(e.ts)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| e.ts.to_string());
            let target = e.target_id.as_deref().unwrap_or("-");
            let target: String = target.chars().take(12).collect();
            println!("{ts}  {:<4} {:<22} {target}", e.source.as_str(), e.action);
        }
    }
    Ok(())
}

// ===== recall (Phase 8) =====

/// Curated, predictable grammar — no NLP. Add phrases by extending this
/// function. Anything unrecognized returns an error pointing the user at
/// the supported phrases.
pub fn parse_recall_phrase(phrase: &str) -> anyhow::Result<(i64, i64)> {
    let now = Utc::now();
    let phrase = phrase.trim().to_lowercase();
    let day_ms = 86_400_000_i64;

    // Helpers
    let today_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp_millis();

    // Bare relative: "last 5 minutes", "past 2 days", "last hour".
    if let Some(rest) = phrase
        .strip_prefix("last ")
        .or_else(|| phrase.strip_prefix("past "))
    {
        if let Some(ms) = parse_human_duration(rest) {
            return Ok((now.timestamp_millis() - ms, now.timestamp_millis()));
        }
    }

    // Absolute days.
    match phrase.as_str() {
        "today" => return Ok((today_start, today_start + day_ms)),
        "yesterday" => return Ok((today_start - day_ms, today_start)),
        "tomorrow" => return Ok((today_start + day_ms, today_start + 2 * day_ms)),
        "this morning" => return Ok((today_start + 6 * 3_600_000, today_start + 12 * 3_600_000)),
        "this afternoon" => {
            return Ok((today_start + 12 * 3_600_000, today_start + 18 * 3_600_000))
        }
        "this evening" => return Ok((today_start + 18 * 3_600_000, today_start + 23 * 3_600_000)),
        "morning" => return Ok((today_start + 6 * 3_600_000, today_start + 12 * 3_600_000)),
        "afternoon" => return Ok((today_start + 12 * 3_600_000, today_start + 18 * 3_600_000)),
        "evening" => return Ok((today_start + 18 * 3_600_000, today_start + 23 * 3_600_000)),
        "lunch" => {
            return Ok((
                today_start + 12 * 3_600_000,
                today_start + 13 * 3_600_000 + 1_800_000,
            ))
        }
        "breakfast" => return Ok((today_start + 6 * 3_600_000, today_start + 9 * 3_600_000)),
        "night" => return Ok((today_start + 22 * 3_600_000, today_start + 28 * 3_600_000)),
        _ => {}
    }

    // Anchored "before/after/since/until <named>".
    for anchor in ["before ", "after ", "since ", "until "] {
        if let Some(rest) = phrase.strip_prefix(anchor) {
            if let Ok((s, u)) = parse_recall_phrase(rest) {
                return Ok(match anchor {
                    "before " => (i64::MIN / 2, s), // up to start of window
                    "after " => (u, i64::MAX / 2),
                    "since " => (s, now.timestamp_millis()),
                    "until " => (i64::MIN / 2, s),
                    _ => unreachable!(),
                });
            }
        }
    }

    // Yesterday <part>
    if let Some(rest) = phrase.strip_prefix("yesterday ") {
        let (a, _b) = parse_recall_phrase(rest)?;
        return Ok((a - day_ms, a - day_ms + (rest_len_window(rest))));
    }

    anyhow::bail!(
        "could not parse '{phrase}'. Try one of: today, yesterday, this morning, this afternoon, this evening, lunch, breakfast, night, last hour, last 5 minutes, past 2 days, before lunch, after lunch, since this morning."
    )
}

fn rest_len_window(rest: &str) -> i64 {
    match rest {
        "morning" | "afternoon" | "evening" => 6 * 3_600_000,
        "lunch" => 90 * 60_000,
        "breakfast" => 3 * 3_600_000,
        "night" => 6 * 3_600_000,
        _ => 4 * 3_600_000,
    }
}

fn parse_human_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    // "5 minutes", "2 days", "3 hours", "1 week"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() == 2 {
        let n: i64 = parts[0].parse().ok()?;
        let unit = parts[1].trim_end_matches('s');
        let ms = match unit {
            "second" => n * 1_000,
            "minute" => n * 60_000,
            "hour" => n * 3_600_000,
            "day" => n * 86_400_000,
            "week" => n * 7 * 86_400_000,
            _ => return None,
        };
        return Some(ms);
    }
    // Single-word like "hour", "minute"
    let unit = s.trim_end_matches('s');
    let ms = match unit {
        "hour" => 3_600_000,
        "minute" => 60_000,
        "day" => 86_400_000,
        "week" => 7 * 86_400_000,
        _ => return None,
    };
    Some(ms)
}

async fn run_recall(
    client: &mut IpcClient,
    phrase: String,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let (since, until) = parse_recall_phrase(&phrase)?;
    let filter = ActivityFilter {
        since: Some(since),
        until: Some(until),
        ..Default::default()
    };
    let resp = client
        .request(Request::ListActivity {
            filter,
            limit,
            cursor: None,
        })
        .await?;
    let (entries, next_cursor) = super::expect_response(resp, |r| match r {
        Response::Ok {
            data:
                ResponseData::ActivityEntries {
                    entries,
                    next_cursor,
                },
        } => Some((entries, next_cursor)),
        _ => None,
    })?;
    let fmt = resolve_format(format);
    match fmt {
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "phrase": phrase,
                "since": since,
                "until": until,
                "entries": entries,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        OutputFormat::Jsonl => println!("{}", jsonl(&entries)?),
        _ => {
            let s = chrono::DateTime::from_timestamp_millis(since)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            let u = chrono::DateTime::from_timestamp_millis(until)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();
            println!("Activity from {s} to {u} (\"{phrase}\"):\n");
            render_list_table(&entries, next_cursor);
        }
    }
    Ok(())
}

// ===== replay (Phase 8) =====

async fn run_replay(
    client: &mut IpcClient,
    since: String,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let since_ms = parse_time_input(&since)?;
    let filter = ActivityFilter {
        since: Some(since_ms),
        ..Default::default()
    };
    let resp = client
        .request(Request::ListActivity {
            filter,
            limit,
            cursor: None,
        })
        .await?;
    let mut entries = super::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ActivityEntries { entries, .. },
        } => Some(entries),
        _ => None,
    })?;
    // Sort ascending for narrative.
    entries.sort_by_key(|e| e.ts);

    let fmt = resolve_format(format);
    if matches!(fmt, OutputFormat::Json | OutputFormat::Jsonl) {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("No activity to replay since {since}.");
        return Ok(());
    }
    println!("Since {since}:");
    let groups = group_for_replay(&entries);
    for line in groups {
        println!("  {line}");
    }
    Ok(())
}

/// Group consecutive rows by action prefix and emit human-readable lines.
fn group_for_replay(entries: &[ActivityEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if entries.is_empty() {
        return out;
    }
    let mut i = 0;
    while i < entries.len() {
        let action = &entries[i].action;
        let mut j = i + 1;
        while j < entries.len()
            && entries[j].action == *action
            && entries[j].ts - entries[j - 1].ts < 5 * 60_000
        {
            j += 1;
        }
        let count = j - i;
        let ts = chrono::DateTime::from_timestamp_millis(entries[i].ts)
            .map(|dt| dt.format("%H:%M").to_string())
            .unwrap_or_default();
        out.push(format!(
            "{ts}  {}",
            describe_group(action, count, &entries[i..j])
        ));
        i = j;
    }
    out
}

fn describe_group(action: &str, count: usize, slice: &[ActivityEntry]) -> String {
    let plural = if count == 1 { "" } else { "s" };
    match action {
        "mail.read" => format!("Read {count} thread{plural}"),
        "mail.archive" => format!("Archived {count} thread{plural}"),
        "mail.trash" => format!("Trashed {count} thread{plural}"),
        "mail.star" => format!("Starred {count} thread{plural}"),
        "mail.unstar" => format!("Unstarred {count} thread{plural}"),
        "mail.snooze" => format!("Snoozed {count} thread{plural}"),
        "mail.unsnooze" => format!("Unsnoozed {count} thread{plural}"),
        "mail.send" => format!("Sent {count} message{plural}"),
        "mail.reply" => format!("Replied to {count} thread{plural}"),
        "mail.forward" => format!("Forwarded {count} thread{plural}"),
        "mail.unsubscribe" => format!("Unsubscribed from {count} list{plural}"),
        "thread.open" => format!("Opened {count} thread{plural}"),
        "thread.summarize" => format!("Summarised {count} thread{plural}"),
        "search.run" => {
            let q = slice
                .first()
                .and_then(|e| e.context.as_ref())
                .and_then(|c| c.get("query"))
                .and_then(|v| v.as_str())
                .unwrap_or("…");
            if count == 1 {
                format!("Searched \"{q}\"")
            } else {
                format!("Ran {count} searches (most recent: \"{q}\")")
            }
        }
        "draft.create" => format!("Started {count} draft{plural}"),
        "draft.discard" => format!("Discarded {count} draft{plural}"),
        "screener.allow" | "screener.block" | "screener.snooze" => {
            format!("Triaged {count} sender{plural} in the screener")
        }
        "rule.run" => format!("Ran {count} rule{plural}"),
        _ if action.starts_with("activity.") => format!("Activity-log: {action} (×{count})"),
        other => format!("{count}× {other}"),
    }
}

// ===== confirm =====

fn confirm(prompt: &str) -> anyhow::Result<bool> {
    if !io::stdin().is_terminal() {
        // Non-interactive: refuse rather than guess. Caller can pass `--yes`.
        anyhow::bail!("non-interactive: pass `--yes` to confirm");
    }
    eprint!("{prompt} [y/N] ");
    io::stderr().flush()?;
    let mut buf = String::new();
    let mut stdin = io::stdin().lock();
    let mut byte = [0u8; 1];
    while stdin.read(&mut byte)? > 0 {
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0] as char);
    }
    let trimmed = buf.trim().to_lowercase();
    Ok(trimmed == "y" || trimmed == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_recognizes_common_units() {
        assert_eq!(parse_duration_ms("30s"), Some(30_000));
        assert_eq!(parse_duration_ms("5m"), Some(300_000));
        assert_eq!(parse_duration_ms("2h"), Some(7_200_000));
        assert_eq!(parse_duration_ms("3d"), Some(259_200_000));
        assert_eq!(parse_duration_ms("2w"), Some(1_209_600_000));
    }

    #[test]
    fn parse_duration_rejects_garbage() {
        assert!(parse_duration_ms("3y").is_none()); // unsupported unit
        assert!(parse_duration_ms("foo").is_none());
        assert!(parse_duration_ms("").is_none());
    }

    #[test]
    fn parse_time_input_accepts_iso_date() {
        let ms = parse_time_input("2026-05-01").unwrap();
        // 2026-05-01 00:00 UTC = 1777_968_000_000 ms (sanity-check the order of magnitude)
        assert!(ms > 1_700_000_000_000);
        assert!(ms < 2_000_000_000_000);
    }

    #[test]
    fn parse_time_input_rejects_unparseable() {
        assert!(parse_time_input("not-a-time").is_err());
    }

    #[test]
    fn parse_cursor_splits_on_comma() {
        let c = parse_cursor("1_000,42").unwrap_err();
        // Strict: we don't accept underscores; this is a sanity test that we don't pretend to.
        assert!(c.to_string().contains("invalid digit") || c.to_string().contains("cursor"));

        let c = parse_cursor("1000,42").unwrap();
        assert_eq!(c.ts, 1000);
        assert_eq!(c.id, 42);
    }

    #[test]
    fn recall_resolves_today_to_a_day_long_window() {
        let (s, u) = parse_recall_phrase("today").unwrap();
        assert!(u - s >= 86_400_000 - 1, "today should span ~1 day");
        assert!(u - s <= 86_400_000 + 1);
    }

    #[test]
    fn recall_resolves_last_hour_to_one_hour() {
        let (s, u) = parse_recall_phrase("last hour").unwrap();
        let diff = u - s;
        assert!((3_595_000..=3_605_000).contains(&diff), "got {diff}");
    }

    #[test]
    fn recall_resolves_last_5_minutes() {
        let (s, u) = parse_recall_phrase("last 5 minutes").unwrap();
        let diff = u - s;
        assert!((295_000..=305_000).contains(&diff));
    }

    #[test]
    fn recall_rejects_unknown_phrase() {
        assert!(parse_recall_phrase("when alice last replied to me").is_err());
        assert!(parse_recall_phrase("during the meeting").is_err());
    }

    #[test]
    fn recall_lunch_is_a_90_minute_window_starting_at_noon() {
        let (s, u) = parse_recall_phrase("lunch").unwrap();
        let diff = u - s;
        assert_eq!(diff, 90 * 60_000);
    }

    #[test]
    fn replay_groups_consecutive_same_action_rows() {
        use mxr_protocol::{ActivityEntry, ActivityTier, ClientKind};
        let mk = |ts: i64, action: &str| ActivityEntry {
            id: ts,
            ts,
            account_id: None,
            source: ClientKind::Tui,
            action: action.into(),
            target_kind: None,
            target_id: None,
            tier: ActivityTier::Important,
            context: None,
            redacted: false,
        };
        let entries = vec![
            mk(0, "mail.read"),
            mk(10_000, "mail.read"),
            mk(20_000, "mail.read"),
            mk(30_000, "mail.archive"),
            mk(40_000, "mail.archive"),
            mk(900_000, "mail.archive"), // 15min later: new group
        ];
        let lines = group_for_replay(&entries);
        // Expected: 3 groups — mail.read×3, mail.archive×2, mail.archive×1
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("Read 3 thread"));
        assert!(lines[1].contains("Archived 2 thread"));
        assert!(lines[2].contains("Archived 1 thread"));
    }
}
