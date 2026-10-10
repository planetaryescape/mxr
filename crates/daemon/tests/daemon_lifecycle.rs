use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

static DAEMON_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

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

    fn clear_pid(&mut self) {
        self.pid = None;
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
    let _guard = daemon_lifecycle_guard();
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

/// Locks in the property that the demo daemon (`mxr demo`) cannot poison the
/// real daemon's state. The real demo flow sets `MXR_INSTANCE=mxr-demo` plus
/// `MXR_DATA_DIR` / `MXR_CONFIG_DIR` pointing at demo-only paths, and the
/// daemon resolves every on-disk resource (socket, SQLite store, Tantivy
/// index, semantic vectors, PID file, bridge port/token) through those env
/// vars. This test spawns two daemons concurrently under different
/// instance/data/config triples and asserts their state is fully disjoint.
#[test]
fn demo_instance_and_real_instance_coexist_with_separate_state() {
    let _guard = daemon_lifecycle_guard();

    let temp_real = TempDir::new().expect("real temp dir");
    let temp_demo = TempDir::new().expect("demo temp dir");

    let real_instance = unique_instance_name("mxr-test-real");
    let demo_instance = unique_instance_name("mxr-test-demo");
    assert_ne!(
        real_instance, demo_instance,
        "test setup must use distinct instance names"
    );

    let real_data_dir = temp_real.path().join("data");
    let real_config_dir = temp_real.path().join("config");
    let demo_data_dir = temp_demo.path().join("data");
    let demo_config_dir = temp_demo.path().join("config");
    std::fs::create_dir_all(&real_data_dir).expect("real data dir");
    std::fs::create_dir_all(&real_config_dir).expect("real config dir");
    std::fs::create_dir_all(&demo_data_dir).expect("demo data dir");
    std::fs::create_dir_all(&demo_config_dir).expect("demo config dir");

    let real_socket = instance_socket_path(&real_instance);
    let demo_socket = instance_socket_path(&demo_instance);
    assert_ne!(
        real_socket, demo_socket,
        "MXR_INSTANCE must namespace the IPC socket path"
    );

    let real_pid_path = real_data_dir.join("daemon.pid");
    let demo_pid_path = demo_data_dir.join("daemon.pid");

    let mut real_daemon = TestDaemon::new(real_socket.clone(), real_pid_path.clone());
    let mut demo_daemon = TestDaemon::new(demo_socket.clone(), demo_pid_path.clone());

    // Start the "real" daemon first.
    let (real_stdout, real_stderr) = run_status(&real_instance, &real_data_dir, &real_config_dir);
    let real_status = parse_status(&real_stdout);
    let real_daemon_pid = real_status["daemon_pid"]
        .as_u64()
        .expect("real daemon pid present");
    real_daemon.set_pid(real_daemon_pid);
    assert!(
        real_stderr.contains("Starting daemon... ready."),
        "expected real daemon to autostart, stderr={real_stderr:?}"
    );

    // Start the "demo" daemon while the real one is still running.
    let (demo_stdout, demo_stderr) = run_status(&demo_instance, &demo_data_dir, &demo_config_dir);
    let demo_status = parse_status(&demo_stdout);
    let demo_daemon_pid = demo_status["daemon_pid"]
        .as_u64()
        .expect("demo daemon pid present");
    demo_daemon.set_pid(demo_daemon_pid);
    assert!(
        demo_stderr.contains("Starting daemon... ready."),
        "expected demo daemon to autostart, stderr={demo_stderr:?}"
    );

    // Distinct processes.
    assert_ne!(
        real_daemon_pid, demo_daemon_pid,
        "real and demo daemons must run as distinct processes"
    );

    // Data directories must be disjoint — neither nested under the other.
    assert!(
        !real_data_dir.starts_with(&demo_data_dir),
        "real data dir must not live under demo data dir"
    );
    assert!(
        !demo_data_dir.starts_with(&real_data_dir),
        "demo data dir must not live under real data dir"
    );

    // Each daemon owns a PID file inside its own data dir.
    assert!(
        real_pid_path.exists(),
        "real daemon must write its pid file at {}",
        real_pid_path.display()
    );
    assert!(
        demo_pid_path.exists(),
        "demo daemon must write its pid file at {}",
        demo_pid_path.display()
    );

    // The Tantivy search index lives at `<data_dir>/search_index` (see
    // `crates/daemon/src/state.rs`). Each daemon must materialize its own
    // index dir, at distinct paths, so seeding the demo can never write into
    // the real index.
    let real_index = real_data_dir.join("search_index");
    let demo_index = demo_data_dir.join("search_index");
    assert_ne!(
        real_index, demo_index,
        "real and demo Tantivy indexes must live at different paths"
    );
    assert!(
        real_index.is_dir(),
        "real daemon must create its search_index dir at {}",
        real_index.display()
    );
    assert!(
        demo_index.is_dir(),
        "demo daemon must create its search_index dir at {}",
        demo_index.display()
    );

    assert_ne!(
        real_pid_path, demo_pid_path,
        "real and demo daemons must own distinct pid files"
    );

    // Both daemons remain independently responsive after the other started.
    let (real_stdout2, real_stderr2) = run_status(&real_instance, &real_data_dir, &real_config_dir);
    let real_status2 = parse_status(&real_stdout2);
    let real_pid2 = real_status2["daemon_pid"]
        .as_u64()
        .expect("real daemon still up");
    assert_eq!(
        real_pid2, real_daemon_pid,
        "real daemon pid changed after demo daemon came up: real should not have been restarted"
    );
    assert!(
        real_stderr2.trim().is_empty(),
        "second real status should reuse the running real daemon, stderr={real_stderr2:?}"
    );

    let (demo_stdout2, demo_stderr2) = run_status(&demo_instance, &demo_data_dir, &demo_config_dir);
    let demo_status2 = parse_status(&demo_stdout2);
    let demo_pid2 = demo_status2["daemon_pid"]
        .as_u64()
        .expect("demo daemon still up");
    assert_eq!(
        demo_pid2, demo_daemon_pid,
        "demo daemon pid changed unexpectedly"
    );
    assert!(
        demo_stderr2.trim().is_empty(),
        "second demo status should reuse the running demo daemon, stderr={demo_stderr2:?}"
    );
}

#[test]
fn status_recovers_running_daemon_when_socket_path_disappears() {
    let _guard = daemon_lifecycle_guard();
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

#[cfg(unix)]
#[tokio::test]
// Keep the shared daemon fixture serialized while the async client is active.
#[allow(clippy::await_holding_lock)]
async fn foreground_daemon_stops_on_signals_and_preserves_synced_state() {
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{
        ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    };
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use std::process::{Child, Stdio};

    let _guard = daemon_lifecycle_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name("mxr-test-supervised-stop");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let home_dir = temp.path().join("home");
    let xdg_config_home = home_dir.join(".config");
    let socket_path = temp.path().join("mxr.sock");
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::create_dir_all(&xdg_config_home).expect("fixture XDG config dir");
    mxr_test_support::daemon::write_fake_account_config(&config_dir);
    let mut daemon = TestDaemon::new(socket_path.clone(), pid_path.clone());
    let inherited_env = std::env::vars_os()
        .filter(|(key, _)| !key.to_string_lossy().starts_with("MXR_"))
        .collect::<Vec<_>>();

    let spawn = || {
        let mut command = StdCommand::new(env!("CARGO_BIN_EXE_mxr"));
        command
            .env_clear()
            .envs(inherited_env.clone())
            .env("HOME", &home_dir)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .args(["daemon", "--foreground", "--no-bridge"])
            .env("MXR_INSTANCE", &instance)
            .env("MXR_DATA_DIR", &data_dir)
            .env("MXR_CONFIG_DIR", &config_dir)
            .env("MXR_SOCKET_PATH", &socket_path)
            .env("MXR_ACTIVITY", "off")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn foreground daemon")
    };
    let run_cli = |args: &[&str]| {
        let mut command = StdCommand::new(env!("CARGO_BIN_EXE_mxr"));
        command
            .env_clear()
            .envs(inherited_env.clone())
            .env("HOME", &home_dir)
            .env("XDG_CONFIG_HOME", &xdg_config_home)
            .env("MXR_INSTANCE", &instance)
            .env("MXR_DATA_DIR", &data_dir)
            .env("MXR_CONFIG_DIR", &config_dir)
            .env("MXR_SOCKET_PATH", &socket_path)
            .env("MXR_ACTIVITY", "off")
            .args(args)
            .output()
            .expect("run mxr against isolated daemon")
    };
    let wait_ready = |child: &mut Child| {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                child.try_wait().expect("poll daemon").is_none(),
                "foreground daemon exited before readiness"
            );
            if socket_path.exists() && pid_path.exists() {
                let output = run_cli(&["status", "--format", "json"]);
                if output.status.success() {
                    return;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "daemon did not become ready"
            );
            sleep(Duration::from_millis(25));
        }
    };
    let stop_and_wait = |child: &mut Child, signal: Signal| {
        let pid = Pid::from_raw(child.id() as i32);
        kill(pid, signal).expect("send signal to fixture daemon child");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = child.try_wait().expect("poll stopped daemon") {
                assert!(
                    status.success(),
                    "signal shutdown should exit successfully: {status}"
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "daemon shutdown exceeded bound"
            );
            sleep(Duration::from_millis(25));
        }
        assert!(!socket_path.exists(), "shutdown should remove owned socket");
        assert!(!pid_path.exists(), "shutdown should remove owned pid file");
    };

    let mut first = spawn();
    daemon.set_pid(first.id() as u64);
    wait_ready(&mut first);
    let sync = run_cli(&["sync", "--wait", "--wait-timeout-secs", "30"]);
    assert!(
        sync.status.success(),
        "fake sync failed: {}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let before = run_cli(&["search", "deployment", "--format", "json", "--limit", "50"]);
    assert!(before.status.success(), "search before stop should succeed");
    let before: Value = serde_json::from_slice(&before.stdout).expect("search JSON before stop");
    let before_results = before
        .as_array()
        .or_else(|| before.get("results").and_then(Value::as_array))
        .expect("search response contains results");
    assert!(
        !before_results.is_empty(),
        "fake sync should persist matching mail"
    );
    let before_message_ids = before_results
        .iter()
        .map(|result| {
            result["message_id"]
                .as_str()
                .expect("message id")
                .to_owned()
        })
        .collect::<Vec<_>>();

    let mut active_client = tokio_util::codec::Framed::new(
        tokio::net::UnixStream::connect(&socket_path)
            .await
            .expect("connect an active client before shutdown"),
        IpcCodec::new(),
    );
    active_client
        .send(IpcMessage {
            id: 1,
            source: ClientKind::Cli,
            payload: IpcPayload::Request(Request::Ping),
        })
        .await
        .expect("send ping on active client");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), active_client.next())
            .await
            .expect("active client ping timed out"),
        Some(Ok(IpcMessage {
            payload: IpcPayload::Response(Response::Ok {
                data: ResponseData::Pong
            }),
            ..
        }))
    ));
    stop_and_wait(&mut first, Signal::SIGTERM);
    daemon.clear_pid();
    drop(active_client);
    let log = std::fs::read_to_string(data_dir.join("logs").join("mxr.log"))
        .expect("read daemon shutdown log");
    assert!(log.contains("SIGTERM received"), "signal should be logged");
    assert!(
        log.contains("daemon shutdown drain completed"),
        "ordered runtime drain should complete"
    );

    let mut second = spawn();
    daemon.set_pid(second.id() as u64);
    wait_ready(&mut second);
    let after = run_cli(&["search", "deployment", "--format", "json", "--limit", "50"]);
    assert!(
        after.status.success(),
        "search after restart should succeed"
    );
    let after: Value = serde_json::from_slice(&after.stdout).expect("search JSON after restart");
    let after_results = after
        .as_array()
        .or_else(|| after.get("results").and_then(Value::as_array))
        .expect("search response contains results after restart");
    assert_eq!(
        after_results
            .iter()
            .map(|result| result["message_id"]
                .as_str()
                .expect("message id")
                .to_owned())
            .collect::<Vec<_>>(),
        before_message_ids,
        "restart should retain the acknowledged search results"
    );

    stop_and_wait(&mut second, Signal::SIGINT);
    daemon.clear_pid();
}

fn daemon_lifecycle_guard() -> MutexGuard<'static, ()> {
    DAEMON_LIFECYCLE_LOCK.lock().expect("daemon lifecycle lock")
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
        strip_dynamic_loader_warnings(String::from_utf8(output.stderr).expect("utf8 stderr")),
    )
}

fn strip_dynamic_loader_warnings(stderr: String) -> String {
    stderr
        .lines()
        .filter(|line| !line.contains("libpcre2-8.so.0: no version information available"))
        .collect::<Vec<_>>()
        .join("\n")
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
