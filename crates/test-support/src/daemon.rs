
use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static DAEMON_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn daemon_lock() -> MutexGuard<'static, ()> {
    DAEMON_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

pub fn unique_instance_name(prefix: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("now")
        .as_nanos();
    format!("{prefix}-{}-{stamp}", std::process::id())
}

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

pub fn link_mxr_binary(dst: &Path) -> PathBuf {
    let src = assert_cmd::cargo::cargo_bin("mxr");
    if std::fs::hard_link(&src, dst).is_err() {
        std::fs::copy(&src, dst).expect("copy mxr binary");
    }
    dst.to_path_buf()
}

/// Run `<bin> <args>` with an explicit environment. Panics on non-zero exit.
pub fn run_with_env(bin: &Path, envs: &[(&str, &str)], args: &[&str]) -> CliOutput {
    let mut command = Command::new(bin);
    command
        .env_remove("EDITOR")
        .env_remove("VISUAL")
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

pub struct TimedLine {
    pub at: Duration,
    pub stream: &'static str,
    pub text: String,
}

pub struct TimedOutput {
    pub output: CliOutput,
    pub lines: Vec<TimedLine>,
    pub elapsed: Duration,
}

impl TimedOutput {
    pub fn first_line_containing(&self, marker: &str) -> Option<Duration> {
        self.lines
            .iter()
            .find(|line| line.stream == "out" && line.text.contains(marker))
            .map(|line| line.at)
    }

    pub fn timeline(&self) -> String {
        self.lines
            .iter()
            .map(|line| {
                format!(
                    "  +{:>7.3}s {}| {}",
                    line.at.as_secs_f64(),
                    line.stream,
                    line.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn read_timed_lines(
    mut source: impl std::io::Read,
    stream: &'static str,
    started_at: std::time::Instant,
) -> (String, Vec<TimedLine>) {
    let mut raw = String::new();
    let mut lines = Vec::new();
    let mut pending = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match source.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        raw.push(byte[0] as char);
        if byte[0] == b'\n' || byte[0] == b'\r' {
            if !pending.is_empty() {
                lines.push(TimedLine {
                    at: started_at.elapsed(),
                    stream,
                    text: String::from_utf8_lossy(&pending).trim_end().to_string(),
                });
                pending.clear();
            }
        } else {
            pending.push(byte[0]);
        }
    }
    if !pending.is_empty() {
        lines.push(TimedLine {
            at: started_at.elapsed(),
            stream,
            text: String::from_utf8_lossy(&pending).trim_end().to_string(),
        });
    }
    (raw, lines)
}

pub fn run_with_env_timed(bin: &Path, envs: &[(&str, &str)], args: &[&str]) -> TimedOutput {
    let mut command = StdCommand::new(bin);
    command
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .env_remove("MXR_DAEMON_ADDR")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let started_at = std::time::Instant::now();
    let mut child = command.args(args).spawn().expect("spawn command");

    let child_stderr = child.stderr.take().expect("piped stderr");
    let stderr_reader =
        std::thread::spawn(move || read_timed_lines(child_stderr, "err", started_at));
    let child_stdout = child.stdout.take().expect("piped stdout");
    let (stdout, mut lines) = read_timed_lines(child_stdout, "out", started_at);

    let status = child.wait().expect("wait for command");
    let elapsed = started_at.elapsed();
    let (stderr, stderr_lines) = stderr_reader.join().expect("join stderr reader");
    lines.extend(stderr_lines);
    lines.sort_by_key(|line| line.at);
    if !status.success() {
        panic!(
            "command {args:?} failed (exit {:?})\nstdout={stdout}\nstderr={stderr}",
            status.code()
        );
    }
    TimedOutput {
        output: CliOutput { stdout, stderr },
        lines,
        elapsed,
    }
}

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

        let _guard = daemon_lock();
    }
}
