use crate::cli::OutputFormat;
use crate::output::resolve_format;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HealthReport {
    healthy: bool,
    data_dir_exists: bool,
    database_exists: bool,
    index_exists: bool,
    socket_exists: bool,
}

fn evaluate_health(
    data_dir_exists: bool,
    database_exists: bool,
    index_exists: bool,
    socket_exists: bool,
) -> HealthReport {
    HealthReport {
        healthy: data_dir_exists && database_exists && index_exists && socket_exists,
        data_dir_exists,
        database_exists,
        index_exists,
        socket_exists,
    }
}

pub fn run(
    reindex: bool,
    check: bool,
    index_stats: bool,
    store_stats: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");
    let socket_path = crate::state::AppState::socket_path();

    if check {
        let report = evaluate_health(
            data_dir.exists(),
            db_path.exists(),
            index_path.exists(),
            socket_path.exists(),
        );
        match fmt {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "healthy": report.healthy,
                        "data_dir_exists": report.data_dir_exists,
                        "database_exists": report.database_exists,
                        "index_exists": report.index_exists,
                        "socket_exists": report.socket_exists,
                    }))?
                );
            }
            _ => {
                println!(
                    "healthy={} data_dir={} database={} index={} socket={}",
                    report.healthy,
                    report.data_dir_exists,
                    report.database_exists,
                    report.index_exists,
                    report.socket_exists
                );
            }
        }
        if report.healthy {
            return Ok(());
        }
        anyhow::bail!("mxr health check failed");
    }

    if index_stats {
        let size_bytes = dir_size(&index_path);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": index_path.display().to_string(),
                "exists": index_path.exists(),
                "size_bytes": size_bytes,
            }))?
        );
        return Ok(());
    }

    if store_stats {
        let size_bytes = file_size(&db_path);
        let log_path = data_dir.join("logs").join("mxr.log");
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "database_path": db_path.display().to_string(),
                "database_size_bytes": size_bytes,
                "log_path": log_path.display().to_string(),
                "log_size_bytes": file_size(&log_path),
            }))?
        );
        return Ok(());
    }

    println!("Data dir:     {}", data_dir.display());
    println!(
        "Database:     {} (exists: {})",
        db_path.display(),
        db_path.exists()
    );
    println!(
        "Search index: {} (exists: {})",
        index_path.display(),
        index_path.exists()
    );
    println!(
        "Socket:       {} (exists: {})",
        socket_path.display(),
        socket_path.exists()
    );
    println!("Config:       {}", mxr_config::config_file_path().display());

    if reindex {
        println!("\nReindex requested - this requires daemon restart to take effect.");
        if index_path.exists() {
            std::fs::remove_dir_all(&index_path)?;
            println!("Removed search index directory. Restart daemon to rebuild.");
        }
    }

    Ok(())
}

fn file_size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn dir_size(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_health_requires_all_components() {
        let report = evaluate_health(true, true, false, true);
        assert!(!report.healthy);
    }

    #[test]
    fn evaluate_health_marks_all_present_as_healthy() {
        let report = evaluate_health(true, true, true, true);
        assert!(report.healthy);
    }
}
