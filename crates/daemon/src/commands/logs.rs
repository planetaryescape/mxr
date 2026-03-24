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
    let db_path = crate::mxr_config::data_dir().join("mxr.db");
    if !db_path.exists() {
        return Ok(0);
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let store = crate::mxr_store::Store::new(&db_path).await?;
        Ok::<u64, anyhow::Error>(store.prune_events_before(cutoff.timestamp()).await?)
    })
}

pub fn run(
    no_follow: bool,
    level: Option<String>,
    _since: Option<String>,
    purge: bool,
) -> anyhow::Result<()> {
    let data_dir = crate::mxr_config::data_dir();
    let log_dir = data_dir.join("logs");
    let log_path = log_dir.join("mxr.log");

    if purge {
        let config = crate::mxr_config::load_config().unwrap_or_default();
        let retention_days = config.logging.event_retention_days;
        let cutoff = SystemTime::now() - Duration::from_secs(retention_days as u64 * 24 * 60 * 60);
        let removed_files = purge_old_log_files(&log_dir, cutoff)?;
        let removed_events = purge_events(retention_days)?;
        println!(
            "Purged {} log file(s) and {} event log entr{}",
            removed_files,
            removed_events,
            if removed_events == 1 { "y" } else { "ies" }
        );
        return Ok(());
    }

    if !log_path.exists() {
        println!("No log file found at {}", log_path.display());
        return Ok(());
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        if let Some(ref lvl) = level {
            if !line.to_lowercase().contains(&lvl.to_lowercase()) {
                continue;
            }
        }
        println!("{}", line);
    }

    if !no_follow {
        println!("--- Following {} (Ctrl-C to stop) ---", log_path.display());
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
                    if let Some(ref lvl) = level {
                        if !line.to_lowercase().contains(&lvl.to_lowercase()) {
                            continue;
                        }
                    }
                    println!("{}", line);
                }
                pos = current_len;
            } else if current_len < pos {
                pos = 0;
            }
        }
    }

    Ok(())
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
