use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

const IMAP_ACCOUNT_CONFIG: &str = r#"
[accounts.consulting]
name = "Consulting"
email = "consulting@example.com"

[accounts.consulting.sync]
type = "imap"
host = "imap.example.com"
port = 993
username = "consulting@example.com"
password_ref = "mxr/consulting-imap"
use_tls = true
"#;

fn unique_instance_name(prefix: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time")
        .as_nanos();
    format!("{prefix}-{}-{stamp}", std::process::id())
}

/// Base `mxr` invocation wired to an isolated instance with NO daemon
/// addressing that could leak in from the caller's environment.
///
/// `MXR_DAEMON_ADDR` would override `MXR_SOCKET_PATH` and could point repair at
/// a real unix / tcp / cmd daemon (cmd:// even spawns one), so it is removed
/// explicitly. `MXR_KEYCHAIN=off` keeps the persist path from touching the real
/// OS keychain / Secret Service (disk stays authoritative regardless).
fn isolated_mxr(instance: &str, data_dir: &Path, config_dir: &Path, socket_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mxr").expect("mxr binary");
    cmd.env_remove("MXR_DAEMON_ADDR")
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
        .env("MXR_SOCKET_PATH", socket_path)
        .env("MXR_INSTANCE", instance)
        .env("MXR_KEYCHAIN", "off");
    cmd
}

/// Combined-intent proof: PR #155 (repair runs with no daemon) composed with
/// 0.6.11 (credentials persist disk-first to a 0600 `secrets.toml`). With no
/// daemon reachable, repair takes the in-process path and lands the credential
/// on disk itself.
#[test]
fn repair_persists_credential_in_process_when_no_daemon() {
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name("accounts-repair-nodaemon");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = temp.path().join("runtime").join("mxr.sock");
    let secrets_path = config_dir.join("secrets.toml");

    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(config_dir.join("config.toml"), IMAP_ACCOUNT_CONFIG).expect("write config");

    isolated_mxr(&instance, &data_dir, &config_dir, &socket_path)
        .env("MXR_SECRETS_PATH", &secrets_path)
        .args(["accounts", "repair", "consulting"])
        .write_stdin("s3cret-app-pw\n")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Repaired credentials for 'consulting'.")
                .and(predicate::str::contains("secrets.toml, mode 0600")),
        )
        .stderr(predicate::str::contains("Starting daemon").not());

    assert!(
        !socket_path.exists(),
        "repair must not bind a daemon socket"
    );
    assert!(
        !data_dir.join("daemon.pid").exists(),
        "repair must not write a daemon pid file"
    );

    assert!(
        secrets_path.exists(),
        "repair must write the disk-first secrets store"
    );
    let contents = std::fs::read_to_string(&secrets_path).expect("read secrets.toml");
    assert!(
        contents.contains("mxr/consulting-imap"),
        "secrets.toml must record the credential service, got:\n{contents}"
    );
    assert!(
        contents.contains("consulting@example.com"),
        "secrets.toml must record the credential account, got:\n{contents}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&secrets_path)
            .expect("secrets.toml metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "secrets.toml must be 0600");
    }
}

/// Kills a spawned test daemon by pid on drop so a panicking assertion never
/// leaks a resident daemon.
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
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            let _ = StdCommand::new("kill").arg(pid.to_string()).status();
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

/// Regression guard for Codex defect #1: the daemon-free repair path must not
/// break when a daemon IS already running. Repair must reuse the running daemon
/// (not autostart a second one) and the credential must still be persisted.
#[test]
fn repair_reuses_running_daemon_and_still_persists() {
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name("accounts-repair-daemon");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = temp.path().join("runtime").join("mxr.sock");
    let secrets_path = config_dir.join("secrets.toml");

    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(socket_path.parent().expect("runtime dir")).expect("runtime dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(config_dir.join("config.toml"), IMAP_ACCOUNT_CONFIG).expect("write config");

    let mut daemon = TestDaemon::new(socket_path.clone());

    let status = isolated_mxr(&instance, &data_dir, &config_dir, &socket_path)
        .args(["status", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let status_stderr = String::from_utf8(status.stderr).expect("utf8 stderr");
    assert!(
        status_stderr.contains("Starting daemon"),
        "status should autostart the daemon, stderr={status_stderr:?}"
    );
    let status_json: Value =
        serde_json::from_slice(&status.stdout).expect("status json should parse");
    let pid = status_json["daemon_pid"]
        .as_u64()
        .expect("daemon pid should be present");
    daemon.pid = Some(pid);

    isolated_mxr(&instance, &data_dir, &config_dir, &socket_path)
        .env("MXR_SECRETS_PATH", &secrets_path)
        .args(["accounts", "repair", "consulting"])
        .write_stdin("s3cret-app-pw\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Repaired credentials for 'consulting'.",
        ))
        .stderr(predicate::str::contains("Starting daemon").not());

    assert!(
        secrets_path.exists(),
        "repair must persist the credential to the disk-first store"
    );
    let contents = std::fs::read_to_string(&secrets_path).expect("read secrets.toml");
    assert!(
        contents.contains("mxr/consulting-imap") && contents.contains("consulting@example.com"),
        "secrets.toml must record the repaired credential, got:\n{contents}"
    );

    let status2 = isolated_mxr(&instance, &data_dir, &config_dir, &socket_path)
        .args(["status", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let status2_json: Value =
        serde_json::from_slice(&status2.stdout).expect("status json should parse");
    assert_eq!(
        status2_json["daemon_pid"].as_u64(),
        Some(pid),
        "repair must not restart or duplicate the running daemon"
    );
}
