#![cfg_attr(
    test,
    expect(
        clippy::panic,
        reason = "tests panic with diagnostic context for direct failures"
    )
)]

use crate::ipc_client::IpcClient;
use crate::loops;
use crate::reindex::{reindex, ReindexProgress};
use crate::serve::{
    drain_connection_tasks, serve_client_connection, BULK_CONCURRENCY_LIMIT,
    CONNECTION_DRAIN_TIMEOUT, REQUEST_CONCURRENCY_LIMIT,
};
use crate::state::AppState;
use mxr_protocol::{
    AccountSyncStatus, DaemonHealthClass, Request, Response, ResponseData, IPC_PROTOCOL_VERSION,
};
use mxr_transport::{
    BoxedIo, Connector, PeerInfo, ServerTransport, TransportError, TransportListener,
    UdsServerTransport, UnixConnector,
};
use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const STATUS_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_PROBE_ATTEMPTS: usize = 5;
const SOCKET_PROBE_DELAY: Duration = Duration::from_millis(100);
const ORPHAN_DAEMON_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a restart waits for the previous daemon process to fully exit
/// before spawning its successor. Graceful shutdown can take connection
/// drain (5s) + runtime-task drain (5s) + final flushes; matches the window
/// `shutdown_daemon_for_maintenance` already allows.
const DAEMON_EXIT_DRAIN_TIMEOUT: Duration = Duration::from_secs(12);

/// CLI-time overrides for the HTTP bridge. Always merged on top of the
/// `[bridge]` section in `~/.config/mxr/config.toml`.
#[derive(Debug, Clone, Default)]
pub struct BridgeOverrides {
    pub disabled: bool,
    pub port: Option<u16>,
}

pub async fn run_daemon() -> anyhow::Result<()> {
    run_daemon_with_overrides(BridgeOverrides::default()).await
}

/// `mxr daemon --stdio` — serve exactly ONE connection over this process's
/// stdin/stdout, the LSP/inetd model (phase 5b, transport adapters).
///
/// The process IS the daemon for the lifetime of one stdio connection: it
/// acquires the same exclusive state as a socket daemon (so it cannot run
/// alongside one), serves the single connection through the generic serve core
/// over `tokio::io::join(stdin, stdout)`, and exits when stdin closes
/// (connection lifetime = process lifetime). No UDS socket is bound and no HTTP
/// bridge is started — a stdio server owns exactly one client.
///
/// Peer trust: [`PeerInfo::local`] (`LocalProcess`) — the spawner is the
/// authenticator (discovery §7), exactly like the in-process transport. No
/// token handshake.
///
/// **Stdout discipline:** frames own stdout. Tracing is file-only in this mode
/// (the dispatcher calls `init_tracing(false)`, as for `dial-stdio`); nothing on
/// this path writes to stdout before or during serving. Diagnostics and the
/// index-lock conflict message go to stderr.
pub async fn run_stdio() -> anyhow::Result<()> {
    // Acquire the exclusive runtime state (search-index lock included). A
    // running socket daemon holds this, so `--stdio` cannot collide with one;
    // surface that as a clear stderr message rather than a raw lock error.
    let state = Arc::new(match AppState::new().await {
        Ok(state) => state,
        Err(error) if is_index_lock_error(&error.to_string()) => {
            anyhow::bail!(
                "Cannot start a --stdio daemon: another daemon already holds the runtime lock. \
                 Stop it first, or connect to it with `mxr daemon dial-stdio`.\nOriginal error: {error}"
            );
        }
        Err(error) => return Err(error),
    });

    let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
    let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
    let event_rx = state.event_tx.subscribe();
    let shutdown_rx = state.shutdown_receiver();

    // One connection over stdin/stdout. `LocalProcess` peer trust: no token
    // gate (the spawner vouches for the peer). Returns when stdin hits EOF.
    let stream = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    serve_client_connection(
        stream,
        state.clone(),
        request_semaphore,
        bulk_semaphore,
        PeerInfo::local(),
        None,
        event_rx,
        shutdown_rx,
    )
    .await;

    // Drain any in-flight background work spawned by handlers, then exit.
    state.request_shutdown();
    state.shutdown_runtime_tasks(Duration::from_secs(5)).await;
    Ok(())
}

pub async fn run_daemon_with_overrides(bridge_overrides: BridgeOverrides) -> anyhow::Result<()> {
    // Bind where every CLI-side probe/request will look (honors MXR_DAEMON_ADDR).
    let sock_path = resolve_daemon_socket()?;
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // A responsive daemon already owns the socket — refuse immediately,
    // without touching anything.
    if matches!(
        inspect_socket_state(&sock_path).await,
        SocketState::Reachable
    ) {
        anyhow::bail!(
            "Daemon already running at {}. Use `mxr status` or `mxr logs --level error`, or stop the existing daemon before rerunning `mxr daemon --foreground`.",
            sock_path.display()
        );
    }

    // Acquire exclusive resources (the search-index write lock) BEFORE
    // touching the socket file. The index lock — not the socket probe — is
    // the authoritative singleton guard: a busy daemon whose socket probe
    // momentarily times out still holds it, so we bail here with its socket
    // left intact.
    //
    // Ordering is load-bearing. The previous code removed a "stale" socket
    // first, then failed `AppState::new` on the lock and exited — which
    // orphaned the still-running daemon with no socket and permanently
    // wedged IPC (clients could neither connect nor restart it).
    let state = Arc::new(match AppState::new().await {
        Ok(state) => state,
        Err(error) if is_index_lock_error(&error.to_string()) => {
            anyhow::bail!(
                "Daemon already running (search index is locked by another process) at {}. Use `mxr status` or `mxr logs --level error`, or stop the existing daemon.\nOriginal error: {error}",
                sock_path.display()
            );
        }
        Err(error) => return Err(error),
    });

    // We hold the exclusive lock, so we are the sole daemon. Build the
    // configured transports (factory match over config — UDS always on, TCP
    // opt-in) and bind each. `UdsServerTransport::bind` owns the socket
    // lifecycle that used to live inline here: clear a genuinely-stale socket,
    // bind, chmod 0600, and remember our socket identity for successor-safe
    // cleanup. The pid file and index-lock singleton stay daemon-level.
    let tcp_cfg = mxr_config::load_config()
        .map(|config| config.transports.tcp)
        .unwrap_or_default();
    // When the TCP transport is enabled, resolve (creating on first run) the
    // shared daemon token that its connections must present. `None` for a
    // UDS-only daemon; the serve core only consults it for `TokenRequired`
    // peers, so UDS/memory connections are never affected.
    let auth_token: Option<Arc<str>> = if tcp_cfg.enabled {
        match mxr_config::resolve_daemon_token(true) {
            Ok(Some(token)) => Some(Arc::from(token.as_str())),
            Ok(None) => None,
            Err(error) => {
                anyhow::bail!("could not resolve the daemon token for the TCP transport: {error}");
            }
        }
    } else {
        None
    };
    let transports = build_transports(&sock_path, &tcp_cfg);
    let mut listeners: Vec<Box<dyn TransportListener>> = Vec::with_capacity(transports.len());
    for transport in &transports {
        match transport.bind().await {
            Ok(listener) => {
                tracing::info!("Daemon listening on {}", listener.endpoint());
                listeners.push(listener);
            }
            Err(error) => {
                // A partial bind must not leave earlier listeners' sockets
                // behind: clean them up before failing.
                for listener in &mut listeners {
                    let _ = listener.cleanup().await;
                }
                return Err(error.into());
            }
        }
    }

    // Every post-bind exit — a clean shutdown OR any error (pid-file write,
    // bridge startup, accept failure) — must funnel through the ordered
    // teardown after this block, so no exit path leaves a stale socket. The
    // serving body runs in a guarded scope; teardown then runs unconditionally.
    let mut connections = JoinSet::new();
    let serve_result: anyhow::Result<()> = async {
        write_daemon_pid_file()?;
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));

        // All syncing happens in the background sync loops — no blocking initial sync.
        // The daemon starts accepting clients immediately. The sync loops detect
        // Initial/GmailBackfill cursors and handles them with no startup delay.

        // A previous daemon that died mid-sync leaves sync_in_progress=true
        // behind; clear it before any loop can read it as "already syncing".
        loops::reconcile_interrupted_syncs(&state).await;

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

        let reminders_state = state.clone();
        let reminders_handle = tokio::spawn(async move {
            let shutdown_rx = reminders_state.shutdown_receiver();
            loops::auto_reminders_loop(reminders_state, shutdown_rx).await;
        });
        state.register_auto_reminders_loop(reminders_handle);

        let sends_state = state.clone();
        let sends_handle = tokio::spawn(async move {
            let shutdown_rx = sends_state.shutdown_receiver();
            loops::scheduled_sends_loop(sends_state, shutdown_rx).await;
        });
        state.register_scheduled_sends_loop(sends_handle);

        let reconciler_state = state.clone();
        let reconciler_handle = tokio::spawn(async move {
            let shutdown_rx = reconciler_state.shutdown_receiver();
            loops::reply_pair_reconciler_loop(reconciler_state, shutdown_rx).await;
        });
        state.register_reply_pair_reconciler(reconciler_handle);

        let contacts_state = state.clone();
        let contacts_handle = tokio::spawn(async move {
            let shutdown_rx = contacts_state.shutdown_receiver();
            loops::contacts_refresher_loop(contacts_state, shutdown_rx).await;
        });
        state.register_contacts_refresher(contacts_handle);

        let wrapped_warmer_state = state.clone();
        let wrapped_warmer_handle = tokio::spawn(async move {
            let shutdown_rx = wrapped_warmer_state.shutdown_receiver();
            loops::wrapped_warmer_loop(wrapped_warmer_state, shutdown_rx).await;
        });
        state.register_wrapped_warmer(wrapped_warmer_handle);

        // Activity prune loop: enforces the tiered retention windows from
        // config. Fire-and-forget; on shutdown the watch channel exits the
        // loop. Not registered with `runtime_tasks` because we don't need to
        // join on it during graceful shutdown — losing the last sweep is
        // harmless.
        let activity_prune_state = state.clone();
        tokio::spawn(async move {
            let shutdown_rx = activity_prune_state.shutdown_receiver();
            loops::activity_prune_loop(activity_prune_state, shutdown_rx).await;
        });

        // Mutation dedup + undo prune. 24h dedup TTL means rows older
        // than that are safe to drop; hourly cadence keeps the table
        // bounded under heavy mutation traffic.
        let mutation_dedup_state = state.clone();
        tokio::spawn(async move {
            let shutdown_rx = mutation_dedup_state.shutdown_receiver();
            loops::mutation_dedup_prune_loop(mutation_dedup_state, shutdown_rx).await;
        });

        // Managed HTTP bridge. Reads [bridge] from config, applies CLI
        // overrides, and keeps daemon-hosted serving loopback-only until remote
        // bridge TLS is a validated product decision.
        if !bridge_overrides.disabled {
            match crate::bridge::spawn_bridge_loop(
                state.clone(),
                &bridge_overrides,
                sock_path.clone(),
            )
            .await
            {
                Ok(Some(handle)) => {
                    state.register_bridge_loop(handle);
                }
                Ok(None) => {
                    tracing::info!("bridge disabled by config");
                }
                Err(crate::bridge::BridgeStartupError::Bind { addr, error }) => {
                    tracing::warn!(
                        %addr,
                        %error,
                        "HTTP bridge disabled because its port is unavailable"
                    );
                }
                Err(error) => {
                    anyhow::bail!("bridge startup failed: {error}");
                }
            }
        } else {
            tracing::info!("bridge disabled by --no-bridge flag");
        }

        let mut shutdown_rx = state.shutdown_receiver();

        // Accept connections from every bound transport.
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
                accepted = accept_any(&mut listeners), if !listeners.is_empty() => {
                    let (stream, peer) = accepted?;
                    let state = state.clone();
                    let request_semaphore = request_semaphore.clone();
                    let bulk_semaphore = bulk_semaphore.clone();
                    let event_rx = state.event_tx.subscribe();
                    let connection_shutdown_rx = state.shutdown_receiver();
                    let auth_token = auth_token.clone();

                    connections.spawn(async move {
                        serve_client_connection(
                            stream,
                            state,
                            request_semaphore,
                            bulk_semaphore,
                            peer,
                            auth_token,
                            event_rx,
                            connection_shutdown_rx,
                        )
                        .await;
                    });
                }
            }
        }
        Ok(())
    }
    .await;

    // ── Ordered teardown — runs on EVERY post-bind exit (clean or error) ──
    // Stop accepting FIRST — before the drains — so new clients get a prompt
    // connection-refused during shutdown instead of hanging against a listening
    // socket that no longer has an accept loop. The socket file is NOT unlinked
    // yet: that is deferred to `cleanup` below.
    for listener in &mut listeners {
        listener.stop_accepting().await;
    }
    drain_connection_tasks(&mut connections, CONNECTION_DRAIN_TIMEOUT).await;
    state.shutdown_runtime_tasks(Duration::from_secs(5)).await;
    // Release each transport's resources LAST — after the connection and
    // runtime-task drains. `UdsListener::cleanup` removes the socket file only
    // if it is still ours: a successor daemon spawned during the drain window
    // may have already re-bound the socket path, and deleting it here would
    // orphan that successor (alive and syncing, but unreachable by every
    // client). The pid file cleanup stays daemon-level for the same reason.
    for listener in &mut listeners {
        let _ = listener.cleanup().await;
    }
    drop(listeners);
    clear_daemon_pid_file_if_owned();
    serve_result
}

/// Accept from whichever bound transport is ready first. `select_all` over the
/// listeners' accept futures; the loser futures are dropped (accept is
/// cancel-safe per the `TransportListener::accept` contract). After each accept
/// the slice is rotated by one so a continuously-ready earlier listener cannot
/// starve later ones — round-robin fairness. No-op for the single UDS listener
/// configured this phase, but keeps the multi-transport claim honest.
async fn accept_any(
    listeners: &mut [Box<dyn TransportListener>],
) -> Result<(BoxedIo, PeerInfo), TransportError> {
    let result = {
        let futures = listeners.iter_mut().map(|listener| listener.accept());
        let (result, _index, _remaining) = futures::future::select_all(futures).await;
        result
    };
    listeners.rotate_left(1);
    result
}

/// Build the configured server transports. Factory match over config (provider
/// pattern): the Unix domain socket is always on; the TCP-loopback transport is
/// added when `[transports.tcp]` opts in. Its bind address is validated here so
/// an obviously-wrong `bind` fails fast with a clear message; a non-loopback
/// address is refused again at `TcpServerTransport::bind` as defense in depth.
fn build_transports(
    sock_path: &Path,
    tcp_cfg: &mxr_config::TcpTransportConfig,
) -> Vec<Box<dyn ServerTransport>> {
    let mut transports: Vec<Box<dyn ServerTransport>> =
        vec![Box::new(UdsServerTransport::new(sock_path.to_path_buf()))];

    if tcp_cfg.enabled {
        match tcp_cfg.bind.parse::<std::net::IpAddr>() {
            Ok(ip) if mxr_transport::is_loopback_ip(ip) => {
                let addr = std::net::SocketAddr::new(ip, tcp_cfg.port);
                transports.push(Box::new(mxr_transport::TcpServerTransport::new(addr)));
            }
            Ok(ip) => {
                tracing::warn!(
                    bind = %ip,
                    "ignoring [transports.tcp]: non-loopback bind is refused (use 127.0.0.1 or ::1)"
                );
            }
            Err(error) => {
                tracing::warn!(
                    bind = %tcp_cfg.bind,
                    %error,
                    "ignoring [transports.tcp]: bind is not a valid IP address"
                );
            }
        }
    }

    transports
}

/// The daemon socket path every CLI-side operation agrees on. The daemon's
/// bind, autostart, the liveness/stale probe, doctor's reachability check, and
/// the request path (`IpcClient`) all resolve here, so start / probe / request
/// can never disagree. `MXR_DAEMON_ADDR` (`unix://<path>`) takes precedence over
/// `MXR_SOCKET_PATH` / the per-instance default; only `unix://` exists this
/// phase.
///
/// The standalone `mxr-tui` / `mxr-web` / `mxr-mcp` clients still resolve their
/// socket through `mxr_config::socket_path()` and do NOT yet honor
/// `MXR_DAEMON_ADDR`; that adoption lands in phase 5 (see decision log D053).
pub(crate) fn resolve_daemon_socket() -> anyhow::Result<PathBuf> {
    // The daemon's own UDS bind, autostart, and the UDS liveness probe are
    // Unix-only concepts. A `tcp://` / `cmd://` value in `MXR_DAEMON_ADDR` is a
    // *client-side* transport override — it does not relocate the daemon's Unix
    // socket, so those schemes fall back to the default UDS path here.
    match resolve_daemon_addr()? {
        mxr_transport::TransportAddr::Unix(path) => Ok(path),
        mxr_transport::TransportAddr::Tcp(_) | mxr_transport::TransportAddr::Cmd(_) => {
            Ok(AppState::socket_path())
        }
    }
}

/// The resolved client transport address (honors `MXR_DAEMON_ADDR`). The single
/// source every client-side connect agrees on — `unix://`, `tcp://<host:port>`,
/// or `cmd://<command>`. Unix is the default when `MXR_DAEMON_ADDR` is unset.
pub(crate) fn resolve_daemon_addr() -> anyhow::Result<mxr_transport::TransportAddr> {
    mxr_transport::TransportAddr::resolve(AppState::socket_path())
        .map_err(|error| anyhow::anyhow!("invalid {}: {error}", mxr_transport::DAEMON_ADDR_ENV))
}

/// Build the CLI's daemon connector from `MXR_DAEMON_ADDR`. `unix://` keeps the
/// path-based Unix connector (and the whole autostart/stale-socket story);
/// `tcp://` dials loopback with the resolved bearer token (env
/// `MXR_DAEMON_TOKEN` > token file); `cmd://` spawns the command and pipes its
/// stdio (SSH / container bridges). Token/cmd transports do not autostart a
/// local daemon.
pub(crate) fn build_cli_connector() -> anyhow::Result<Box<dyn Connector>> {
    Ok(match resolve_daemon_addr()? {
        mxr_transport::TransportAddr::Unix(path) => Box::new(UnixConnector::new(path)),
        mxr_transport::TransportAddr::Tcp(addr) => {
            let token = mxr_config::resolve_daemon_token(false)
                .map_err(|error| anyhow::anyhow!("could not read the daemon token: {error}"))?;
            Box::new(mxr_transport::TcpConnector::new(addr, token))
        }
        mxr_transport::TransportAddr::Cmd(argv) => Box::new(mxr_transport::CmdConnector::new(argv)),
    })
}

pub async fn ensure_daemon_running() -> anyhow::Result<()> {
    // Autostart, the stale-socket probe, and pid-file recovery are all
    // Unix-socket, same-machine lifecycle. A `tcp://` / `cmd://` client address
    // manages its own reachability (a loopback TCP daemon the user started, or
    // an SSH/container process the `cmd://` spawns), so skip local lifecycle
    // management entirely and let the connect attempt speak for itself.
    if !matches!(
        resolve_daemon_addr()?,
        mxr_transport::TransportAddr::Unix(_)
    ) {
        return Ok(());
    }
    let sock_path = resolve_daemon_socket()?;

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
    let sock_path = resolve_daemon_socket()?;
    restart_daemon_process(
        &sock_path,
        None,
        "Restarting daemon to match the current binary...",
    )
    .await
}

pub async fn ensure_daemon_supports_tui() -> anyhow::Result<()> {
    let sock_path = resolve_daemon_socket()?;
    let snapshot =
        match fetch_daemon_status_snapshot_from_path(&sock_path, STATUS_REQUEST_TIMEOUT).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                // The status query can still time out if the daemon is
                // pathologically busy. A live daemon that answers ping is
                // protocol-compatible enough to launch into — don't block the
                // TUI on a transient status stall (mirrors the daemon-match
                // path's ping fallback).
                if daemon_responds_to_ping(&sock_path, Duration::from_secs(2)).await {
                    eprintln!(
                    "Daemon status check failed ({error}); daemon is responsive, launching anyway."
                );
                    return Ok(());
                }
                return Err(error);
            }
        };

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
        .map_or(0, |duration| duration.as_secs());
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
    let connector = UnixConnector::new(path.to_path_buf());
    for attempt in 0..SOCKET_PROBE_ATTEMPTS {
        // Connect-and-drop liveness probe, dialed through the transport
        // `Connector` (no raw `UnixStream` in daemon code outside the UDS
        // transport). The connect error's `io::ErrorKind` still drives retry.
        match connector.connect().await {
            Ok(_io) => return true,
            Err(TransportError::Connect { source, .. })
                if should_retry_socket_probe(&source) && attempt + 1 < SOCKET_PROBE_ATTEMPTS =>
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

/// Clear the pid file only if it still names this process. A successor
/// daemon overwrites the pid file when it starts; deleting it then would
/// leave the successor undiscoverable by `live_daemon_pid`.
fn clear_daemon_pid_file_if_owned() {
    if read_daemon_pid_file() == Some(std::process::id()) {
        clear_daemon_pid_file();
    } else {
        tracing::info!("leaving daemon pid file untouched: it no longer names this process");
    }
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
    let current_instance = mxr_config::app_instance_name();
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
        if arg1 != "daemon" {
            continue;
        }

        let args = parts.collect::<Vec<_>>();
        if args != ["--instance", current_instance.as_str()] {
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
        if process_has_exited(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    process_has_exited(pid)
}

/// `kill(pid, 0)` reports zombies as alive, so a plain liveness probe can
/// never observe the exit of a daemon whose parent doesn't reap it (the TUI
/// spawns daemons and never waits on them). Treat zombies as exited, and
/// opportunistically reap the process when it is our own child.
fn process_has_exited(pid: u32) -> bool {
    {
        use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};

        // Only reaps when `pid` is our child; ECHILD otherwise, which we
        // ignore and fall through to the generic probes.
        if matches!(
            waitpid(Pid::from_raw(pid as i32), Some(WaitPidFlag::WNOHANG)),
            Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..))
        ) {
            return true;
        }
    }

    if !process_is_alive(pid) {
        return true;
    }
    process_is_zombie(pid)
}

/// There is no portable zombie probe (macOS has no procfs); shelling out to
/// `ps` matches the existing ps-based daemon discovery fallback in this file.
fn process_is_zombie(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-o", "state=", "-p", &pid.to_string()])
        .output()
        .ok()
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
        })
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
                "Broken daemon pid {daemon_pid} did not exit cleanly. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`."
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
    let snapshot = match fetch_daemon_status_snapshot_from_path(sock_path, STATUS_REQUEST_TIMEOUT)
        .await
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            if daemon_responds_to_ping(sock_path, Duration::from_secs(2)).await {
                eprintln!(
                    "Daemon status check failed ({error}); daemon is still responsive, leaving it running."
                );
                return Ok(());
            }
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
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected daemon status response"),
    }
}

async fn restart_daemon_process(
    sock_path: &std::path::Path,
    daemon_pid: Option<u32>,
    message: &str,
) -> anyhow::Result<()> {
    eprint!("{message}");

    // Capture the old daemon's pid before shutdown clears the pid file. The
    // socket stops accepting the moment the old daemon leaves its accept
    // loop, but the process keeps draining connections and background tasks
    // for up to ~10s after that — and still holds the search-index lock.
    // Spawning the successor inside that window loses a race: the old
    // daemon's exit cleanup runs after the successor binds the socket, and
    // an unguarded cleanup deletes the successor's socket, orphaning it
    // (alive and syncing, but unreachable by every client).
    let old_pid = daemon_pid.or_else(read_daemon_pid_file);

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
                "Existing daemon{pid_note} did not exit cleanly. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`."
            );
        }
        SocketState::Stale => {
            let _ = std::fs::remove_file(sock_path);
            clear_daemon_pid_file();
        }
        SocketState::Missing => {}
    }

    if let Some(pid) = old_pid {
        if !wait_for_process_exit(pid, DAEMON_EXIT_DRAIN_TIMEOUT).await {
            eprintln!(" failed.");
            anyhow::bail!(
                "Existing daemon (pid {pid}) is still shutting down. Wait a few seconds and rerun, or check `mxr logs --level error`."
            );
        }
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

    // Capture the daemon PID before it has a chance to clear its
    // pid file. We use this to wait for the actual process to exit
    // (not just the socket to disappear) so callers like `reset
    // --hard` can rely on the daemon being fully gone before they
    // start mutating shared state.
    let pid_before_shutdown = read_daemon_pid_file();

    let _ = request_shutdown_to(sock_path).await;

    // Phase 1: poll the socket until it's gone. The daemon removes
    // the socket file at the very end of its shutdown sequence, so
    // socket-gone is a strong signal that cleanup finished.
    let deadline = std::time::Instant::now() + wait_timeout;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = inspect_socket_state(sock_path).await;
        if !matches!(state, SocketState::Reachable) {
            break;
        }
    }

    // Phase 2: even after the socket is gone, the process may still
    // be in tokio runtime drop / final flushes. The shutdown sequence
    // can take up to drain (5s) + runtime tasks (5s) in pathological
    // cases. Wait an additional generous window for the process
    // itself to exit. This is what fixes the `reset_cli` flake:
    // previously the CLI returned while the daemon was mid-shutdown,
    // the test then asserted process-gone, and lost the race.
    if let Some(pid) = pid_before_shutdown {
        let process_deadline = std::time::Instant::now() + Duration::from_secs(12);
        while std::time::Instant::now() < process_deadline {
            if !process_is_alive(pid) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    Ok(inspect_socket_state(sock_path).await)
}

async fn request_shutdown() -> anyhow::Result<()> {
    request_shutdown_to(&resolve_daemon_socket()?).await
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

#[cfg(test)]
async fn daemon_responds_to_status(sock_path: &std::path::Path, timeout: Duration) -> bool {
    fetch_daemon_status_snapshot_from_path(sock_path, timeout)
        .await
        .is_ok()
}

async fn daemon_responds_to_ping(sock_path: &std::path::Path, timeout: Duration) -> bool {
    let response = tokio::time::timeout(timeout, async {
        let mut client = IpcClient::connect_to(sock_path).await?;
        client.request(Request::Ping).await
    })
    .await;

    matches!(
        response,
        Ok(Ok(Response::Ok {
            data: ResponseData::Pong,
        }))
    )
}

fn spawn_startup_maintenance(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = run_startup_maintenance(state).await {
            tracing::warn!("startup maintenance failed: {error}");
        }
    })
}

async fn run_startup_maintenance(state: Arc<AppState>) -> anyhow::Result<()> {
    // Crash-safe drafts: any draft in `'sending'` whose most-recent
    // activity is older than 1 hour is presumed orphaned (daemon died
    // mid-send). Reset back to `'draft'` so the user can retry. The
    // 1-hour cutoff is generous — a real send rarely takes >30s, but a
    // brief OAuth refresh or large attachment could.
    let orphan_cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
    if let Ok(orphans) = state
        .store
        .list_orphaned_sending_drafts(orphan_cutoff)
        .await
    {
        for draft_id in &orphans {
            if let Err(e) = state.store.reset_orphaned_draft(draft_id).await {
                tracing::warn!(
                    draft_id = %draft_id,
                    "startup: failed to reset orphaned sending draft: {e}"
                );
            }
        }
        if !orphans.is_empty() {
            tracing::info!(
                recovered = orphans.len(),
                "startup: reset orphaned 'sending' drafts back to 'draft' for retry"
            );
        }
    }

    // Lost scheduled sends: a scheduled-send attempt whose outcome was
    // never recorded means the daemon died between clearing `send_at` and
    // the send resolving — the message may or may not have gone out.
    // Surface each so the user can check and resend, then mark it resolved
    // (`interrupted`) so it isn't reported again on the next startup.
    if let Ok(lost) = state.store.list_lost_scheduled_sends().await {
        for entry in &lost {
            tracing::warn!(
                draft_id = %entry.draft_id,
                "startup: scheduled send may not have completed before a daemon restart; \
                 verify and resend if needed"
            );
            let _ = state
                .store
                .insert_event(
                    "warn",
                    "scheduled_send",
                    &format!(
                        "Scheduled send for draft {} may not have completed (daemon restarted mid-send). Verify and resend if needed.",
                        entry.draft_id
                    ),
                    None,
                    Some(&format!("draft_id={}", entry.draft_id)),
                )
                .await;
            let _ = state
                .store
                .record_scheduled_send_outcome(&entry.draft_id, entry.attempted_at, "interrupted")
                .await;
        }
        if !lost.is_empty() {
            tracing::warn!(
                count = lost.len(),
                "startup: surfaced scheduled sends that may not have completed before a restart"
            );
        }
    }

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
    let mut command = std::process::Command::new(exe);
    command
        .arg("daemon")
        .arg("--instance")
        .arg(mxr_config::app_instance_name())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    detach_daemon_child(&mut command);
    let mut child = command.spawn()?;

    for i in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100 * (i + 1))).await;
        if daemon_responds_to_ping(sock_path, Duration::from_millis(250)).await {
            eprintln!(" ready.");
            return Ok(());
        }
        // A dead child will never answer; fail fast with the log tail
        // instead of pinging into the void for the rest of the window.
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
    }

    // Past the normal window. Startup legitimately takes minutes after an
    // upgrade — schema migrations and WAL recovery on a multi-GB store, or
    // a search-index rebuild — and those run before the socket binds.
    // While the process is alive, keep waiting instead of declaring
    // failure and tempting the user (or a wrapper script) into spawning a
    // second daemon against a half-migrated store.
    if matches!(child.try_wait(), Ok(None)) {
        eprintln!();
        eprintln!(
            "Daemon is still starting — this can take a few minutes after an upgrade (database migration, search-index rebuild)."
        );
        let patient_deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        let mut next_note = tokio::time::Instant::now() + Duration::from_secs(30);
        while tokio::time::Instant::now() < patient_deadline {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if daemon_responds_to_ping(sock_path, Duration::from_millis(250)).await {
                eprintln!("Daemon ready.");
                return Ok(());
            }
            if matches!(child.try_wait(), Ok(Some(_))) {
                break;
            }
            if tokio::time::Instant::now() >= next_note {
                eprintln!("Still starting...");
                next_note = tokio::time::Instant::now() + Duration::from_secs(30);
            }
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

#[cfg(unix)]
fn detach_daemon_child(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // Autostarted daemons must outlive short-lived CLI invocations,
    // including shells that send SIGHUP when the command exits.
    command.process_group(0);
}

#[cfg(not(unix))]
fn detach_daemon_child(_command: &mut std::process::Command) {}

#[cfg(test)]
mod tests {
    use super::{
        classify_health, current_build_id, daemon_requires_restart, daemon_responds_to_ping,
        daemon_responds_to_status, is_index_lock_error, request_shutdown_to,
        spawn_startup_maintenance,
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
    use tokio::net::UnixListener;
    use tokio_util::codec::Framed;

    #[tokio::test]
    async fn wait_for_process_exit_observes_an_unreaped_child() {
        use super::wait_for_process_exit;

        // Spawn a child that exits immediately and deliberately do not
        // reap it: it stays a zombie, which `kill(pid, 0)` reports as
        // alive. The exit probe must still observe it as exited.
        let child = std::process::Command::new("true")
            .spawn()
            .expect("spawn child");
        let pid = child.id();
        // Drop the handle without wait() so nothing reaps the zombie
        // before the probe runs.
        std::mem::forget(child);

        assert!(
            wait_for_process_exit(pid, Duration::from_secs(5)).await,
            "zombie child must count as exited"
        );
    }

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
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
            keywords: std::collections::BTreeSet::new(),
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
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
            keywords: std::collections::BTreeSet::new(),
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
                    reply_later: false,
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
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
            keywords: std::collections::BTreeSet::new(),
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
                source: ::mxr_protocol::ClientKind::default(),
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
                        source: ::mxr_protocol::ClientKind::default(),
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
                                feature_health: None,
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
    async fn daemon_ping_probe_does_not_need_database_status() {
        let socket_path = std::path::PathBuf::from(format!(
            "/tmp/mxr-ping-ready-{}.sock",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("bind ping socket");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut framed = Framed::new(stream, IpcCodec::new());
            if let Some(Ok(message)) = framed.next().await {
                framed
                    .send(IpcMessage {
                        id: message.id,
                        source: ::mxr_protocol::ClientKind::default(),
                        payload: IpcPayload::Response(Response::Ok {
                            data: ResponseData::Pong,
                        }),
                    })
                    .await
                    .expect("send pong");
            }
        });

        assert!(daemon_responds_to_ping(&socket_path, Duration::from_secs(1)).await);
        server.await.expect("join ping server");
        let _ = std::fs::remove_file(&socket_path);
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
                            source: ::mxr_protocol::ClientKind::default(),
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
}
