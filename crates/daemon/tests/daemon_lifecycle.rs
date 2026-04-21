use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

struct TestDaemon {
    socket_path: PathBuf,
    pid_path: PathBuf,
    pid: Option<u64>,
}

impl TestDaemon {
    fn new(socket_path: PathBuf, pid_path: PathBuf) -> Self {
        Self {
            socket_path,
            pid_path,
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
            let _ = StdCommand::new("kill").arg(pid.to_string()).status();
            for _ in 0..20 {
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

#[test]
fn status_autostarted_daemon_stays_resident() {
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name("mxr-test-daemon");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let mut daemon = TestDaemon::new(socket_path.clone(), pid_path);

    let (first_stdout, first_stderr) = run_status(&instance, &data_dir, &config_dir);
    let first = parse_status(&first_stdout);
    let first_pid = first["daemon_pid"]
        .as_u64()
        .expect("first daemon pid should be present");
    daemon.set_pid(first_pid);
    assert!(
        first_stderr.contains("Starting daemon... ready."),
        "expected first status to autostart the daemon, stderr={first_stderr:?}"
    );

    sleep(Duration::from_millis(250));

    let (second_stdout, second_stderr) = run_status(&instance, &data_dir, &config_dir);
    let second = parse_status(&second_stdout);
    let second_pid = second["daemon_pid"]
        .as_u64()
        .expect("second daemon pid should be present");

    assert_eq!(
        second_pid, first_pid,
        "daemon pid changed between consecutive status requests"
    );
    assert!(
        second["uptime_secs"].as_u64().unwrap_or(0) >= first["uptime_secs"].as_u64().unwrap_or(0),
        "daemon uptime did not advance between consecutive status requests"
    );
    assert!(
        second_stderr.trim().is_empty(),
        "second status should reuse the running daemon, stderr={second_stderr:?}"
    );
}

#[test]
fn status_recovers_running_daemon_when_socket_path_disappears() {
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name("mxr-test-daemon-recover");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");

    let mut daemon = TestDaemon::new(socket_path.clone(), pid_path.clone());

    let (first_stdout, _first_stderr) = run_status(&instance, &data_dir, &config_dir);
    let first = parse_status(&first_stdout);
    let first_pid = first["daemon_pid"]
        .as_u64()
        .expect("first daemon pid should be present");
    daemon.set_pid(first_pid);
    assert!(pid_path.exists(), "daemon should write a pid file");

    std::fs::remove_file(&socket_path).expect("remove live socket path");
    std::fs::remove_file(&pid_path).expect("remove daemon pid file");
    assert!(
        !socket_path.exists(),
        "test setup should simulate a missing daemon socket"
    );
    assert!(
        !pid_path.exists(),
        "test setup should simulate a pre-fix daemon with no pid file"
    );

    let (second_stdout, second_stderr) = run_status(&instance, &data_dir, &config_dir);
    let second = parse_status(&second_stdout);
    let second_pid = second["daemon_pid"]
        .as_u64()
        .expect("second daemon pid should be present");
    daemon.set_pid(second_pid);

    assert_ne!(
        second_pid, first_pid,
        "daemon should restart after losing its socket path"
    );
    assert!(
        socket_path.exists(),
        "recovered daemon should restore socket path"
    );
    assert!(
        second_stderr.contains("Restarting daemon to recover from a missing IPC socket... ready."),
        "expected recovery restart message, stderr={second_stderr:?}"
    );
}

fn run_status(instance: &str, data_dir: &Path, config_dir: &Path) -> (String, String) {
    let output = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_INSTANCE", instance)
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
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

fn instance_socket_path(instance: &str) -> PathBuf {
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

fn unique_instance_name(prefix: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time")
        .as_nanos();
    format!("{prefix}-{}-{stamp}", std::process::id())
}
