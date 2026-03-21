use crate::handler::handle_request;
use crate::ipc_client::IpcClient;
use crate::loops;
use crate::state::AppState;
use futures::{FutureExt, SinkExt, StreamExt};
use mxr_protocol::{
    AccountSyncStatus, DaemonHealthClass, IpcCodec, IpcMessage, IpcPayload, Request, Response,
    ResponseData, IPC_PROTOCOL_VERSION,
};
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

const STATUS_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn run_daemon() -> anyhow::Result<()> {
    let sock_path = AppState::socket_path();
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match inspect_socket_state(&sock_path).await {
        SocketState::Reachable => {
            anyhow::bail!(
                "Daemon already running at {}. Use `mxr status` or `mxr logs --level error`, or stop the existing daemon before rerunning `mxr daemon --foreground`.",
                sock_path.display()
            );
        }
        SocketState::Stale => {
            let _ = std::fs::remove_file(&sock_path);
        }
        SocketState::Missing => {}
    }

    let state = Arc::new(match AppState::new().await {
        Ok(state) => state,
        Err(error) if is_index_lock_error(&error.to_string()) => {
            anyhow::bail!(
                "Search index is locked by another process. Try `mxr status`, `mxr logs --level error`, or `mxr daemon --foreground`.\nOriginal error: {error}"
            );
        }
        Err(error) => return Err(error),
    });

    let listener = UnixListener::bind(&sock_path)?;
    tracing::info!("Daemon listening on {}", sock_path.display());

    // Reindex search if empty but DB has messages
    {
        let search = state.search.lock().await;
        let test_results = search.search("*", 1);
        let index_empty = test_results.map(|r| r.is_empty()).unwrap_or(true);
        drop(search);

        if index_empty {
            let accounts = state.store.list_accounts().await.unwrap_or_default();
            for account in &accounts {
                let envelopes = state
                    .store
                    .list_envelopes_by_account(&account.id, 10000, 0)
                    .await
                    .unwrap_or_default();
                if !envelopes.is_empty() {
                    tracing::info!(
                        "Reindexing {} existing messages for {}",
                        envelopes.len(),
                        account.email
                    );
                    let mut search = state.search.lock().await;
                    for env in &envelopes {
                        let _ = search.index_envelope(env);
                    }
                    let _ = search.commit();
                }
            }
        }
    }

    // All syncing happens in the background sync loops — no blocking initial sync.
    // The daemon starts accepting clients immediately. The sync loops detect
    // Initial/GmailBackfill cursors and handles them with no startup delay.

    // Spawn background loops
    loops::spawn_sync_loops(state.clone());

    let snooze_state = state.clone();
    tokio::spawn(async move {
        loops::snooze_loop(snooze_state).await;
    });

    // Accept connections
    loop {
        let (stream, _addr) = listener.accept().await?;
        let state = state.clone();
        let mut event_rx = state.event_tx.subscribe();

        tokio::spawn(async move {
            let (mut sink, mut stream) = Framed::new(stream, IpcCodec::new()).split();
            let (resp_tx, mut resp_rx) = mpsc::unbounded_channel::<IpcMessage>();

            loop {
                tokio::select! {
                    msg = stream.next() => {
                        match msg {
                            Some(Ok(ipc_msg)) => {
                                // Spawn handler as a task — requests run concurrently
                                let state = state.clone();
                                let resp_tx = resp_tx.clone();
                                tokio::spawn(async move {
                                    let response = guard_ipc_response(ipc_msg.id, async {
                                        handle_request(&state, &ipc_msg).await
                                    })
                                    .await;
                                    let _ = resp_tx.send(response);
                                });
                            }
                            Some(Err(e)) => {
                                tracing::error!("IPC decode error: {}", e);
                                break;
                            }
                            None => break,
                        }
                    }
                    resp = resp_rx.recv() => {
                        if let Some(resp_msg) = resp {
                            if sink.send(resp_msg).await.is_err() {
                                break;
                            }
                        }
                    }
                    event = event_rx.recv() => {
                        if let Ok(event_msg) = event {
                            if sink.send(event_msg).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }

            tracing::debug!("Client disconnected");
        });
    }
}

pub async fn ensure_daemon_running() -> anyhow::Result<()> {
    let sock_path = AppState::socket_path();

    match inspect_socket_state(&sock_path).await {
        SocketState::Reachable => {
            ensure_current_daemon_matches_binary(&sock_path).await?;
            return Ok(());
        }
        SocketState::Stale => {
            let _ = std::fs::remove_file(&sock_path);
        }
        SocketState::Missing => {}
    }

    spawn_daemon_process(&sock_path, "Starting daemon...").await
}

pub async fn restart_daemon() -> anyhow::Result<()> {
    let sock_path = AppState::socket_path();
    restart_daemon_process(
        &sock_path,
        None,
        "Restarting daemon to match the current binary...",
    )
    .await
}

pub async fn ensure_daemon_supports_tui() -> anyhow::Result<()> {
    let snapshot =
        fetch_daemon_status_snapshot_from_path(&AppState::socket_path(), STATUS_REQUEST_TIMEOUT)
            .await?;

    if snapshot.protocol_version >= mxr_protocol::IPC_PROTOCOL_VERSION {
        Ok(())
    } else {
        anyhow::bail!(
            "The running daemon is using IPC protocol {} but this TUI expects {}. Restart the existing daemon after upgrading, then rerun `mxr`.",
            snapshot.protocol_version,
            mxr_protocol::IPC_PROTOCOL_VERSION
        )
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DaemonStatusSnapshot {
    pub daemon_pid: Option<u32>,
    pub protocol_version: u32,
    pub daemon_version: Option<String>,
    pub daemon_build_id: Option<String>,
}

pub(crate) fn current_daemon_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub(crate) fn current_build_id() -> String {
    let version = current_daemon_version();
    let Ok(exe) = std::env::current_exe() else {
        return format!("{version}:unknown");
    };
    let path = std::fs::canonicalize(&exe).unwrap_or(exe);
    let Ok(meta) = std::fs::metadata(&path) else {
        return format!("{version}:{}", path.display());
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{version}:{}:{}:{modified}", path.display(), meta.len())
}

pub(crate) fn daemon_requires_restart(
    protocol_version: u32,
    daemon_version: Option<&str>,
    daemon_build_id: Option<&str>,
) -> bool {
    let current_build_id = current_build_id();
    protocol_version != IPC_PROTOCOL_VERSION
        || daemon_version != Some(current_daemon_version())
        || daemon_build_id != Some(current_build_id.as_str())
}

pub(crate) fn classify_health(
    sync_statuses: &[AccountSyncStatus],
    repair_required: bool,
    restart_required: bool,
) -> DaemonHealthClass {
    if restart_required {
        DaemonHealthClass::RestartRequired
    } else if repair_required {
        DaemonHealthClass::RepairRequired
    } else if sync_statuses.iter().any(|status| !status.healthy) {
        DaemonHealthClass::Degraded
    } else {
        DaemonHealthClass::Healthy
    }
}

pub(crate) async fn search_requires_repair(state: &Arc<AppState>, total_messages: u32) -> bool {
    let search = state.search.lock().await;
    match search.search("*", 1) {
        Ok(results) => total_messages > 0 && results.is_empty(),
        Err(_) => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketState {
    Reachable,
    Stale,
    Missing,
}

async fn inspect_socket_state(path: &std::path::Path) -> SocketState {
    if !path.exists() {
        return SocketState::Missing;
    }

    if tokio::net::UnixStream::connect(path).await.is_ok() {
        SocketState::Reachable
    } else {
        SocketState::Stale
    }
}

fn is_index_lock_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("lockbusy")
        || lower.contains("lockfile")
        || lower.contains("failed to acquire index lock")
        || lower.contains("failed to acquire lockfile")
        || lower.contains("already an `indexwriter` working")
        || lower.contains("already an indexwriter working")
}

async fn ensure_current_daemon_matches_binary(sock_path: &std::path::Path) -> anyhow::Result<()> {
    let snapshot =
        match fetch_daemon_status_snapshot_from_path(sock_path, STATUS_REQUEST_TIMEOUT).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("Restarting daemon after failed status check: {error}");
                return restart_daemon_process(
                    sock_path,
                    None,
                    "Restarting daemon to recover from a bad running daemon...",
                )
                .await;
            }
        };

    if !daemon_requires_restart(
        snapshot.protocol_version,
        snapshot.daemon_version.as_deref(),
        snapshot.daemon_build_id.as_deref(),
    ) {
        return Ok(());
    }

    restart_daemon_process(
        sock_path,
        snapshot.daemon_pid,
        "Restarting daemon to match the current binary...",
    )
    .await
}

async fn fetch_daemon_status_snapshot_from_path(
    sock_path: &std::path::Path,
    timeout: Duration,
) -> anyhow::Result<DaemonStatusSnapshot> {
    let resp = tokio::time::timeout(timeout, async {
        let mut client = IpcClient::connect_to(sock_path).await?;
        client.request(Request::GetStatus).await
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "Timed out waiting for daemon status from {} after {}s",
            sock_path.display(),
            timeout.as_secs()
        )
    })??;

    match resp {
        Response::Ok {
            data:
                ResponseData::Status {
                    daemon_pid,
                    protocol_version,
                    daemon_version,
                    daemon_build_id,
                    ..
                },
        } => Ok(DaemonStatusSnapshot {
            daemon_pid,
            protocol_version,
            daemon_version,
            daemon_build_id,
        }),
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected daemon status response"),
    }
}

async fn guard_ipc_response<F>(msg_id: u64, future: F) -> IpcMessage
where
    F: std::future::Future<Output = IpcMessage>,
{
    match AssertUnwindSafe(future).catch_unwind().await {
        Ok(response) => response,
        Err(panic_payload) => {
            let panic_message = panic_payload_message(&*panic_payload);
            tracing::error!(
                request_id = msg_id,
                "Daemon handler panicked: {panic_message}"
            );
            IpcMessage {
                id: msg_id,
                payload: IpcPayload::Response(Response::Error {
                    message: format!(
                        "Daemon handler panicked while processing the request: {panic_message}"
                    ),
                }),
            }
        }
    }
}

fn panic_payload_message(panic_payload: &(dyn Any + Send)) -> String {
    if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = panic_payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

async fn restart_daemon_process(
    sock_path: &std::path::Path,
    daemon_pid: Option<u32>,
    message: &str,
) -> anyhow::Result<()> {
    eprint!("{message}");

    if matches!(
        inspect_socket_state(sock_path).await,
        SocketState::Reachable
    ) {
        let _ = request_shutdown().await;
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if !matches!(
                inspect_socket_state(sock_path).await,
                SocketState::Reachable
            ) {
                break;
            }
        }
    }

    match inspect_socket_state(sock_path).await {
        SocketState::Reachable => {
            eprintln!(" failed.");
            let pid_note = daemon_pid
                .map(|pid| format!(" (pid {pid})"))
                .unwrap_or_default();
            anyhow::bail!(
                "Existing daemon{} did not exit cleanly. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`.",
                pid_note
            );
        }
        SocketState::Stale => {
            let _ = std::fs::remove_file(sock_path);
        }
        SocketState::Missing => {}
    }

    spawn_daemon_process(sock_path, "").await
}

async fn request_shutdown() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    client.notify(Request::Shutdown).await
}

async fn daemon_responds_to_status(sock_path: &std::path::Path, timeout: Duration) -> bool {
    fetch_daemon_status_snapshot_from_path(sock_path, timeout)
        .await
        .is_ok()
}

async fn spawn_daemon_process(sock_path: &std::path::Path, prefix: &str) -> anyhow::Result<()> {
    if !prefix.is_empty() {
        eprint!("{prefix}");
    }

    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    for i in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100 * (i + 1))).await;
        if daemon_responds_to_status(sock_path, Duration::from_millis(250)).await {
            eprintln!(" ready.");
            return Ok(());
        }
    }

    eprintln!(" failed.");
    let log_path = AppState::data_dir().join("logs/mxr.log");
    if log_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&log_path) {
            let last_lines: Vec<&str> = contents.lines().rev().take(5).collect();
            eprintln!("Recent daemon logs:");
            for line in last_lines.into_iter().rev() {
                eprintln!("  {line}");
            }
        }
    }
    anyhow::bail!(
        "Failed to start daemon. Check logs at {}. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`.",
        log_path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        classify_health, current_build_id, daemon_requires_restart, daemon_responds_to_status,
        guard_ipc_response, is_index_lock_error,
    };
    use futures::{SinkExt, StreamExt};
    use mxr_core::AccountId;
    use mxr_protocol::{
        AccountSyncStatus, DaemonHealthClass, IpcCodec, IpcMessage, IpcPayload, Response,
        ResponseData, IPC_PROTOCOL_VERSION,
    };
    use std::time::Duration;
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;

    #[test]
    fn detects_tantivy_lockbusy_message() {
        let msg = "Search error: Failed to acquire Lockfile: LockBusy. Some(\"Failed to acquire index lock. If you are using a regular directory, this means there is already an `IndexWriter` working on this `Directory`, in this process or in a different process.\")";
        assert!(is_index_lock_error(msg));
    }

    #[test]
    fn ignores_unrelated_search_error() {
        assert!(!is_index_lock_error("Search error: schema does not match"));
    }

    #[test]
    fn restart_required_for_build_mismatch() {
        assert!(daemon_requires_restart(0, Some("0.0.0"), None));
        assert!(daemon_requires_restart(
            mxr_protocol::IPC_PROTOCOL_VERSION,
            Some(env!("CARGO_PKG_VERSION")),
            Some("other-build"),
        ));
        assert!(!daemon_requires_restart(
            mxr_protocol::IPC_PROTOCOL_VERSION,
            Some(env!("CARGO_PKG_VERSION")),
            Some(current_build_id().as_str()),
        ));
    }

    #[test]
    fn health_class_prioritizes_restart_then_repair_then_degraded() {
        let sync = [AccountSyncStatus {
            account_id: AccountId::new(),
            account_name: "main".into(),
            last_attempt_at: None,
            last_success_at: Some("2026-03-21T10:00:00+00:00".into()),
            last_error: None,
            failure_class: None,
            consecutive_failures: 0,
            backoff_until: None,
            sync_in_progress: false,
            current_cursor_summary: Some("initial".into()),
            last_synced_count: 1,
            healthy: true,
        }];

        assert_eq!(
            classify_health(&sync, false, true),
            DaemonHealthClass::RestartRequired
        );
        assert_eq!(
            classify_health(&sync, true, false),
            DaemonHealthClass::RepairRequired
        );

        let mut degraded = sync.to_vec();
        degraded[0].healthy = false;
        assert_eq!(
            classify_health(&degraded, false, false),
            DaemonHealthClass::Degraded
        );
    }

    #[tokio::test]
    async fn handler_panic_returns_error_response() {
        let response = guard_ipc_response(7, async {
            panic!("boom");
            #[allow(unreachable_code)]
            IpcMessage {
                id: 7,
                payload: IpcPayload::Response(Response::Ok {
                    data: ResponseData::Pong,
                }),
            }
        })
        .await;

        match response.payload {
            IpcPayload::Response(Response::Error { message }) => {
                assert!(message.contains("boom"));
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn daemon_status_probe_requires_an_actual_response() {
        let unready_socket_path = std::path::PathBuf::from(format!(
            "/tmp/mxr-unready-{}.sock",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::remove_file(&unready_socket_path);
        let _listener = UnixListener::bind(&unready_socket_path).expect("bind unready socket");

        assert!(
            !daemon_responds_to_status(&unready_socket_path, Duration::from_millis(50)).await,
            "bound socket without an accept loop should not count as ready"
        );
        let _ = std::fs::remove_file(&unready_socket_path);

        let ready_socket_path = std::path::PathBuf::from(format!(
            "/tmp/mxr-ready-{}.sock",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::remove_file(&ready_socket_path);
        let listener = UnixListener::bind(&ready_socket_path).expect("bind ready socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut framed = Framed::new(stream, IpcCodec::new());
            if let Some(Ok(message)) = framed.next().await {
                framed
                    .send(IpcMessage {
                        id: message.id,
                        payload: IpcPayload::Response(Response::Ok {
                            data: ResponseData::Status {
                                uptime_secs: 1,
                                accounts: vec!["personal".to_string()],
                                total_messages: 1,
                                daemon_pid: Some(42),
                                sync_statuses: Vec::new(),
                                protocol_version: IPC_PROTOCOL_VERSION,
                                daemon_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                                daemon_build_id: Some("test-build".to_string()),
                                repair_required: false,
                            },
                        }),
                    })
                    .await
                    .expect("send status");
            }
        });

        assert!(daemon_responds_to_status(&ready_socket_path, Duration::from_secs(1)).await);
        server.await.expect("join status server");
        let _ = std::fs::remove_file(&ready_socket_path);
    }
}
