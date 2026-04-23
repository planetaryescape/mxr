#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::handler::handle_request;
use crate::ipc_client::IpcClient;
use crate::loops;
use crate::reindex::{reindex, ReindexProgress};
use crate::state::AppState;
use futures::{FutureExt, SinkExt, StreamExt};
use mxr_protocol::{
    AccountSyncStatus, DaemonHealthClass, IpcCodec, IpcMessage, IpcPayload, Request, Response,
    ResponseData, IPC_PROTOCOL_VERSION,
};
use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, watch, Semaphore};
use tokio::task::JoinSet;
use tokio_util::codec::Framed;

const STATUS_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_CONCURRENCY_LIMIT: usize = 64;
const CONNECTION_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_PROBE_ATTEMPTS: usize = 5;
const SOCKET_PROBE_DELAY: Duration = Duration::from_millis(100);
const ORPHAN_DAEMON_EXIT_TIMEOUT: Duration = Duration::from_secs(5);

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
    write_daemon_pid_file()?;
    tracing::info!("Daemon listening on {}", sock_path.display());
    let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));

    // All syncing happens in the background sync loops — no blocking initial sync.
    // The daemon starts accepting clients immediately. The sync loops detect
    // Initial/GmailBackfill cursors and handles them with no startup delay.

    // Spawn background loops
    loops::spawn_sync_loops(state.clone());
    let startup_handle = spawn_startup_maintenance(state.clone());
    state.register_startup_maintenance(startup_handle);

    let snooze_state = state.clone();
    let snooze_handle = tokio::spawn(async move {
        let shutdown_rx = snooze_state.shutdown_receiver();
        loops::snooze_loop(snooze_state, shutdown_rx).await;
    });
    state.register_snooze_loop(snooze_handle);

    let mut shutdown_rx = state.shutdown_receiver();
    let mut connections = JoinSet::new();

    // Accept connections
    loop {
        tokio::select! {
            joined = connections.join_next(), if !connections.is_empty() => {
                match joined {
                    Some(Ok(())) => {}
                    Some(Err(error)) => {
                        tracing::warn!("client connection task failed: {error}");
                    }
                    None => {}
                }
            }
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow_and_update() {
                    tracing::info!("Daemon shutdown requested; stopping IPC accept loop");
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _addr) = accepted?;
                let state = state.clone();
                let request_semaphore = request_semaphore.clone();
                let event_rx = state.event_tx.subscribe();
                let connection_shutdown_rx = state.shutdown_receiver();

                connections.spawn(async move {
                    serve_client_connection(
                        stream,
                        state,
                        request_semaphore,
                        event_rx,
                        connection_shutdown_rx,
                    )
                    .await;
                });
            }
        }
    }

    drop(listener);
    drain_connection_tasks(&mut connections, CONNECTION_DRAIN_TIMEOUT).await;
    state.shutdown_runtime_tasks(Duration::from_secs(5)).await;
    let _ = std::fs::remove_file(&sock_path);
    clear_daemon_pid_file();
    Ok(())
}

async fn serve_client_connection(
    stream: UnixStream,
    state: Arc<AppState>,
    request_semaphore: Arc<Semaphore>,
    mut event_rx: broadcast::Receiver<IpcMessage>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let (mut sink, mut stream) = Framed::new(stream, IpcCodec::new()).split();
    let mut request_tasks = JoinSet::new();
    let mut accept_requests = true;
    let mut can_send = true;
    let mut shutdown_requested = false;

    loop {
        tokio::select! {
            biased;

            joined = request_tasks.join_next(), if !request_tasks.is_empty() => {
                match joined {
                    Some(Ok(response)) => {
                        if can_send && sink.send(response).await.is_err() {
                            can_send = false;
                            accept_requests = false;
                        }
                    }
                    Some(Err(error)) => {
                        tracing::warn!("ipc request task failed: {error}");
                    }
                    None => {}
                }
            }
            changed = shutdown_rx.changed(), if !shutdown_requested => {
                match changed {
                    Ok(()) if *shutdown_rx.borrow_and_update() => {
                        shutdown_requested = true;
                        accept_requests = false;
                    }
                    Ok(()) => {}
                    Err(_) => {
                        shutdown_requested = true;
                        accept_requests = false;
                    }
                }
            }
            msg = stream.next(), if accept_requests => {
                match msg {
                    Some(Ok(ipc_msg)) => {
                        let permit_wait_started = std::time::Instant::now();
                        let permit = match request_semaphore.clone().acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => {
                                accept_requests = false;
                                continue;
                            }
                        };
                        let state = state.clone();
                        request_tasks.spawn(async move {
                            let _permit = permit;
                            tracing::trace!(
                                wait_ms = permit_wait_started.elapsed().as_secs_f64() * 1000.0,
                                "ipc request permit acquired"
                            );
                            guard_ipc_response(ipc_msg.id, async {
                                handle_request(&state, &ipc_msg).await
                            })
                            .await
                        });
                    }
                    Some(Err(error)) => {
                        tracing::error!("IPC decode error: {}", error);
                        accept_requests = false;
                    }
                    None => {
                        accept_requests = false;
                    }
                }
            }
            event = event_rx.recv(), if accept_requests && can_send && !shutdown_requested => {
                match event {
                    Ok(event_msg) => {
                        if sink.send(event_msg).await.is_err() {
                            can_send = false;
                            accept_requests = false;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::debug!(skipped, "client event stream lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        accept_requests = false;
                    }
                }
            }
        }

        if !accept_requests && request_tasks.is_empty() {
            break;
        }
    }

    tracing::debug!("Client disconnected");
}

async fn drain_connection_tasks(connections: &mut JoinSet<()>, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while !connections.is_empty() {
        let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
            tracing::warn!("client connection drain timed out");
            connections.abort_all();
            while let Some(joined) = connections.join_next().await {
                if let Err(error) = joined {
                    tracing::trace!("aborted client connection task: {error}");
                }
            }
            return;
        };

        match tokio::time::timeout(remaining, connections.join_next()).await {
            Ok(Some(Ok(()))) => {}
            Ok(Some(Err(error))) => tracing::warn!("client connection task failed: {error}"),
            Ok(None) => break,
            Err(_) => {
                tracing::warn!("client connection drain timed out");
                connections.abort_all();
                while let Some(joined) = connections.join_next().await {
                    if let Err(error) = joined {
                        tracing::trace!("aborted client connection task: {error}");
                    }
                }
                return;
            }
        }
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
            if let Some(pid) = live_daemon_pid() {
                return recover_broken_running_daemon(
                    &sock_path,
                    pid,
                    "Restarting daemon to recover from a missing IPC socket...",
                )
                .await;
            }
            let _ = std::fs::remove_file(&sock_path);
            clear_daemon_pid_file();
        }
        SocketState::Missing => {
            if let Some(pid) = live_daemon_pid() {
                return recover_broken_running_daemon(
                    &sock_path,
                    pid,
                    "Restarting daemon to recover from a missing IPC socket...",
                )
                .await;
            }
        }
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

pub(crate) async fn search_requires_repair(state: &AppState, total_messages: u32) -> bool {
    if total_messages == 0 {
        return false;
    }

    match tokio::time::timeout(
        Duration::from_millis(50),
        state
            .search
            .search("*", 1, 0, mxr_core::types::SortOrder::DateDesc),
    )
    .await
    {
        Ok(Ok(results)) => results.results.is_empty(),
        Ok(Err(_)) => true,
        Err(_) => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SocketState {
    Reachable,
    Stale,
    Missing,
}

pub(crate) async fn inspect_socket_state(path: &std::path::Path) -> SocketState {
    if !path.exists() {
        return SocketState::Missing;
    }

    if socket_accepts_connections(path).await {
        SocketState::Reachable
    } else {
        SocketState::Stale
    }
}

async fn socket_accepts_connections(path: &Path) -> bool {
    for attempt in 0..SOCKET_PROBE_ATTEMPTS {
        match tokio::net::UnixStream::connect(path).await {
            Ok(_) => return true,
            Err(error)
                if should_retry_socket_probe(&error) && attempt + 1 < SOCKET_PROBE_ATTEMPTS =>
            {
                tokio::time::sleep(SOCKET_PROBE_DELAY).await;
            }
            Err(_) => return false,
        }
    }

    false
}

fn should_retry_socket_probe(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
    )
}

fn daemon_pid_file_path() -> PathBuf {
    AppState::data_dir().join("daemon.pid")
}

fn write_daemon_pid_file() -> anyhow::Result<()> {
    let pid_path = daemon_pid_file_path();
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(pid_path, std::process::id().to_string())?;
    Ok(())
}

fn clear_daemon_pid_file() {
    let _ = std::fs::remove_file(daemon_pid_file_path());
}

fn read_daemon_pid_file() -> Option<u32> {
    std::fs::read_to_string(daemon_pid_file_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn process_is_alive(pid: u32) -> bool {
    match kill(Pid::from_raw(pid as i32), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => false,
    }
}

fn live_daemon_pid() -> Option<u32> {
    if let Some(pid) = read_daemon_pid_file() {
        if process_is_alive(pid) {
            return Some(pid);
        }
        clear_daemon_pid_file();
    }

    let pid = fallback_live_daemon_pid_without_pid_file()?;
    let _ = std::fs::write(daemon_pid_file_path(), pid.to_string());
    Some(pid)
}

fn fallback_live_daemon_pid_without_pid_file() -> Option<u32> {
    let current_exe = std::env::current_exe().ok()?;
    let current_name = current_exe.file_name()?.to_str()?;
    let output = std::process::Command::new("ps")
        .args(["-Ao", "pid=,command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let mut matches = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }

        let Some(exe) = parts.next() else {
            continue;
        };
        let Some(arg1) = parts.next() else {
            continue;
        };
        if parts.next().is_some() || arg1 != "daemon" {
            continue;
        }

        let Some(exe_name) = Path::new(exe).file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if exe_name == current_name && process_is_alive(pid) {
            matches.push(pid);
        }
    }

    if matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    }
}

async fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if !process_is_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    !process_is_alive(pid)
}

fn send_signal(pid: u32, signal: Signal) -> anyhow::Result<()> {
    match kill(Pid::from_raw(pid as i32), Some(signal)) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to send {signal:?} to daemon pid {pid}: {error}"
        )),
    }
}

async fn recover_broken_running_daemon(
    sock_path: &Path,
    daemon_pid: u32,
    message: &str,
) -> anyhow::Result<()> {
    eprint!("{message}");

    send_signal(daemon_pid, Signal::SIGTERM)?;
    if !wait_for_process_exit(daemon_pid, ORPHAN_DAEMON_EXIT_TIMEOUT).await {
        send_signal(daemon_pid, Signal::SIGKILL)?;
        if !wait_for_process_exit(daemon_pid, Duration::from_secs(1)).await {
            eprintln!(" failed.");
            anyhow::bail!(
                "Broken daemon pid {} did not exit cleanly. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`.",
                daemon_pid
            );
        }
    }

    clear_daemon_pid_file();
    let _ = std::fs::remove_file(sock_path);
    spawn_daemon_process(sock_path, "").await
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
            clear_daemon_pid_file();
        }
        SocketState::Missing => {}
    }

    spawn_daemon_process(sock_path, "").await
}

pub(crate) async fn shutdown_daemon_for_maintenance(
    sock_path: &std::path::Path,
    wait_timeout: Duration,
) -> anyhow::Result<SocketState> {
    let mut state = inspect_socket_state(sock_path).await;
    if !matches!(state, SocketState::Reachable) {
        return Ok(state);
    }

    let _ = request_shutdown_to(sock_path).await;
    let deadline = std::time::Instant::now() + wait_timeout;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = inspect_socket_state(sock_path).await;
        if !matches!(state, SocketState::Reachable) {
            return Ok(state);
        }
    }

    Ok(inspect_socket_state(sock_path).await)
}

async fn request_shutdown() -> anyhow::Result<()> {
    request_shutdown_to(&AppState::socket_path()).await
}

async fn request_shutdown_to(sock_path: &std::path::Path) -> anyhow::Result<()> {
    let mut client = IpcClient::connect_to(sock_path).await?;
    match client.request(Request::Shutdown).await? {
        Response::Ok {
            data: ResponseData::Ack,
        } => Ok(()),
        other => anyhow::bail!("unexpected shutdown response: {other:?}"),
    }
}

async fn daemon_responds_to_status(sock_path: &std::path::Path, timeout: Duration) -> bool {
    fetch_daemon_status_snapshot_from_path(sock_path, timeout)
        .await
        .is_ok()
}

fn spawn_startup_maintenance(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = run_startup_maintenance(state).await {
            tracing::warn!("startup maintenance failed: {error}");
        }
    })
}

async fn run_startup_maintenance(state: Arc<AppState>) -> anyhow::Result<()> {
    let total_messages = state.store.count_all_messages().await.unwrap_or_default();
    if total_messages == 0 {
        return Ok(());
    }

    let indexed_messages = state.search.num_docs().await.unwrap_or_default();

    if indexed_messages == total_messages as u64 {
        return Ok(());
    }

    // Startup maintenance only repairs the lexical Tantivy index from SQLite.
    // Semantic chunks/embeddings remain an optional platform layer and are not
    // part of this mandatory mail-readiness repair path.
    tracing::info!(
        indexed_messages,
        total_messages,
        "Reindexing lexical index from SQLite"
    );
    let _ = reindex(&state.search, &state.store, |progress| match progress {
        ReindexProgress::Starting { total } => {
            tracing::info!(total, "Lexical reindex started");
        }
        ReindexProgress::Indexing { indexed, total }
            if indexed == total || indexed % 10_000 == 0 =>
        {
            tracing::info!(indexed, total, "Lexical reindex progress");
        }
        ReindexProgress::Indexing { .. } => {}
        ReindexProgress::Complete { indexed } => {
            tracing::info!(indexed, "Lexical reindex complete");
        }
    })
    .await?;

    Ok(())
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
        guard_ipc_response, is_index_lock_error, request_shutdown_to, serve_client_connection,
        spawn_startup_maintenance, REQUEST_CONCURRENCY_LIMIT,
    };
    use crate::{handler::handle_request, state::AppState};
    use chrono::Utc;
    use futures::{SinkExt, StreamExt};
    use mxr_core::{
        id::{AccountId, MessageId, ThreadId},
        types::{Address, Envelope, MessageFlags, UnsubscribeMethod},
    };
    use mxr_protocol::{
        AccountSyncStatus, DaemonHealthClass, IpcCodec, IpcMessage, IpcPayload, Request, Response,
        ResponseData, IPC_PROTOCOL_VERSION,
    };
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::{UnixListener, UnixStream};
    use tokio::sync::Semaphore;
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
    async fn startup_maintenance_repairs_partial_index() {
        let state = Arc::new(AppState::in_memory().await.expect("state"));
        let indexed_envelope = Envelope {
            id: MessageId::new(),
            account_id: state.default_account_id(),
            provider_id: "provider-msg-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<msg-1@example.com>".into()),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Sender".into()),
                email: "sender@example.com".into(),
            },
            to: vec![Address {
                name: Some("User".into()),
                email: "user@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "startup reindex subject".into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "startup reindex snippet".into(),
            has_attachments: false,
            size_bytes: 128,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: Vec::new(),
        };
        let missing_envelope = Envelope {
            id: MessageId::new(),
            account_id: state.default_account_id(),
            provider_id: "provider-msg-2".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<msg-2@example.com>".into()),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Sender".into()),
                email: "sender@example.com".into(),
            },
            to: vec![Address {
                name: Some("User".into()),
                email: "user@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "missing corpus subject".into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "missing corpus snippet".into(),
            has_attachments: false,
            size_bytes: 128,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: Vec::new(),
        };

        state
            .store
            .upsert_envelope(&indexed_envelope)
            .await
            .expect("insert envelope");
        state
            .store
            .upsert_envelope(&missing_envelope)
            .await
            .expect("insert envelope");

        state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: vec![mxr_search::SearchIndexEntry {
                    envelope: indexed_envelope.clone(),
                    body: None,
                }],
                removed_message_ids: Vec::new(),
            })
            .await
            .expect("index partial envelope");

        assert!(state
            .search
            .search("missing", 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .expect("pre-maintenance search")
            .results
            .is_empty());

        spawn_startup_maintenance(state.clone())
            .await
            .expect("join maintenance task");

        let results = state
            .search
            .search("missing", 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .expect("search after reindex");
        assert_eq!(results.results.len(), 1);
    }

    #[tokio::test]
    async fn startup_maintenance_reindexes_without_blocking_ping_requests() {
        let state = Arc::new(AppState::in_memory().await.expect("state"));
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: state.default_account_id(),
            provider_id: "provider-msg-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<msg-1@example.com>".into()),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Sender".into()),
                email: "sender@example.com".into(),
            },
            to: vec![Address {
                name: Some("User".into()),
                email: "user@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "startup reindex subject".into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "startup reindex snippet".into(),
            has_attachments: false,
            size_bytes: 128,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: Vec::new(),
        };

        state
            .store
            .upsert_envelope(&envelope)
            .await
            .expect("insert envelope");
        assert!(state
            .search
            .search("startup", 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .expect("empty search")
            .results
            .is_empty());

        let maintenance = spawn_startup_maintenance(state.clone());
        let ping = handle_request(
            &state,
            &IpcMessage {
                id: 1,
                payload: IpcPayload::Request(Request::Ping),
            },
        )
        .await;

        match ping.payload {
            IpcPayload::Response(Response::Ok { .. }) => {}
            other => panic!("expected ping response, got {other:?}"),
        }

        maintenance.await.expect("join maintenance task");

        let results = state
            .search
            .search("startup", 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .expect("search after reindex");
        assert_eq!(results.results.len(), 1);
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
                                semantic_runtime: None,
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

    #[tokio::test]
    async fn shutdown_request_waits_for_acknowledgement() {
        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/mxr-shutdown-ack-{}.sock",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("bind shutdown socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut framed = Framed::new(stream, IpcCodec::new());
            match framed.next().await {
                Some(Ok(message)) => {
                    assert!(matches!(
                        message.payload,
                        IpcPayload::Request(Request::Shutdown)
                    ));
                    framed
                        .send(IpcMessage {
                            id: message.id,
                            payload: IpcPayload::Response(Response::Ok {
                                data: ResponseData::Ack,
                            }),
                        })
                        .await
                        .expect("send shutdown ack");
                }
                other => panic!("expected shutdown request, got {other:?}"),
            }
        });

        request_shutdown_to(&socket_path)
            .await
            .expect("shutdown request");

        server.await.expect("join shutdown ack server");
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn client_connection_acknowledges_shutdown_before_exiting() {
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let state_for_cleanup = state.clone();
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();

        let server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                state,
                request_semaphore,
                event_rx,
                shutdown_rx,
            )
            .await;
        });

        let mut client = Framed::new(client_stream, IpcCodec::new());
        client
            .send(IpcMessage {
                id: 44,
                payload: IpcPayload::Request(Request::Shutdown),
            })
            .await
            .expect("send shutdown request");

        let response = tokio::time::timeout(Duration::from_secs(1), client.next())
            .await
            .expect("response should arrive")
            .expect("response frame")
            .expect("response should decode");

        match response.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("expected shutdown ack, got {other:?}"),
        }

        drop(client);

        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .expect("connection task should exit")
            .expect("connection task join");

        state_for_cleanup
            .shutdown_runtime_tasks(Duration::from_secs(1))
            .await;
    }
}
