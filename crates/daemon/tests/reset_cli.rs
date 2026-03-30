use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::thread::sleep;
use std::time::Duration;
use tempfile::TempDir;

struct TestDaemon {
    socket_path: PathBuf,
    pid: Option<u64>,
}

impl TestDaemon {
    fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            pid: None,
        }
    }

    fn set_pid(&mut self, pid: u64) {
        self.pid = Some(pid);
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            if process_is_alive(pid) {
                let _ = StdCommand::new("kill").arg(pid.to_string()).status();
            }
            for _ in 0..20 {
                if !self.socket_path.exists() {
                    break;
                }
                sleep(Duration::from_millis(50));
            }
        }

        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[test]
fn reset_requires_hard_flag() {
    Command::cargo_bin("mxr")
        .expect("mxr binary")
        .args(["reset"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--hard"));
}

#[test]
fn reset_non_interactive_refuses_without_explicit_override() {
    let temp = TempDir::new().expect("temp dir");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = temp.path().join("runtime").join("mxr.sock");

    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env("MXR_SOCKET_PATH", &socket_path)
        .args(["reset", "--hard"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--yes-i-understand-this-destroys-local-state",
        ));
}

#[test]
fn reset_dry_run_and_destructive_override_handle_daemon_runtime_state() {
    let temp = TempDir::new().expect("temp dir");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = temp.path().join("runtime").join("mxr.sock");
    let config_path = config_dir.join("config.toml");

    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(&config_path, "[general]\nsync_interval = 60\n").expect("write config");

    let mut daemon = TestDaemon::new(socket_path.clone());
    let status = run_status(&data_dir, &config_dir, &socket_path);
    let daemon_pid = parse_status(&status.0)["daemon_pid"]
        .as_u64()
        .expect("daemon pid");
    daemon.set_pid(daemon_pid);

    create_runtime_artifacts(&data_dir);

    let dry_run = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env("MXR_SOCKET_PATH", &socket_path)
        .args(["reset", "--hard", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .clone();
    let dry_run_stdout = String::from_utf8(dry_run.stdout).expect("utf8 stdout");
    assert!(dry_run_stdout.contains("Daemon state: running"));
    assert!(dry_run_stdout.contains("Shutdown attempt: yes"));
    assert!(dry_run_stdout.contains(&config_path.display().to_string()));
    assert!(dry_run_stdout.contains("credentials/keychain"));
    assert!(socket_path.exists(), "dry-run should not stop daemon");

    Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env("MXR_SOCKET_PATH", &socket_path)
        .args([
            "reset",
            "--hard",
            "--yes-i-understand-this-destroys-local-state",
        ])
        .assert()
        .success();

    assert!(config_path.exists(), "config file should be preserved");
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("config contents"),
        "[general]\nsync_interval = 60\n"
    );
    assert!(
        !socket_path.exists(),
        "socket should be removed after reset"
    );
    wait_for_process_exit(daemon_pid);
    assert!(
        !process_is_alive(daemon_pid),
        "daemon pid {daemon_pid} should have exited after reset"
    );
    assert!(!data_dir.exists(), "data dir should be removed when empty");

    Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env("MXR_SOCKET_PATH", &socket_path)
        .args([
            "reset",
            "--hard",
            "--yes-i-understand-this-destroys-local-state",
        ])
        .assert()
        .success();
}

fn create_runtime_artifacts(data_dir: &Path) {
    std::fs::create_dir_all(data_dir.join("models")).expect("models dir");
    std::fs::create_dir_all(data_dir.join("logs")).expect("logs dir");
    std::fs::create_dir_all(data_dir.join("source")).expect("source dir");
    std::fs::create_dir_all(data_dir.join("attachments").join("message-1"))
        .expect("attachments dir");
    std::fs::create_dir_all(data_dir.join("cache")).expect("cache dir");
    std::fs::write(data_dir.join("models").join("model.bin"), "model").expect("model file");
    std::fs::write(data_dir.join("logs").join("mxr.log"), "log").expect("log file");
    std::fs::write(data_dir.join("source").join("message.txt"), "source").expect("source file");
    std::fs::write(
        data_dir
            .join("attachments")
            .join("message-1")
            .join("file.txt"),
        "attachment",
    )
    .expect("attachment file");
    std::fs::write(data_dir.join("cache").join("scratch.tmp"), "scratch").expect("cache file");
}

fn run_status(data_dir: &Path, config_dir: &Path, socket_path: &Path) -> (String, String) {
    let output = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
        .env("MXR_SOCKET_PATH", socket_path)
        .args(["status", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();

    (
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        String::from_utf8(output.stderr).expect("utf8 stderr"),
    )
}

fn parse_status(stdout: &str) -> Value {
    serde_json::from_str(stdout).expect("status json")
}

fn process_is_alive(pid: u64) -> bool {
    StdCommand::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn wait_for_process_exit(pid: u64) {
    for _ in 0..40 {
        if !process_is_alive(pid) {
            return;
        }
        sleep(Duration::from_millis(100));
    }
}
