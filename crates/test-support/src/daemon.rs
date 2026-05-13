//! Daemon-spawning helpers shared between integration tests.
//!
//! Promoted from `crates/daemon/tests/cli_journey.rs` so any test
//! crate can spawn `mxr` against a fake-provider config without
//! re-implementing the same boilerplate.

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static DAEMON_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire the workspace-global daemon lock.
///
/// The `mxr` daemon's auto-start picks an instance-scoped socket
/// path, which is per-test. The lock guards the cargo build cache
/// (multiple integration tests trying to build `mxr` simultaneously
/// thrash) and the macOS-specific `Application Support/<instance>`
/// directory cleanup.
pub fn daemon_lock() -> MutexGuard<'static, ()> {
    DAEMON_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("daemon lock poisoned")
}

/// RAII guard that kills the spawned daemon and cleans up its
/// socket + pid files on drop.
pub struct DaemonGuard {
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
    pub pid: Option<u64>,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            let _ = StdCommand::new("kill").arg(pid.to_string()).status();
            for _ in 0..40 {
                if !self.socket_path.exists() {
                    break;
                }
                sleep(Duration::from_millis(50));
            }
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(&self.pid_path);
    }
}

/// Where the daemon expects its IPC socket for `instance`. Mirrors
/// the daemon's runtime layout exactly.
pub fn instance_socket_path(instance: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .expect("home dir")
            .join("Library")
            .join("Application Support")
            .join(instance)
            .join("mxr.sock")
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(instance)
            .join("mxr.sock")
    }
}

/// Generate an instance name unique to this process + timestamp so
/// concurrent test runs in the same workspace don't clash on the
/// socket path.
pub fn unique_instance_name(prefix: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("now")
        .as_nanos();
    format!("{prefix}-{}-{stamp}", std::process::id())
}

/// Write a config.toml that enables the fake sync + send providers
/// on a single account named `fake`. Disables the bridge to avoid
/// cross-test port contention.
pub fn write_fake_account_config(config_dir: &Path) {
    let toml = r#"[general]
default_account = "fake"

[bridge]
enabled = false

[accounts.fake]
name = "Fake Account"
email = "fake@example.com"

[accounts.fake.sync]
type = "fake"

[accounts.fake.send]
type = "fake"
"#;
    std::fs::write(config_dir.join("config.toml"), toml).expect("write fake config");
}

/// Captured stdout/stderr of an `mxr` subcommand invocation.
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
}

/// Run `mxr <args>` against the spawned daemon. Panics on non-zero
/// exit. Returns captured stdout + stderr.
pub fn run_status_only(
    instance: &str,
    data_dir: &Path,
    config_dir: &Path,
    args: &[&str],
) -> CliOutput {
    let output = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_INSTANCE", instance)
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(args)
        .assert()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    if !output.status.success() {
        panic!(
            "command {args:?} failed (exit {:?})\nstdout={stdout}\nstderr={stderr}",
            output.status.code()
        );
    }
    CliOutput { stdout, stderr }
}

/// Run `mxr <args>` and parse stdout as JSON. Panics on non-zero
/// exit OR JSON parse failure.
pub fn run_json(
    instance: &str,
    data_dir: &Path,
    config_dir: &Path,
    args: &[&str],
) -> Value {
    let out = run_status_only(instance, data_dir, config_dir, args);
    serde_json::from_str(out.stdout.trim()).unwrap_or_else(|err| {
        panic!(
            "expected JSON output for `mxr {}`; parse error: {err}\nstdout={}\nstderr={}",
            args.join(" "),
            out.stdout,
            out.stderr
        )
    })
}

/// Like `run_json` but pipes `stdin` into the subcommand.
pub fn run_json_with_stdin(
    instance: &str,
    data_dir: &Path,
    config_dir: &Path,
    args: &[&str],
    stdin: &str,
) -> Value {
    let output = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_INSTANCE", instance)
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(args)
        .write_stdin(stdin)
        .assert()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    if !output.status.success() {
        panic!(
            "command {args:?} failed (exit {:?})\nstdout={stdout}\nstderr={stderr}",
            output.status.code()
        );
    }
    serde_json::from_str(stdout.trim()).unwrap_or_else(|err| {
        panic!(
            "expected JSON output for `mxr {}`; parse error: {err}\nstdout={stdout}\nstderr={stderr}",
            args.join(" ")
        )
    })
}

/// Spawn a daemon with the fake-provider fixture. Returns the
/// guard, the instance name, and the data_dir + config_dir paths
/// (held alive by the caller's `TempDir`).
///
/// Caller responsibility: hold the returned `TempDir` for the
/// lifetime of the test (drop it AFTER the `DaemonGuard`).
pub fn spawn_fake_daemon(
    temp: &tempfile::TempDir,
    instance_prefix: &str,
) -> (DaemonGuard, String, PathBuf, PathBuf) {
    let instance = unique_instance_name(instance_prefix);
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_account_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path,
        pid_path,
        pid: None,
    };

    // Boot the daemon and capture its pid.
    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(
        daemon.pid.is_some(),
        "daemon should auto-start with status: {status:#}"
    );

    (daemon, instance, data_dir, config_dir)
}
