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
        ("HOME".into(), local.join("home").display().to_string()),
        (
            "XDG_CONFIG_HOME".into(),
            local.join("xdg-config").display().to_string(),
        ),
        (
            "XDG_DATA_HOME".into(),
            local.join("xdg-data").display().to_string(),
        ),
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
                while let Some(Ok(message)) = framed.next().await {
                    counter.fetch_add(1, Ordering::Relaxed);
                    let data = match message.payload {
                        IpcPayload::Request(Request::GetStatus) => ResponseData::Status {
                            workspace_owner_supported: false,
                            draft_revision_supported: true,
                            send_outcome_supported: true,
                            archive_recovery_supported: true,
                            attachment_transfer_supported: true,
                            draft_attachment_upload_supported: true,
                            recovery_supported: false,
                            recovery: None,
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
                        IpcPayload::Request(Request::RemoteAuthority { request }) => {
                            let response = match request {
                                mxr_protocol::RemoteRequest::ApprovePairing {
                                    request_id,
                                    dry_run,
                                } => serde_json::from_value(serde_json::json!({
                                    "result": "Pairing",
                                    "pairing": {
                                        "capabilities": [],
                                        "request_id": request_id,
                                        "name": "fixture",
                                        "challenge": "fixture-challenge",
                                        "expires_at": 0,
                                        "approved": false,
                                        "failed_attempts": 0
                                    },
                                    "dry_run": dry_run
                                }))
                                .unwrap(),
                                mxr_protocol::RemoteRequest::RevokeClient {
                                    client_id,
                                    dry_run,
                                } => serde_json::from_value(serde_json::json!({
                                    "result": "Client",
                                    "client": {
                                        "capabilities": [],
                                        "client_id": client_id,
                                        "name": "fixture",
                                        "created_at": 0,
                                        "expires_at": 0,
                                        "revoked_at": null
                                    },
                                    "dry_run": dry_run
                                }))
                                .unwrap(),
                                other => panic!("unexpected remote authority request: {other:?}"),
                            };
                            ResponseData::RemoteAuthority { response }
                        }
                        other => panic!("unexpected request in fixture: {other:?}"),
                    };
                    let response = IpcMessage {
                        expected_workspace: None,
                        id: message.id,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(Response::Ok { data }),
                    };
                    if framed.send(response).await.is_err() {
                        break;
                    }
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
    run_mxr_with_optional_override(envs, args, Some(daemon_addr)).await
}

async fn run_mxr_without_daemon_override(
    envs: &[(String, String)],
    args: &[&str],
) -> std::process::Output {
    run_mxr_with_optional_override(envs, args, None).await
}

async fn run_mxr_with_optional_override(
    envs: &[(String, String)],
    args: &[&str],
    daemon_addr: Option<&str>,
) -> std::process::Output {
    let mut command = tokio::process::Command::new(MXR_BIN);
    command
        .args(args)
        .envs(envs.iter().map(|(key, value)| (key, value)));
    if let Some(address) = daemon_addr {
        command.env("MXR_DAEMON_ADDR", address);
    } else {
        command
            .env_remove("MXR_DAEMON_ADDR")
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("all_proxy");
    }
    let child = command
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

fn connection_state_path(envs: &[(String, String)]) -> PathBuf {
    PathBuf::from(&envs[1].1).join("client-connection.json")
}

fn saved_registration() -> mxr_config::connection_state::ConnectionRegistration {
    mxr_config::connection_state::ConnectionRegistration {
        capabilities: vec![mxr_core::RemoteCapability::MailRead],
        origin: "https://saved.example".into(),
        client_id: "fixture-client".into(),
        host_id: "fixture-host".into(),
        workspace_id: "fixture-workspace".into(),
        recovery_generation: "1".into(),
        mail_secret: mxr_core::RemoteSecret::new("a".repeat(64)),
        cleanup_secret: mxr_core::RemoteSecret::new("b".repeat(64)),
    }
}

fn assert_no_pairing_connection(listener: &std::net::TcpListener) {
    assert!(matches!(
        listener.accept(),
        Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock
    ));
}

#[tokio::test]
async fn connect_validates_names_before_preview_or_pairing_state() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("https://{address}");
    let state_path = connection_state_path(&envs);

    let invalid_names = [
        String::new(),
        "   ".into(),
        "invalid\nname".into(),
        "é".repeat(41),
    ];
    let status =
        run_mxr_without_daemon_override(&envs, &["connect", "--status", "--name", ""]).await;
    assert!(status.status.success());
    let local_preview =
        run_mxr_without_daemon_override(&envs, &["connect", "--local", "--dry-run", "--name", ""])
            .await;
    assert!(local_preview.status.success());

    for name in &invalid_names {
        let output = run_mxr_without_daemon_override(
            &envs,
            &["connect", &origin, "--name", name, "--confirm"],
        )
        .await;
        assert!(
            !output.status.success(),
            "invalid name was accepted: {name:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Client name must be non-empty"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!state_path.exists(), "invalid name persisted client state");
        assert_no_pairing_connection(&listener);
    }

    let output =
        run_mxr_without_daemon_override(&envs, &["connect", &origin, "--name", "   ", "--dry-run"])
            .await;
    assert!(
        !output.status.success(),
        "invalid name was accepted in preview"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Client name must be non-empty"));
    assert!(!state_path.exists());

    let bounded_name = "é".repeat(40);
    let output = run_mxr_without_daemon_override(
        &envs,
        &["connect", &origin, "--name", &bounded_name, "--dry-run"],
    )
    .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["selection_applied"], false);
    assert!(!state_path.exists());
    assert_no_pairing_connection(&listener);
}

#[tokio::test]
async fn connect_status_redacts_command_override_without_executing_it() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    let local = run_mxr_without_daemon_override(&envs, &["connect", "--status"]).await;
    assert!(local.status.success());
    let local: serde_json::Value = serde_json::from_slice(&local.stdout).unwrap();
    assert_eq!(local["effective_target"], "local");
    assert_eq!(local["selection_source"], "local");

    let state_path = connection_state_path(&envs);
    mxr_config::connection_state::update(&state_path, Some(0), |state| {
        state.generation = 1;
        state.selected = Some(saved_registration());
        Ok(())
    })
    .unwrap();
    let saved = run_mxr_without_daemon_override(&envs, &["connect", "--status"]).await;
    assert!(saved.status.success());
    let saved: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap();
    assert_eq!(saved["effective_target"], "https://saved.example");
    assert_eq!(saved["selection_source"], "saved");

    let marker = root.path().join("command-ran");
    let synthetic_argument = root.path().join("synthetic-token-that-must-not-appear");
    let address = format!(
        "cmd://touch {} {}",
        marker.display(),
        synthetic_argument.display()
    );
    let status = run_mxr(&envs, &["connect", "--status"], &address).await;
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["effective_target"], "cmd://<command hidden>");
    assert_eq!(status["selection_source"], "environment");
    assert!(!status
        .to_string()
        .contains(&synthetic_argument.display().to_string()));
    assert!(!status.to_string().contains(&marker.display().to_string()));
    assert!(!marker.exists(), "status executed the command override");
    assert!(!synthetic_argument.exists());
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
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while requests.load(Ordering::Relaxed) - before_watch < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("status watch did not poll the selected daemon twice");
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
async fn serve_root_dry_run_cannot_confirm_pairing_and_leaf_previews_work() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("authority.sock");
    let (_server, requests) = spawn_fake_daemon(&socket);
    let mut envs = isolated_env(root.path());
    envs[3].1 = socket.display().to_string();

    let invalid = run_mxr(
        &envs,
        &[
            "serve",
            "--dry-run",
            "pair",
            "approve",
            "request-unsafe",
            "--confirm",
        ],
        "unused",
    )
    .await;
    assert!(!invalid.status.success());
    let error = String::from_utf8(invalid.stderr).unwrap();
    assert!(
        error.contains("put `--dry-run` after the serve action"),
        "{error}"
    );
    assert_eq!(
        requests.load(Ordering::Relaxed),
        0,
        "invalid invocation reached IPC"
    );

    let foreground_preview = run_mxr(
        &envs,
        &[
            "serve",
            "--foreground",
            "--dry-run",
            "--listen",
            "127.0.0.1:0",
            "--format",
            "json",
        ],
        "unused",
    )
    .await;
    assert!(foreground_preview.status.success());
    let output: serde_json::Value = serde_json::from_slice(&foreground_preview.stdout).unwrap();
    assert_eq!(output["foreground"], true);
    assert_eq!(output["dry_run"], true);
    assert_eq!(
        requests.load(Ordering::Relaxed),
        0,
        "foreground preview reached IPC"
    );

    for args in [
        vec![
            "serve",
            "--format",
            "json",
            "pair",
            "approve",
            "request-preview",
            "--dry-run",
        ],
        vec!["serve", "clients", "revoke", "client-preview", "--dry-run"],
    ] {
        let preview = run_mxr(&envs, &args, "unused").await;
        assert!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        let output: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
        assert_eq!(output["response"]["dry_run"], true, "{output}");
    }
    assert_eq!(requests.load(Ordering::Relaxed), 4);
}

#[tokio::test]
async fn unreachable_target_and_remote_reindex_never_touch_client_store() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    std::fs::create_dir_all(Path::new(&envs[3].1).parent().unwrap()).unwrap();
    let (_healthy_local_daemon, local_requests) = spawn_fake_daemon(Path::new(&envs[3].1));
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
    assert_eq!(local_requests.load(Ordering::Relaxed), 0);

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
    assert_eq!(local_requests.load(Ordering::Relaxed), 0);

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

#[tokio::test]
async fn remote_account_setup_fails_before_prompts_or_client_profile_writes() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    let output = run_mxr(
        &envs,
        &["accounts", "add", "imap"],
        "cmd://missing-fixture-bridge",
    )
    .await;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("unsupported through a selected remote daemon"),
        "{error}"
    );
    assert!(!root.path().join("client/config/config.toml").exists());
    assert!(!root.path().join("client/data/mxr.db").exists());
}

#[tokio::test]
async fn recovery_unreachable_cmd_target_never_falls_back_to_local_daemon() {
    let root = tempfile::tempdir().unwrap();
    let mut envs = isolated_env(root.path());
    envs.push(("MXR_DAEMON_AUTOSTART".into(), "on".into()));
    std::fs::create_dir_all(Path::new(&envs[3].1).parent().unwrap()).unwrap();
    let (server, requests) = spawn_fake_daemon(Path::new(&envs[3].1));
    let output = run_mxr(
        &envs,
        &["recovery", "status", "--format", "json"],
        "cmd://mxr-nonexistent-recovery-bridge",
    )
    .await;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cmd:// daemon bridge"));
    assert_eq!(requests.load(Ordering::Relaxed), 0);
    assert!(!Path::new(&envs[2].1).join("mxr.db").exists());
    server.abort();
}

#[tokio::test]
async fn serve_old_local_daemon_reports_upgrade_before_new_request_or_remote_override() {
    let root = tempfile::tempdir().unwrap();
    let envs = isolated_env(root.path());
    let socket = Path::new(&envs[3].1);
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut frames = Framed::new(stream, IpcCodec::new());
        let request = frames.next().await.unwrap().unwrap();
        assert!(matches!(
            request.payload,
            IpcPayload::Request(Request::GetStatus)
        ));
        let data:ResponseData=serde_json::from_value(serde_json::json!({"kind":"Status","uptime_secs":0,"accounts":[],"total_messages":0,"protocol_version":12})).unwrap();
        frames
            .send(IpcMessage {
                id: request.id,
                expected_workspace: None,
                source: ClientKind::Daemon,
                payload: IpcPayload::Response(Response::Ok { data }),
            })
            .await
            .unwrap();
        // An old host must never receive a new authority variant or a shutdown/restart.
        assert!(frames.next().await.is_none());
    });
    let output = run_mxr(
        &envs,
        &["serve", "status", "--format", "json"],
        "cmd://missing-remote-fixture",
    )
    .await;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains(&format!(
            "requires IPC protocol {}",
            mxr_protocol::IPC_PROTOCOL_VERSION
        )),
        "{error}"
    );
    assert!(error.contains("local daemon uses 12"), "{error}");
    server.await.unwrap();
    assert!(!Path::new(&envs[2].1).join("mxr.db").exists());
}
