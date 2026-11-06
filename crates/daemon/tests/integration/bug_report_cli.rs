//! Integration tests for `mxr bug-report` sanitization warnings. An
//! unsanitized report must announce itself on stderr before any output,
//! and must leave stdout clean so piping stays safe. No daemon is needed:
//! the report degrades gracefully when the store and logs are absent.

use std::process::Command;
use tempfile::TempDir;

const WARNING_PREFIX: &str = "warning: this report is not sanitized.";

fn isolated_command(root: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mxr"));
    cmd.env("HOME", root)
        .env("MXR_CONFIG_DIR", root.join("config"))
        .env("MXR_DATA_DIR", root.join("data"))
        .env("MXR_DAEMON_AUTOSTART", "off")
        .env("MXR_ACTIVITY", "off")
        .env_remove("MXR_DAEMON_ADDR");
    cmd
}

#[test]
fn no_sanitize_warns_on_stderr_and_keeps_stdout_clean() {
    let temp = TempDir::new().expect("temp dir");
    let output = isolated_command(temp.path())
        .args(["bug-report", "--no-sanitize", "--stdout"])
        .output()
        .expect("run mxr bug-report");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stderr.contains(WARNING_PREFIX), "stderr was: {stderr}");
    assert!(
        stderr.contains("Read it before you share it."),
        "stderr was: {stderr}"
    );
    assert!(!stdout.contains(WARNING_PREFIX), "warning leaked to stdout");
    assert!(
        stdout.contains("# "),
        "report missing from stdout: {stdout}"
    );
}

#[test]
fn sanitized_default_run_prints_no_warning() {
    let temp = TempDir::new().expect("temp dir");
    let output = isolated_command(temp.path())
        .args(["bug-report", "--stdout"])
        .output()
        .expect("run mxr bug-report");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(WARNING_PREFIX), "stderr was: {stderr}");
}
