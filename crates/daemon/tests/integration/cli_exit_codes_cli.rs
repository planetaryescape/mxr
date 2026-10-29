//! Subprocess-level checks of the documented exit codes (docs/blueprint/09-cli.md
//! "Exit codes"). These drive the real `mxr` binary with autostart disabled
//! and no socket, so nothing is started and nothing outside the temp dir is
//! touched.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests assert directly on fixtures"
)]

use std::process::{Command, Output};

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");

/// A private instance with temp config/data dirs and a socket path that is never created.
struct Isolated {
    _dir: tempfile::TempDir,
    envs: Vec<(String, String)>,
}

fn isolated(tag: &str) -> Isolated {
    let dir = tempfile::tempdir().unwrap();
    let envs = vec![
        (
            "MXR_INSTANCE".to_string(),
            format!("mxr-exit-codes-it-{tag}-{}", std::process::id()),
        ),
        (
            "MXR_CONFIG_DIR".to_string(),
            dir.path().join("config").display().to_string(),
        ),
        (
            "MXR_DATA_DIR".to_string(),
            dir.path().join("data").display().to_string(),
        ),
        (
            "MXR_SOCKET_PATH".to_string(),
            dir.path().join("missing.sock").display().to_string(),
        ),
        ("MXR_ACTIVITY".to_string(), "off".to_string()),
    ];
    Isolated { _dir: dir, envs }
}

fn run(isolated: &Isolated, args: &[&str]) -> Output {
    let mut command = Command::new(MXR_BIN);
    command
        .args(args)
        .env_remove("MXR_DAEMON_ADDR")
        .env_remove("MXR_DAEMON_TOKEN")
        .env("MXR_DAEMON_AUTOSTART", "off");
    for (key, value) in &isolated.envs {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn status_exits_3_when_no_daemon_is_running_and_autostart_is_off() {
    let isolated = isolated("unavailable");
    let output = run(&isolated, &["status"]);
    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("MXR_DAEMON_AUTOSTART=off"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn json_format_reports_daemon_unavailable_as_one_stderr_line() {
    let isolated = isolated("json");
    let output = run(&isolated, &["status", "--format", "json"]);
    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        output.stdout.is_empty(),
        "stdout must carry results only, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let last = stderr(&output)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string();
    let value: serde_json::Value = serde_json::from_str(&last).unwrap();
    assert_eq!(value["error"]["kind"], "daemon_unavailable");
    assert_eq!(value["error"]["exit_code"], 3);
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("MXR_DAEMON_AUTOSTART=off"));
}

#[test]
fn general_failure_still_exits_1_with_no_json_line() {
    let isolated = isolated("general");
    let missing = isolated._dir.path().join("no-such-bundle.mxrbackup");
    let output = run(&isolated, &["backup", "inspect", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("Error:"),
        "stderr: {}",
        stderr(&output)
    );
    let last = stderr(&output)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string();
    assert!(
        !last.starts_with("{\"error\""),
        "unexpected JSON line: {last}"
    );
}
