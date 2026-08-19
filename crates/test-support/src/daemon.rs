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
///
/// Poisoning is recovered from rather than propagated. The guarded value is
/// `()` — a serialisation token, not data with invariants a panicking test
/// could have left half-updated — so there is nothing for the next test to
/// observe in a broken state. Propagating instead turned one genuine failure
/// into N: the first test to panic poisoned the mutex and every test after it
/// died on `daemon lock poisoned`, burying the one real cause under a pile of
/// identical secondary failures.
pub fn daemon_lock() -> MutexGuard<'static, ()> {
    DAEMON_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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

/// Place a copy of the `mxr` binary at `dst` under a distinct file name.
///
/// The daemon's pid-file fallback finds a running daemon by scanning `ps` for
/// `<exe file name> daemon --instance <instance>`
/// (`server::fallback_live_daemon_pid_without_pid_file`). `mxr demo` always
/// uses the fixed `mxr-demo` instance, so a test driving the stock `mxr`
/// binary can adopt — and then restart or shut down — a developer's real demo
/// daemon, or another checkout's. Running under a unique file name keeps the
/// scan from matching anything but this test's own daemon.
///
/// Hard-links when the temp dir shares a filesystem with the build output and
/// copies otherwise; either way the name is what matters.
pub fn link_mxr_binary(dst: &Path) -> PathBuf {
    let src = assert_cmd::cargo::cargo_bin("mxr");
    if std::fs::hard_link(&src, dst).is_err() {
        std::fs::copy(&src, dst).expect("copy mxr binary");
    }
    dst.to_path_buf()
}

/// Run `<bin> <args>` with an explicit environment. Panics on non-zero exit.
///
/// The instance-scoped helpers above cover commands that accept
/// `MXR_INSTANCE` / `MXR_DATA_DIR` / `MXR_CONFIG_DIR`. `mxr demo` sets those
/// three itself and derives its profile from the OS config/data dirs, so its
/// tests have to isolate through `HOME` / `XDG_*`, pin the socket, and run a
/// uniquely-named binary (see [`link_mxr_binary`]).
pub fn run_with_env(bin: &Path, envs: &[(&str, &str)], args: &[&str]) -> CliOutput {
    let mut command = Command::new(bin);
    command
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        // Would override the socket the caller pinned.
        .env_remove("MXR_DAEMON_ADDR");
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.args(args).assert().get_output().clone();
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

/// Like [`run_with_env`] but parses stdout as JSON.
pub fn run_json_with_env(bin: &Path, envs: &[(&str, &str)], args: &[&str]) -> Value {
    let out = run_with_env(bin, envs, args);
    serde_json::from_str(out.stdout.trim()).unwrap_or_else(|err| {
        panic!(
            "expected JSON output for `mxr {}`; parse error: {err}\nstdout={}\nstderr={}",
            args.join(" "),
            out.stdout,
            out.stderr
        )
    })
}

/// Run `mxr <args>` and parse stdout as JSON. Panics on non-zero
/// exit OR JSON parse failure.
pub fn run_json(instance: &str, data_dir: &Path, config_dir: &Path, args: &[&str]) -> Value {
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

#[cfg(test)]
mod tests {
    use super::daemon_lock;

    /// One panicking test must stay one failing test. Before `daemon_lock`
    /// recovered from poisoning, the second acquisition here panicked with
    /// `daemon lock poisoned` — which is exactly how a single genuine
    /// integration failure used to present as four.
    ///
    /// The panic backtrace the helper thread prints is expected output.
    #[test]
    #[expect(
        clippy::panic,
        reason = "the test poisons the lock on purpose, which requires a panic"
    )]
    fn a_panic_under_the_lock_does_not_poison_the_next_acquisition() {
        let poisoner = std::thread::spawn(|| {
            let _held = daemon_lock();
            panic!("simulated integration failure while holding the daemon lock");
        });
        assert!(
            poisoner.join().is_err(),
            "the helper thread must actually panic, or this proves nothing"
        );

        // Would panic with `daemon lock poisoned` without the recovery.
        let _guard = daemon_lock();
    }
}
