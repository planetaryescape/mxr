#![expect(
    clippy::unwrap_used,
    reason = "integration test asserts directly on isolated fixtures"
)]
#![expect(
    clippy::panic,
    reason = "an unexpected fixture request means the test harness is invalid"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use mxr_protocol::{
    ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    IPC_PROTOCOL_VERSION,
};
use tokio::net::UnixListener;
use tokio_util::codec::Framed;

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");

fn isolated_env(root: &Path) -> Vec<(String, String)> {
    let local = root.join("client");
    vec![
        ("MXR_INSTANCE".into(), "mxr-selected-daemon-it".into()),
        (
            "MXR_CONFIG_DIR".into(),
            local.join("config").display().to_string(),
        ),
        (
            "MXR_DATA_DIR".into(),
            local.join("data").display().to_string(),
        ),
        (
            "MXR_SOCKET_PATH".into(),
            local.join("mxr.sock").display().to_string(),
        ),
        ("MXR_ACTIVITY".into(), "off".into()),
    ]
}

fn spawn_fake_daemon(path: &Path) -> (tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = UnixListener::bind(path).unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    let server = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let counter = counter.clone();
            tokio::spawn(async move {
                let mut framed = Framed::new(stream, IpcCodec::new());
                if let Some(Ok(message)) = framed.next().await {
                    counter.fetch_add(1, Ordering::Relaxed);
                    let data = match message.payload {
                        IpcPayload::Request(Request::GetStatus) => ResponseData::Status {
                            uptime_secs: 73,
                            accounts: vec!["target-account".into()],
                            total_messages: 19,
                            daemon_pid: Some(9876),
                            sync_statuses: vec![],
                            protocol_version: IPC_PROTOCOL_VERSION,
                            daemon_version: Some("target-version".into()),
                            daemon_build_id: Some("target-build".into()),
                            repair_required: false,
                            semantic_runtime: None,
                            feature_health: None,
                            degraded: false,
                        },
                        IpcPayload::Request(Request::GetDoctorReport) => {
                            let report = serde_json::from_value(serde_json::json!({
                                "healthy": true,
                                "data_dir_exists": true,
                                "database_exists": true,
                                "index_exists": true,
                                "socket_exists": true,
                                "socket_reachable": true,
                                "stale_socket": false,
                                "daemon_running": true,
                                "daemon_pid": 9876,
                                "index_lock_held": false,
                                "index_lock_error": null,
                                "database_path": "/target-profile/data/mxr.db",
                                "database_size_bytes": 4321,
                                "index_path": "/target-profile/data/search_index",
                                "index_size_bytes": 1234,
                                "log_path": "/target-profile/data/logs/mxr.log",
                                "log_size_bytes": 22,
                                "sync_statuses": [],
                                "recent_sync_events": [],
                                "recent_error_logs": [],
                                "recommended_next_steps": [
                                    "mxr doctor --reindex",
                                    "mxr daemon --foreground"
                                ]
                            }))
                            .unwrap();
                            ResponseData::DoctorReport { report }
                        }
                        other => panic!("unexpected request in fixture: {other:?}"),
                    };
                    let response = IpcMessage {
                        id: message.id,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(Response::Ok { data }),
                    };
                    let _ = framed.send(response).await;
                }
            });
        }
    });
    (server, requests)
}

async fn run_mxr(
    envs: &[(String, String)],
    args: &[&str],
    daemon_addr: &str,
) -> std::process::Output {
    let child = tokio::process::Command::new(MXR_BIN)
        .args(args)
        .envs(envs.iter().map(|(key, value)| (key, value)))
        .env("MXR_DAEMON_ADDR", daemon_addr)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
        .await
        .expect("mxr subprocess exceeded the 10-second smoke-test bound")
        .unwrap()
}

#[tokio::test]
async fn cmd_target_diagnostics_report_target_and_separate_local_profile() {
    let root = tempfile::tempdir().unwrap();
    let target_socket = root.path().join("selected.sock");
    let (_server, requests) = spawn_fake_daemon(&target_socket);
    let envs = isolated_env(root.path());
    let addr = format!(
        "cmd://env MXR_SOCKET_PATH={} {} daemon dial-stdio",
        target_socket.display(),
        MXR_BIN
    );

    let status = run_mxr(&envs, &["status", "--format", "json"], &addr).await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["daemon_target"]["kind"], "cmd");
    assert_eq!(status["daemon_target"]["display"], "cmd://<command hidden>");
    assert_eq!(status["total_messages"], 19);
    assert_eq!(status["restart_required"], false);
    assert_eq!(status["client_local"]["data_dir"], envs[2].1);
    assert!(status.get("data_dir").is_none());
    assert!(!status
        .to_string()
        .contains(&target_socket.display().to_string()));

    let status_line = run_mxr(&envs, &["status", "--format", "jsonl"], &addr).await;
    assert!(
        status_line.status.success(),
        "{}",
        String::from_utf8_lossy(&status_line.stderr)
    );
    let status_line = String::from_utf8(status_line.stdout).unwrap();
    assert_eq!(status_line.lines().count(), 1);
    let status_line: serde_json::Value = serde_json::from_str(status_line.trim()).unwrap();
    assert_eq!(status_line["daemon_target"]["kind"], "cmd");

    let doctor = run_mxr(&envs, &["doctor", "--check", "--format", "jsonl"], &addr).await;
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let text = String::from_utf8(doctor.stdout).unwrap();
    assert_eq!(text.lines().count(), 1);
    let doctor: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(doctor["daemon_target"]["kind"], "cmd");
    assert_eq!(
        doctor["daemon"]["database_path"],
        "/target-profile/data/mxr.db"
    );
    assert_eq!(doctor["client_local"]["data_dir"], envs[2].1);
    assert!(!text.contains(&target_socket.display().to_string()));

    let stats = run_mxr(
        &envs,
        &["doctor", "--store-stats", "--format", "json"],
        &addr,
    )
    .await;
    assert!(
        stats.status.success(),
        "{}",
        String::from_utf8_lossy(&stats.stderr)
    );
    let stats: serde_json::Value = serde_json::from_slice(&stats.stdout).unwrap();
    assert_eq!(
        stats["daemon"]["database_path"],
        "/target-profile/data/mxr.db"
    );

    let table = run_mxr(&envs, &["doctor", "--format", "table"], &addr).await;
    assert!(
        table.status.success(),
        "{}",
        String::from_utf8_lossy(&table.stderr)
    );
    let table = String::from_utf8(table.stdout).unwrap();
    assert!(
        table.contains(
            "Remediation and next commands below must be run on the selected daemon host."
        ),
        "{table}"
    );
    assert!(table.contains("mxr doctor --reindex"), "{table}");
    assert!(table.contains("mxr daemon --foreground"), "{table}");

    let before_watch = requests.load(Ordering::Relaxed);
    let mut watch = tokio::process::Command::new(MXR_BIN)
        .args(["status", "--watch", "--format", "jsonl"])
        .envs(envs.iter().map(|(key, value)| (key, value)))
        .env("MXR_DAEMON_ADDR", &addr)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(2300)).await;
    watch.kill().await.unwrap();
    watch.wait().await.unwrap();
    assert!(requests.load(Ordering::Relaxed) - before_watch >= 2);

    // Explicit Unix targets also keep their socket ownership separate from
    // the client's profile paths in remote diagnostics.
    let unix_target = root.path().join("unix-target.sock");
    let (_unix_server, _) = spawn_fake_daemon(&unix_target);
    let unix_addr = format!("unix://{}", unix_target.display());
    let status = run_mxr(&envs, &["status", "--format", "json"], &unix_addr).await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["daemon_target"]["kind"], "unix");
    assert_eq!(status["client_local"]["socket_path"], envs[3].1);
    assert_ne!(
        status["client_local"]["socket_path"],
        unix_target.display().to_string()
    );
}

#[tokio::test]
async fn unreachable_target_and_remote_reindex_never_touch_client_store() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    std::fs::create_dir_all(Path::new(&envs[3].1).parent().unwrap()).unwrap();
    let (_healthy_local_daemon, _) = spawn_fake_daemon(Path::new(&envs[3].1));
    let local_data = PathBuf::from(&envs[2].1);
    std::fs::create_dir_all(local_data.join("search_index")).unwrap();
    std::fs::write(local_data.join("search_index/keep"), b"client index").unwrap();
    let marker = root.path().join("spawned");
    let addr = format!("cmd://touch {}", marker.display());

    let reindex = run_mxr(&envs, &["doctor", "--reindex", "--format", "json"], &addr).await;
    assert!(!reindex.status.success());
    assert!(
        !marker.exists(),
        "remote repair rejection must happen before connecting"
    );
    assert_eq!(
        std::fs::read(local_data.join("search_index/keep")).unwrap(),
        b"client index"
    );

    let unreachable = format!("unix://{}", root.path().join("missing.sock").display());
    let doctor = run_mxr(
        &envs,
        &["doctor", "--check", "--format", "json"],
        &unreachable,
    )
    .await;
    assert!(!doctor.status.success());
    assert!(
        String::from_utf8_lossy(&doctor.stderr).contains("connect"),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert_eq!(
        std::fs::read(local_data.join("search_index/keep")).unwrap(),
        b"client index"
    );
    assert!(!local_data.join("mxr.db").exists());

    let missing_status_target = root.path().join("missing-status.sock");
    let status = run_mxr(
        &envs,
        &["status", "--format", "json"],
        &format!("unix://{}", missing_status_target.display()),
    )
    .await;
    assert!(!status.status.success());
    assert!(
        String::from_utf8_lossy(&status.stderr).contains("connect"),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        !missing_status_target.exists(),
        "status must not autostart the explicitly selected daemon"
    );
    assert_eq!(
        std::fs::read(local_data.join("search_index/keep")).unwrap(),
        b"client index"
    );
    assert!(!local_data.join("mxr.db").exists());

    let private_command = "mxr-nonexistent-s01-probe --token=synthetic-s01-secret";
    let private_addr = format!("cmd://{private_command}");
    for args in [
        vec!["status", "--format", "json"],
        vec!["doctor", "--check", "--format", "json"],
    ] {
        let output = run_mxr(&envs, &args, &private_addr).await;
        assert!(!output.status.success());
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains("cmd:// daemon bridge"), "{text}");
        assert!(!text.contains(private_command), "{text}");
        assert!(!text.contains("synthetic-s01-secret"), "{text}");
    }

    let missing_target = root.path().join("missing-maintenance.sock");
    let maintenance = run_mxr(
        &envs,
        &["doctor", "--rebuild-analytics"],
        &format!("unix://{}", missing_target.display()),
    )
    .await;
    assert!(!maintenance.status.success());
    assert!(
        !missing_target.exists(),
        "remote maintenance must not autostart a daemon"
    );
    assert!(!local_data.join("mxr.db").exists());
}

#[tokio::test]
async fn local_default_offline_doctor_keeps_local_store_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    let output = Command::new(MXR_BIN)
        .args(["doctor", "--store-stats", "--format", "json"])
        .envs(envs.iter().map(|(key, value)| (key, value)))
        .env_remove("MXR_DAEMON_ADDR")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stats: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        stats["database_path"],
        PathBuf::from(&envs[2].1)
            .join("mxr.db")
            .display()
            .to_string()
    );
    assert!(stats.get("daemon_target").is_none());
}
