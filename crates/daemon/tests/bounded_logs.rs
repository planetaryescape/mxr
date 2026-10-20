#![expect(
    clippy::unwrap_used,
    reason = "synthetic fixtures unwrap direct failures"
)]
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

struct OwnedChild(std::process::Child);
impl std::ops::Deref for OwnedChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn profile() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("config")).unwrap();
    std::fs::write(
        dir.path().join("config/config.toml"),
        "[logging]\nlevel = 'info'\nmax_size_mb = 1\nmax_files = 3\nstderr = false\n",
    )
    .unwrap();
    dir
}
fn env(command: &mut Command, root: &std::path::Path) {
    command
        .env("HOME", root)
        .env("MXR_CONFIG_DIR", root.join("config"))
        .env("MXR_DATA_DIR", root.join("data"))
        .env("MXR_DAEMON_AUTOSTART", "off")
        .env("MXR_ACTIVITY", "off")
        .env_remove("RUST_LOG")
        .env_remove("MXR_DAEMON_ADDR");
}
fn cli(root: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mxr"));
    env(&mut cmd, root);
    cmd
}

#[test]
fn writer_child() {
    if std::env::var_os("MXR_LOG_CHILD").is_none() {
        return;
    }
    mxr::init_tracing(std::env::var_os("MXR_LOG_MIRROR").is_some()).unwrap();
    for seq in 0..160 {
        tracing::info!(target: "mxr::logging_test", seq, payload = %"x".repeat(12_000), "synthetic operation");
    }
    tracing::info!(target: "mxr::logging_test", payload = %"s".repeat(2_000_000), "oversized synthetic event");
    tracing::trace!(target: "async_imap::imap_stream", secret = "SYNTHETIC_WIRE_SECRET");
    tracing::trace!(target: "tungstenite::protocol::frame", secret = "SYNTHETIC_WIRE_SECRET");
    tracing::info!(target: "mxr::logging_test", "final synthetic operation");
}

#[test]
fn stderr_setting_never_turns_foreground_tracing_into_stdout() {
    let dir = profile();
    for enabled in [false, true] {
        std::fs::write(
            dir.path().join("config/config.toml"),
            format!("[logging]\nmax_size_mb=1\nmax_files=3\nstderr={enabled}\n"),
        )
        .unwrap();
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        env(&mut cmd, dir.path());
        let output = cmd
            .args(["--exact", "writer_child", "--nocapture"])
            .env("MXR_LOG_CHILD", "1")
            .env("MXR_LOG_MIRROR", "1")
            .env("RUST_LOG", "trace")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic operation"));
        assert_eq!(
            String::from_utf8_lossy(&output.stderr).contains("synthetic operation"),
            enabled
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("SYNTHETIC_WIRE_SECRET"));
    }
}

#[tokio::test]
async fn actual_purge_preview_preserves_active_unknown_and_database_then_apply_matches() {
    let dir = profile();
    let data = dir.path().join("data");
    let logs = data.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let archive = logs.join("mxr.00000000000000000000.log");
    let recent = logs.join("mxr.00000000000000000001.log");
    let active = logs.join("mxr.log");
    let unknown = logs.join("unrelated.log");
    let lock = logs.join(".mxr-log.lock");
    for path in [&archive, &recent, &active, &unknown, &lock] {
        std::fs::write(path, "fixture\n").unwrap();
    }
    let old = std::time::SystemTime::now() - Duration::from_secs(120 * 86400);
    for path in [&archive, &active, &unknown, &lock] {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    let symlink = logs.join("mxr.00000000000000000002.log");
    std::os::unix::fs::symlink(&unknown, &symlink).unwrap();
    let store = mxr_store::Store::new(&data.join("mxr.db")).await.unwrap();
    for days in [120, 1] {
        sqlx::query("INSERT INTO event_log (timestamp, level, category, summary) VALUES (?, 'info', 'synthetic', 'safe fixture')").bind((chrono::Utc::now()-chrono::Duration::days(days)).timestamp()).execute(store.writer()).await.unwrap();
    }
    let before = std::fs::read(&active).unwrap();
    let preview = cli(dir.path())
        .args(["logs", "--purge", "--dry-run", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["purged_log_files"], 1);
    assert_eq!(preview["purged_event_rows"], 1);
    assert!(archive.exists());
    assert_eq!(std::fs::read(&active).unwrap(), before);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(count, 2);
    let applied = cli(dir.path())
        .args(["logs", "--purge", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(applied["purged_log_files"], preview["purged_log_files"]);
    assert_eq!(applied["purged_event_rows"], preview["purged_event_rows"]);
    assert!(!archive.exists());
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(remaining, 1, "recent event remains after manual prune");
    for path in [&recent, &active, &unknown, &lock, &symlink] {
        assert!(
            path.symlink_metadata().is_ok(),
            "preserved {}",
            path.display()
        );
    }
}

#[test]
fn purge_preview_of_missing_profile_creates_no_data_or_logs() {
    let dir = profile();
    let output = cli(dir.path())
        .args(["logs", "--purge", "--dry-run", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["purged_log_files"], 0);
    assert_eq!(preview["purged_event_rows"], 0);
    assert!(!dir.path().join("data").exists());
}

#[test]
fn real_initialization_bounds_complete_records_across_processes_and_restart() {
    let dir = profile();
    for _ in 0..2 {
        let mut children = Vec::new();
        for _ in 0..3 {
            let mut cmd = Command::new(std::env::current_exe().unwrap());
            env(&mut cmd, dir.path());
            children.push(
                cmd.args(["--exact", "writer_child", "--nocapture"])
                    .env("MXR_LOG_CHILD", "1")
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
        }
        for child in children {
            assert!(child.wait_with_output().unwrap().status.success());
        }
        let logs = dir.path().join("data/logs");
        let mut files = 0;
        let mut omissions = 0;
        let mut final_records = 0;
        for entry in std::fs::read_dir(logs).unwrap() {
            let entry = entry.unwrap();
            if entry.path().extension().is_none_or(|ext| ext != "log") {
                continue;
            }
            files += 1;
            assert!(
                entry.metadata().unwrap().len() <= 1024 * 1024,
                "file exceeds configured MiB cap"
            );
            let text = std::fs::read_to_string(entry.path()).unwrap();
            assert!(!text.contains("SYNTHETIC_WIRE_SECRET"));
            for line in text.lines() {
                let record: serde_json::Value =
                    serde_json::from_str(line).expect("complete JSON record");
                let message = record["fields"]["message"].as_str().unwrap_or("");
                omissions +=
                    usize::from(message == "log record omitted: exceeds bounded record/file size");
                final_records += usize::from(message == "final synthetic operation");
            }
        }
        assert_eq!(
            files, 3,
            "rollover retains two archives plus the active file"
        );
        assert!(
            omissions > 0,
            "oversized events produce readable diagnostics"
        );
        assert!(
            final_records > 0,
            "writers still append after oversized events"
        );
    }
}

#[test]
fn json_filters_use_fields_and_preserve_raw_and_legacy_text() {
    let dir = profile();
    let logs = dir.path().join("data/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let actual = r#"{"timestamp":"2026-10-20T10:00:00Z","level":"ERROR","fields":{"message":"synthetic failure"},"target":"mxr"}"#;
    std::fs::write(logs.join("mxr.log"), format!("{{\"timestamp\":\"2020-01-01T00:00:00Z\",\"level\":\"INFO\",\"fields\":{{\"message\":\"ERROR decoy\"}}}}\n{actual}\n2026-10-20T10:00:01Z WARN legacy message\n")).unwrap();
    let output = cli(dir.path())
        .args([
            "logs",
            "--no-follow",
            "--level",
            "warn",
            "--since",
            "2026-10-20T00:00:00Z",
            "--format",
            "jsonl",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["level"], "ERROR");
    assert_eq!(rows[0]["timestamp"], "2026-10-20T10:00:00Z");
    assert_eq!(rows[0]["message"], "synthetic failure");
    assert_eq!(rows[0]["raw"], actual);
    assert_eq!(rows[1]["level"], "WARN");
}

#[test]
fn real_cli_follow_reads_replacement_inode_even_when_larger() {
    let dir = profile();
    let logs = dir.path().join("data/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let path = logs.join("mxr.log");
    std::fs::write(&path, "2026-10-20T10:00:00Z INFO initial\n").unwrap();
    let mut child = OwnedChild(
        cli(dir.path())
            .args(["logs", "--format", "jsonl"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let first = rx.recv_timeout(Duration::from_secs(10));
    assert!(
        first.is_ok(),
        "initial log tail missing; child status {:?}",
        child.try_wait()
    );
    let first = first.unwrap();
    assert!(first.contains("initial"));
    std::fs::rename(&path, logs.join("external-old")).unwrap();
    std::fs::write(
        &path,
        "2026-10-20T10:00:01Z INFO replacement-with-longer-content\n",
    )
    .unwrap();
    let received = rx.recv_timeout(Duration::from_secs(3));
    assert!(received
        .unwrap()
        .contains("replacement-with-longer-content"));
    std::fs::rename(&path, logs.join("external-second")).unwrap();
    std::fs::write(
        &path,
        "2026-10-20T10:00:02Z INFO replacement-with-equal--content\n",
    )
    .unwrap();
    let equal = rx.recv_timeout(Duration::from_secs(3));
    child.kill().unwrap();
    child.wait().unwrap();
    reader.join().unwrap();
    assert!(equal.unwrap().contains("replacement-with-equal--content"));
}
