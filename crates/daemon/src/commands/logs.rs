#![cfg_attr(
    test,
    expect(clippy::unwrap_used, reason = "tests unwrap fixture failures")
)]
use crate::{cli::OutputFormat, logging};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct LogRunOptions {
    pub no_follow: bool,
    pub level: Option<String>,
    pub since: Option<String>,
    pub search: Option<String>,
    pub limit: u32,
    pub purge: bool,
    pub dry_run: bool,
    pub format: Option<OutputFormat>,
}

fn purge_selection(dir: &Path, cutoff: SystemTime) -> anyhow::Result<Vec<PathBuf>> {
    Ok(logging::archives(dir)?
        .into_iter()
        .filter(|p| {
            p.symlink_metadata()
                .is_ok_and(|m| m.is_file() && m.modified().is_ok_and(|t| t < cutoff))
        })
        .collect())
}

/// Counts (dry run) or deletes the history rows a retention prune selects.
/// Both tables use the store's predicates, so the preview and the real pass
/// select the same rows.
async fn purge_history(db: &Path, cutoff: i64, dry_run: bool) -> anyhow::Result<(u64, u64)> {
    if !db.exists() {
        return Ok((0, 0));
    }
    if dry_run {
        use sqlx::Connection;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(db)
            .read_only(true)
            .create_if_missing(false);
        let mut conn = sqlx::SqliteConnection::connect_with(&options).await?;
        let events = mxr_store::count_prunable_events(&mut conn, cutoff).await?;
        let sync_logs =
            mxr_store::count_prunable_sync_logs(&mut conn, cutoff, crate::sync_history::WINDOW)
                .await?;
        conn.close().await?;
        Ok((events, sync_logs))
    } else {
        let store = mxr_store::Store::new(db).await?;
        let events = store.prune_events_before(cutoff).await?;
        let sync_logs = store
            .prune_sync_logs_before(cutoff, crate::sync_history::WINDOW)
            .await?;
        Ok((events, sync_logs))
    }
}

pub async fn run(options: LogRunOptions) -> anyhow::Result<()> {
    let dir = mxr_config::data_dir().join("logs");
    let json_mode = matches!(
        options.format,
        Some(OutputFormat::Json | OutputFormat::Jsonl)
    );
    if options.purge {
        let days = mxr_config::load_config()?.logging.event_retention_days;
        // Same cutoff rule as the daily history prune. Event and sync rows
        // store whole Unix seconds, so this exact instant also selects the
        // archives and is reported back.
        let cutoff_secs = crate::loops::history_cutoff(Utc::now(), days);
        let cutoff = DateTime::from_timestamp(cutoff_secs, 0).ok_or_else(|| {
            anyhow::anyhow!("logging.event_retention_days exceeds supported cutoff range")
        })?;
        // Retention 0 keeps everything, so a manual purge deletes nothing.
        let (events, sync_logs, selected) = if days == 0 {
            (0, 0, Vec::new())
        } else {
            // Database work can wait for a writer. Never hold the archive lock
            // across that await: a database writer may itself need to trace.
            let db = mxr_config::data_dir().join("mxr.db");
            let (events, sync_logs) =
                purge_history(&db, cutoff.timestamp(), options.dry_run).await?;
            let selected = {
                // Preview opens an existing lock read-only, without creating logs.
                let _lock = if dir.exists() {
                    logging::lock(&dir, !options.dry_run)?
                } else {
                    None
                };
                let selected = purge_selection(&dir, SystemTime::from(cutoff))?;
                if !options.dry_run {
                    for path in &selected {
                        std::fs::remove_file(path)?;
                    }
                }
                selected
            };
            (events, sync_logs, selected)
        };
        if json_mode {
            println!(
                "{}",
                json!({"purged_log_files":selected.len(),"purged_event_rows":events,"purged_sync_log_rows":sync_logs,"dry_run":options.dry_run,"retention_days":days,"event_cutoff":cutoff.to_rfc3339(),"selected_log_files":selected})
            );
        } else if days == 0 {
            println!("Retention is 0 (keep forever); nothing purged");
        } else {
            println!(
                "{} {} owned archive(s), {} event row(s), and {} sync row(s) before {} (sync keeps the newest {} rows per account)",
                if options.dry_run {
                    "Would purge"
                } else {
                    "Purged"
                },
                selected.len(),
                events,
                sync_logs,
                cutoff.to_rfc3339(),
                crate::sync_history::WINDOW,
            );
        }
        return Ok(());
    }
    if !(1..=10_000).contains(&options.limit) {
        anyhow::bail!("log limit must be between 1 and 10000");
    }
    if let Some(level) = &options.level {
        if severity(level).is_none() {
            anyhow::bail!("unknown log level: {level}");
        }
    }
    let since = options.since.as_deref().map(parse_since).transpose()?;
    let mut positions = HashMap::new();
    let mut tail = VecDeque::new();
    scan(&dir, &mut positions, !options.no_follow, |line| {
        if line_passes(
            &line,
            options.level.as_deref(),
            options.search.as_deref(),
            since,
        ) {
            if tail.len() == options.limit as usize {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    })?;
    if positions.is_empty() {
        if json_mode {
            println!(
                "{}",
                json!({"warning":"no log file","path":dir.join("mxr.log")})
            );
        } else {
            println!("No log file found at {}", dir.join("mxr.log").display());
        }
    }
    for line in tail {
        emit_line(&line, json_mode);
    }
    if options.no_follow {
        return Ok(());
    }
    loop {
        std::thread::sleep(Duration::from_millis(500));
        scan(&dir, &mut positions, true, |line| {
            if line_passes(
                &line,
                options.level.as_deref(),
                options.search.as_deref(),
                since,
            ) {
                emit_line(&line, json_mode);
            }
        })?;
    }
}

type FileId = (u64, u64);

fn scan(
    dir: &Path,
    positions: &mut HashMap<FileId, u64>,
    wait_for_newline: bool,
    mut emit: impl FnMut(String),
) -> anyhow::Result<()> {
    // Pin inode handles and lengths while writers cannot rotate. Release the
    // lock before scanning history, so a large read does not stall the daemon.
    let snapshots = {
        let _lock = logging::lock(dir, false)?;
        let mut snapshots = Vec::new();
        for path in logging::files(dir)? {
            let file = logging::open_read(&path)?;
            let meta = file.metadata()?;
            snapshots.push((file, (meta.dev(), meta.ino()), meta.len()));
        }
        snapshots
    };
    let mut present = HashSet::new();
    for (mut file, id, len) in snapshots {
        present.insert(id);
        let mut offset = positions
            .get(&id)
            .copied()
            .filter(|p| *p <= len)
            .unwrap_or(0);
        file.seek(SeekFrom::Start(offset))?;
        let mut reader = BufReader::new(file.take(len - offset));
        loop {
            let (line, consumed, complete) = read_record(&mut reader)?;
            if consumed == 0 || (!complete && wait_for_newline) {
                break;
            }
            offset += consumed;
            if let Some(line) = line {
                emit(line);
            }
        }
        positions.insert(id, offset);
    }
    positions.retain(|id, _| present.contains(id));
    Ok(())
}

/// Cap memory even when reading a legacy/untrusted file with a huge line.
fn read_record(reader: &mut impl BufRead) -> std::io::Result<(Option<String>, u64, bool)> {
    let mut bytes = Vec::new();
    let mut consumed = 0;
    let mut oversized = false;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok((
                (!oversized && !bytes.is_empty())
                    .then(|| String::from_utf8_lossy(&bytes).into_owned()),
                consumed,
                false,
            ));
        }
        let end = buf.iter().position(|b| *b == b'\n').map(|p| p + 1);
        let take = end.unwrap_or(buf.len());
        if !oversized && bytes.len().saturating_add(take) <= logging::MAX_RECORD_BYTES {
            bytes.extend_from_slice(&buf[..take]);
        } else {
            bytes.clear();
            oversized = true;
        }
        consumed += take as u64;
        reader.consume(take);
        if end.is_some() {
            if bytes.last() == Some(&b'\n') {
                bytes.pop();
            }
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok((
                (!oversized).then(|| String::from_utf8_lossy(&bytes).into_owned()),
                consumed,
                true,
            ));
        }
    }
}

fn parsed_line(line: &str) -> Value {
    if let Ok(value) = serde_json::from_str::<Value>(line) {
        if value.is_object() {
            return json!({"timestamp":value["timestamp"].as_str().unwrap_or(""),"level":value["level"].as_str().unwrap_or(""),"message":value["fields"]["message"].as_str().unwrap_or(""),"raw":line});
        }
    }
    let mut parts = line.splitn(3, ' ');
    json!({"timestamp":parts.next().unwrap_or(""),"level":parts.next().unwrap_or("").trim(),"message":parts.next().unwrap_or(""),"raw":line})
}

fn emit_line(line: &str, json_mode: bool) {
    if json_mode {
        println!("{}", parsed_line(line));
    } else {
        println!("{line}");
    }
}
fn severity(level: &str) -> Option<u8> {
    match level.to_ascii_uppercase().as_str() {
        "TRACE" => Some(0),
        "DEBUG" => Some(1),
        "INFO" => Some(2),
        "WARN" => Some(3),
        "ERROR" => Some(4),
        _ => None,
    }
}
fn line_passes(
    line: &str,
    level: Option<&str>,
    search: Option<&str>,
    since: Option<DateTime<Utc>>,
) -> bool {
    let parsed = parsed_line(line);
    if let Some(level) = level {
        if !parsed["level"]
            .as_str()
            .and_then(severity)
            .zip(severity(level))
            .is_some_and(|(actual, wanted)| actual >= wanted)
        {
            return false;
        }
    }
    if search.is_some_and(|needle| !line.to_lowercase().contains(&needle.to_lowercase())) {
        return false;
    }
    if let Some(cutoff) = since {
        if !parsed["timestamp"]
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|ts| ts >= cutoff)
        {
            return false;
        }
    }
    true
}

fn parse_since(value: &str) -> anyhow::Result<DateTime<Utc>> {
    if let Ok(ts) = DateTime::parse_from_rfc3339(value.trim()) {
        return Ok(ts.with_timezone(&Utc));
    }
    if let Some(duration) = parse_relative_duration(value.trim()) {
        if let Some(cutoff) = Utc::now().checked_sub_signed(duration) {
            return Ok(cutoff);
        }
    }
    anyhow::bail!("Could not parse --since; use RFC 3339 or a duration such as 10m, 2h or 1d")
}
fn parse_relative_duration(value: &str) -> Option<chrono::Duration> {
    let (num, unit) = value.split_at(value.find(|c: char| !c.is_ascii_digit())?);
    let amount: i64 = num.parse().ok()?;
    let multiplier = match unit {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 3600,
        "d" | "day" | "days" => 86400,
        _ => return None,
    };
    chrono::Duration::try_seconds(amount.checked_mul(multiplier)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn purge_dry_run_counts_both_tables_and_deletes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("mxr.db");
        let store = mxr_store::Store::new(&db).await.unwrap();
        let account = mxr_core::Account {
            id: mxr_core::AccountId::new(),
            name: "Test".into(),
            email: "test@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.unwrap();
        for i in 0..3 {
            store
                .insert_event("info", "sync", &format!("Sync {i}"), None, None)
                .await
                .unwrap();
        }
        for _ in 0..55 {
            store
                .insert_sync_log(&account.id, &mxr_store::SyncStatus::Success)
                .await
                .unwrap();
        }
        drop(store);
        // Rows were written now, so a cutoff 91 days ahead selects them all.
        let cutoff = Utc::now().timestamp() + 91 * 86_400 - 90 * 86_400;

        let preview = purge_history(&db, cutoff, true).await.unwrap();
        let rows_after_preview = {
            let store = mxr_store::Store::new(&db).await.unwrap();
            (
                store
                    .list_recent_sync_logs(&account.id, 1_000)
                    .await
                    .unwrap()
                    .len(),
                store
                    .latest_event_timestamp("sync", None)
                    .await
                    .unwrap()
                    .is_some(),
            )
        };
        let real = purge_history(&db, cutoff, false).await.unwrap();

        assert_eq!(preview, (3, 5), "preview counts both tables");
        assert_eq!(
            real, preview,
            "real pass deletes exactly what preview counted"
        );
        assert_eq!(rows_after_preview, (55, true), "preview deleted nothing");
    }

    #[test]
    fn scan_follows_retained_rotation_without_repeating_old_lines_and_waits_for_partial_line() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let active = dir.path().join("mxr.log");
        std::fs::write(&active, "a\nb\n").unwrap();
        let mut positions = HashMap::new();
        let mut lines = Vec::new();
        scan(dir.path(), &mut positions, true, |l| lines.push(l)).unwrap();
        assert_eq!(lines, ["a", "b"]);
        let archive = dir.path().join("mxr.00000000000000000000.log");
        std::fs::rename(&active, &archive).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&archive)
            .unwrap()
            .write_all(b"c\n")
            .unwrap();
        std::fs::write(&active, "d\npart").unwrap();
        lines.clear();
        scan(dir.path(), &mut positions, true, |l| lines.push(l)).unwrap();
        assert_eq!(lines, ["c", "d"]);
        std::fs::OpenOptions::new()
            .append(true)
            .open(&active)
            .unwrap()
            .write_all(b"ial\n")
            .unwrap();
        lines.clear();
        scan(dir.path(), &mut positions, true, |l| lines.push(l)).unwrap();
        assert_eq!(lines, ["partial"]);
        lines.clear();
        scan(dir.path(), &mut positions, true, |l| lines.push(l)).unwrap();
        assert!(lines.is_empty());
    }
    #[test]
    fn bounded_reader_handles_oversize_and_incomplete_records() {
        let text = format!(
            "{}\nkept\npartial",
            "x".repeat(logging::MAX_RECORD_BYTES + 1)
        );
        let mut reader = BufReader::new(text.as_bytes());
        let first = read_record(&mut reader).unwrap();
        assert!(first.0.is_none() && first.2);
        assert_eq!(read_record(&mut reader).unwrap().0.as_deref(), Some("kept"));
        let last = read_record(&mut reader).unwrap();
        assert_eq!(last.0.as_deref(), Some("partial"));
        assert!(!last.2);
    }
    #[test]
    fn unparseable_since_and_level_cannot_match_decoy_fields() {
        assert!(!line_passes(
            "not a timestamp ERROR",
            None,
            None,
            Some(Utc::now())
        ));
        assert!(!line_passes(
            r#"{"level":"INFO","fields":{"message":"ERROR"}}"#,
            Some("error"),
            None,
            None
        ));
        assert!(parse_since("999999999999999999999d").is_err());
    }
}
