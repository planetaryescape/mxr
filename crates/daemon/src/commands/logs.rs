use std::io::{BufRead, BufReader, Seek, SeekFrom};

pub fn run(no_follow: bool, level: Option<String>, _since: Option<String>) -> anyhow::Result<()> {
    let log_path = mxr_config::data_dir().join("logs").join("mxr.log");

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
            std::thread::sleep(std::time::Duration::from_millis(500));
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
                // File was truncated/rotated — reset
                pos = 0;
            }
        }
    }

    Ok(())
}
