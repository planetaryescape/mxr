#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, SystemTime};

fn purge_old_log_files(log_dir: &Path, cutoff: SystemTime) -> anyhow::Result<usize> {
    if !log_dir.exists() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified < cutoff {
            std::fs::remove_file(path)?;
            removed += 1;
        }
    }

    Ok(removed)
}

fn purge_events(retention_days: u32) -> anyhow::Result<u64> {
    let db_path = mxr_config::data_dir().join("mxr.db");
    if !db_path.exists() {
        return Ok(0);
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let store = mxr_store::Store::new(&db_path).await?;
        Ok::<u64, anyhow::Error>(store.prune_events_before(cutoff.timestamp()).await?)
    })
}

pub fn run(
    no_follow: bool,
    level: Option<String>,
    since: Option<String>,
    purge: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let data_dir = mxr_config::data_dir();
    let log_dir = data_dir.join("logs");
    let log_path = log_dir.join("mxr.log");
    let json_mode = matches!(format, Some(OutputFormat::Json) | Some(OutputFormat::Jsonl));

    if purge {
        let config = mxr_config::load_config().unwrap_or_default();
        let retention_days = config.logging.event_retention_days;
        let cutoff = SystemTime::now() - Duration::from_secs(retention_days as u64 * 24 * 60 * 60);
        let removed_files = purge_old_log_files(&log_dir, cutoff)?;
        let removed_events = purge_events(retention_days)?;
        if json_mode {
            println!(
                "{}",
                json!({
                    "purged_log_files": removed_files,
                    "purged_event_rows": removed_events,
                })
            );
        } else {
            println!(
                "Purged {} log file(s) and {} event log entr{}",
                removed_files,
                removed_events,
                if removed_events == 1 { "y" } else { "ies" }
            );
        }
        return Ok(());
    }

    let since_cutoff = match since.as_deref() {
        Some(value) => Some(parse_since(value)?),
        None => None,
    };

    if !log_path.exists() {
        if json_mode {
            println!(
                "{}",
                json!({"warning": "no log file", "path": log_path.display().to_string()})
            );
        } else {
            println!("No log file found at {}", log_path.display());
        }
        return Ok(());
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if !line_passes(&line, level.as_deref(), since_cutoff) {
            continue;
        }
        emit_line(&line, json_mode);
    }

    if !no_follow {
        if !json_mode {
            println!("--- Following {} (Ctrl-C to stop) ---", log_path.display());
        }
        let mut pos = std::fs::metadata(&log_path)?.len();
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let current_len = match std::fs::metadata(&log_path) {
                Ok(m) => m.len(),
                Err(_) => continue,
            };
            if current_len > pos {
                let mut file = std::fs::File::open(&log_path)?;
                file.seek(SeekFrom::Start(pos))?;
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    let line = line?;
                    if !line_passes(&line, level.as_deref(), since_cutoff) {
                        continue;
                    }
                    emit_line(&line, json_mode);
                }
                pos = current_len;
            } else if current_len < pos {
                pos = 0;
            }
        }
    }

    Ok(())
}

fn emit_line(line: &str, json_mode: bool) {
    if !json_mode {
        println!("{line}");
        return;
    }
    let mut parts = line.splitn(3, ' ');
    let timestamp = parts.next().unwrap_or("");
    let level = parts.next().unwrap_or("").trim();
    let message = parts.next().unwrap_or("");
    println!(
        "{}",
        json!({
            "timestamp": timestamp,
            "level": level,
            "message": message,
            "raw": line,
        })
    );
}

fn line_passes(line: &str, level: Option<&str>, since: Option<DateTime<Utc>>) -> bool {
    if let Some(lvl) = level {
        if !line.to_lowercase().contains(&lvl.to_lowercase()) {
            return false;
        }
    }
    if let Some(cutoff) = since {
        if let Some(ts) = parse_log_line_timestamp(line) {
            if ts < cutoff {
                return false;
            }
        }
    }
    true
}

/// Accept either an RFC 3339 timestamp (e.g. `2026-05-03T12:00:00Z`) or a
/// short relative form like `10m`, `2h`, `3d`.
fn parse_since(value: &str) -> anyhow::Result<DateTime<Utc>> {
    let trimmed = value.trim();
    if let Ok(ts) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(ts.with_timezone(&Utc));
    }
    if let Some(duration) = parse_relative_duration(trimmed) {
        return Ok(Utc::now() - duration);
    }
    anyhow::bail!(
        "Could not parse `--since {value}`. Use RFC 3339 (2026-05-03T12:00:00Z) or a relative duration like `10m`, `2h`, `1d`."
    )
}

fn parse_relative_duration(value: &str) -> Option<chrono::Duration> {
    let (num, unit) = value.split_at(value.find(|c: char| !c.is_ascii_digit())?);
    let amount: i64 = num.parse().ok()?;
    match unit {
        "s" | "sec" | "secs" => Some(chrono::Duration::seconds(amount)),
        "m" | "min" | "mins" => Some(chrono::Duration::minutes(amount)),
        "h" | "hr" | "hrs" => Some(chrono::Duration::hours(amount)),
        "d" | "day" | "days" => Some(chrono::Duration::days(amount)),
        _ => None,
    }
}

fn parse_log_line_timestamp(line: &str) -> Option<DateTime<Utc>> {
    // tracing-subscriber emits an RFC 3339 timestamp at the start of each line.
    let candidate = line.split_whitespace().next()?;
    DateTime::parse_from_rfc3339(candidate)
        .ok()
        .map(|ts| ts.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purge_old_log_files_removes_only_older_files() {
        let dir = std::env::temp_dir().join(format!("mxr-logs-test-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let old_file = dir.join("old.log");
        let new_file = dir.join("new.log");
        std::fs::write(&old_file, "old").unwrap();
        std::fs::write(&new_file, "new").unwrap();

        let old_timestamp = (chrono::Utc::now() - chrono::Duration::days(120))
            .format("%Y%m%d%H%M.%S")
            .to_string();
        let status = std::process::Command::new("touch")
            .arg("-t")
            .arg(old_timestamp)
            .arg(&old_file)
            .status()
            .unwrap();
        assert!(status.success());

        let removed = purge_old_log_files(
            &dir,
            SystemTime::now() - Duration::from_secs(90 * 24 * 60 * 60),
        )
        .unwrap();

        assert_eq!(removed, 1);
        assert!(!old_file.exists());
        assert!(new_file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
