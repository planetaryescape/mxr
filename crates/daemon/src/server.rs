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

/// Resolve the daemon socket for a Unix-only client surface (the standalone
/// `mxr web` bridge). Like the TUI and MCP, it rejects `tcp://` / `cmd://` with
/// a clear message rather than silently dialing the default socket — support
/// for those schemes can follow demand.
pub(crate) fn resolve_daemon_socket_unix_only() -> anyhow::Result<PathBuf> {
    match resolve_daemon_addr()? {
        mxr_transport::TransportAddr::Unix(path) => Ok(path),
        mxr_transport::TransportAddr::Tcp(_) | mxr_transport::TransportAddr::Cmd(_) => {
            anyhow::bail!(
                "`mxr web` supports only unix:// daemon addresses; {} is tcp:// or cmd:// — use the mxr CLI for those",
                mxr_transport::DAEMON_ADDR_ENV
            )
        }
    }
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

    let socket_state = inspect_socket_state(&sock_path).await;
    if matches!(socket_state, SocketState::Reachable) {
        return ensure_current_daemon_matches_binary(&sock_path).await;
    }

    match recover_from_broken_socket(&sock_path).await {
        BrokenSocketRecovery::SocketAlive => {
            return ensure_current_daemon_matches_binary(&sock_path).await
        }
        BrokenSocketRecovery::RestartDaemon(daemon) => {
            return recover_broken_running_daemon(
                &sock_path,
                &daemon,
                "Restarting daemon to recover from a missing IPC socket...",
            )
            .await
        }
        BrokenSocketRecovery::StartFresh => {
            if matches!(socket_state, SocketState::Stale) {
                let _ = std::fs::remove_file(&sock_path);
                clear_daemon_pid_file();
            }
        }
    }

    spawn_daemon_process(&sock_path, "Starting daemon...").await
}

/// Work out how to recover a socket that just failed its liveness probe.
///
/// Asking the socket who owns it is the one authoritative identity check
/// available — the daemon reports its own pid — so it runs first, and it also
/// closes the race where a daemon comes up between the probe and this
/// decision.
async fn recover_from_broken_socket(sock_path: &Path) -> BrokenSocketRecovery {
    let socket_owner_pid =
        fetch_daemon_status_snapshot_from_path(sock_path, STATUS_REQUEST_TIMEOUT)
            .await
            .ok()
            .and_then(|snapshot| snapshot.daemon_pid);
    classify_broken_socket(live_daemon_pid, socket_owner_pid)
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

/// One word for what the daemon is: the header line of `mxr status`, and what
/// `mxr doctor --check` turns into an exit code.
///
/// `degraded` is a reading the daemon could not take, not a reading of zero —
/// it empties `sync_statuses`, so without it the unhealthy-account check has
/// nothing left to look at and the same status object carries
/// `degraded: true` beside `health_class: "healthy"`.
pub(crate) fn classify_health(
    sync_statuses: &[AccountSyncStatus],
    repair_required: bool,
    restart_required: bool,
    degraded: bool,
) -> DaemonHealthClass {
    if restart_required {
        DaemonHealthClass::RestartRequired
    } else if repair_required {
        DaemonHealthClass::RepairRequired
    } else if degraded || sync_statuses.iter().any(|status| !status.healthy) {
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
    write_daemon_identity_file();
    Ok(())
}

fn clear_daemon_pid_file() {
    let _ = std::fs::remove_file(daemon_pid_file_path());
    let _ = std::fs::remove_file(daemon_identity_file_path());
}

fn read_daemon_pid_file() -> Option<u32> {
    std::fs::read_to_string(daemon_pid_file_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn daemon_identity_file_path() -> PathBuf {
    AppState::data_dir().join("daemon.identity")
}

/// What the daemon of this profile is, recorded next to its pid.
///
/// The pid file says *which number*; on its own that is not identity, because
/// the OS recycles pids and this profile's daemon may have exited without
/// clearing the file. `started_at` is what makes the record exact: it belongs
/// to one process for that process's whole life and to no successor that
/// inherits the number. The rest is provenance — it catches a data directory
/// copied or shared between profiles.
///
/// It lives beside the pid file rather than inside it so a binary older than
/// this record still reads `daemon.pid` as the plain number it has always
/// been. A missing record is normal (daemons before this change wrote none)
/// and drops the caller back to a weaker check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct DaemonIdentity {
    pid: u32,
    started_at: String,
    /// Which binary is serving this profile. Nothing reads it — it is here for
    /// the person reading a bug report, who otherwise cannot tell which of
    /// several mxr builds on the machine owns the daemon.
    exe: String,
    instance: String,
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl DaemonIdentity {
    fn of_this_process() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        Some(Self {
            pid: std::process::id(),
            started_at: crate::process_probe::start_time(std::process::id())?,
            exe: std::fs::canonicalize(&exe)
                .unwrap_or(exe)
                .display()
                .to_string(),
            instance: mxr_config::app_instance_name(),
            config_dir: mxr_config::config_dir(),
            data_dir: mxr_config::data_dir(),
        })
    }

    /// Whether this record was written by a daemon of the profile the current
    /// process resolves.
    fn describes_our_profile(&self) -> bool {
        self.instance == mxr_config::app_instance_name()
            && self.config_dir == mxr_config::config_dir()
            && self.data_dir == mxr_config::data_dir()
    }
}

fn write_daemon_identity_file() {
    let Some(identity) = DaemonIdentity::of_this_process() else {
        tracing::info!("could not record daemon identity; pid-file adoption falls back to argv");
        return;
    };
    write_identity_record(&identity);
}

/// Persist an identity record, saying so when it does not land.
///
/// A record that fails to write is not a small loss: the next run drops back
/// to the argv check, which cannot tell this daemon from a recycled pid
/// running some other mxr daemon. Failing silently leaves that degradation
/// invisible in the one place — the log — where a kill-safety question gets
/// answered after the fact.
fn write_identity_record(identity: &DaemonIdentity) {
    let path = daemon_identity_file_path();
    match toml::to_string(identity) {
        Ok(rendered) => {
            if let Err(error) = std::fs::write(&path, rendered) {
                tracing::warn!(
                    %error,
                    path = %path.display(),
                    "could not write the daemon identity record; pid-file adoption falls back to argv"
                );
            }
        }
        Err(error) => tracing::warn!(
            %error,
            path = %path.display(),
            "could not render the daemon identity record"
        ),
    }
}

fn read_daemon_identity_file() -> Option<DaemonIdentity> {
    toml::from_str(&std::fs::read_to_string(daemon_identity_file_path()).ok()?).ok()
}

fn process_is_alive(pid: u32) -> bool {
    match kill(Pid::from_raw(pid as i32), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => false,
    }
}

/// How a pid earned the right to be signalled, so the same evidence can be
/// re-checked when the signal is about to go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonEvidence {
    /// Named by this profile's pid file, which only this profile's daemon
    /// writes. The file establishes the profile, so re-checking it must not
    /// demand a matching environment: a daemon started by hand from another
    /// shell is still ours. Nor a matching `--instance`, which verification
    /// weighs but this cannot: the re-check runs where the start times agree
    /// or are both missing, and a wrong answer here goes straight to clearing
    /// the lifecycle files with no scan left to rescue it.
    PidFile,
    /// Found by scanning `ps`. Nothing on the file system vouched for it —
    /// identity came from the executable and the process environment, so both
    /// are what get re-checked.
    ProcessScan,
}

/// A pid together with the evidence that it was this profile's daemon at the
/// moment it was verified.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifiedDaemon {
    pid: u32,
    evidence: DaemonEvidence,
    /// Start time as observed during verification. Nothing else distinguishes
    /// a process from a successor that inherited its number.
    started_at: Option<String>,
}

impl VerifiedDaemon {
    fn verify(pid: u32, evidence: DaemonEvidence) -> Self {
        Self {
            pid,
            evidence,
            started_at: crate::process_probe::start_time(pid),
        }
    }

    /// Whether this pid is still the process that was verified.
    ///
    /// Time passes between deciding to restart a daemon and signalling it —
    /// the status probe in between can take seconds, and asking a wedged
    /// daemon for status is itself a reason for it to exit. If it exits and
    /// the OS hands its number to something else, the signal lands on a
    /// stranger. Re-running the evidence with the signal in hand closes that
    /// window, for the cost of one `ps` call on a path that is already
    /// restarting a daemon.
    fn is_unchanged(&self) -> bool {
        Self::still_the_same_process(
            self.started_at.as_deref(),
            self.evidence,
            crate::process_probe::start_time(self.pid).as_deref(),
            || {
                crate::process_probe::command_words(self.pid)
                    .map(|words| command_runs_a_daemon(&words))
            },
            || process_runs_our_profile(self.pid),
        )
    }

    /// The re-check itself, over observations rather than a live pid.
    fn still_the_same_process(
        started_at: Option<&str>,
        evidence: DaemonEvidence,
        observed_start_time: Option<&str>,
        runs_daemon_argv: impl FnOnce() -> Option<bool>,
        runs_our_profile: impl FnOnce() -> bool,
    ) -> bool {
        match (started_at, observed_start_time) {
            // The ordinary case: a recycled pid has a different start time.
            (Some(before), Some(now)) if before != now => return false,
            // The process is gone, or `ps` stopped answering about it. Either
            // way there is nothing left here that we recognise.
            (Some(_), None) => return false,
            _ => {}
        }
        // Same second, or no start time either time: fall back on the rest of
        // the evidence rather than read an indistinguishable timestamp as
        // proof.
        //
        // Only a definite "that is not a daemon" rejects. A `ps` that will not
        // answer says nothing, and reading silence as a rejection here would
        // strand a live daemon exactly as it did at verification time: the pid
        // file gets cleared, a successor spawns, and it dies on the
        // search-index lock. Both phases treat an unreadable argv the same way.
        if runs_daemon_argv() == Some(false) {
            return false;
        }
        match evidence {
            // The profile is what identified a scanned pid in the first place,
            // so unlike the argv it has to hold up again — an environment we
            // cannot read is not an identity we can act on.
            DaemonEvidence::PidFile => true,
            DaemonEvidence::ProcessScan => runs_our_profile(),
        }
    }
}

/// What to do about a daemon that looked broken.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BrokenSocketRecovery {
    /// The socket answered after all — whatever the earlier probe saw was
    /// transient. Leave every process alone.
    SocketAlive,
    /// This profile's daemon, alive behind a socket it can no longer serve.
    /// Restarting it is what rebuilds the socket.
    RestartDaemon(VerifiedDaemon),
    /// Nothing provably ours is running. Start a fresh daemon rather than
    /// signalling a process we cannot identify.
    StartFresh,
}

/// Decide what a broken socket means, given who — if anyone — answers on the
/// socket right now, and a way to look for this profile's daemon.
///
/// A live answer wins outright, whatever pid it names: a daemon serving our
/// socket is not something to kill, and a mismatched pid means the candidate
/// was never ours. Only when nobody answers may a candidate be restarted, and
/// only candidates whose identity we can prove ever reach here.
///
/// Looking for the candidate is a closure rather than a value because looking
/// is not free of consequence: it clears a stale pid file and stamps an
/// adopted one. A daemon that turns out to be alive and well must have none of
/// that done underneath it, and taking the closure is what makes that
/// impossible rather than merely true today.
fn classify_broken_socket(
    candidate: impl FnOnce() -> Option<VerifiedDaemon>,
    socket_owner_pid: Option<u32>,
) -> BrokenSocketRecovery {
    if socket_owner_pid.is_some() {
        return BrokenSocketRecovery::SocketAlive;
    }
    match candidate() {
        Some(daemon) => BrokenSocketRecovery::RestartDaemon(daemon),
        None => BrokenSocketRecovery::StartFresh,
    }
}

/// This profile's running daemon, if one can be identified. Only pids that
/// survive identity verification come back: callers signal what they get.
fn live_daemon_pid() -> Option<VerifiedDaemon> {
    if let Some(pid) = read_daemon_pid_file() {
        if process_is_alive(pid) && pid_file_still_names_our_daemon(pid) {
            return Some(VerifiedDaemon::verify(pid, DaemonEvidence::PidFile));
        }
        clear_daemon_pid_file();
    }

    let pid = fallback_live_daemon_pid_without_pid_file()?;
    let adopted = VerifiedDaemon::verify(pid, DaemonEvidence::ProcessScan);
    let _ = std::fs::write(daemon_pid_file_path(), pid.to_string());
    // Restore the record alongside the pid file. An older record left in place
    // would name a pid the file no longer does, which the next run rightly
    // reads as stale.
    adopt_daemon_identity_file(&adopted);
    Some(adopted)
}

/// Record an adopted daemon so the next run can check its pid exactly instead
/// of falling back to the argv check. Everything but the start time follows
/// from having just verified it against this profile.
fn adopt_daemon_identity_file(adopted: &VerifiedDaemon) {
    let Some(started_at) = adopted.started_at.clone() else {
        let _ = std::fs::remove_file(daemon_identity_file_path());
        return;
    };
    let identity = DaemonIdentity {
        pid: adopted.pid,
        started_at,
        exe: std::env::current_exe()
            .map(|exe| {
                std::fs::canonicalize(&exe)
                    .unwrap_or(exe)
                    .display()
                    .to_string()
            })
            .unwrap_or_default(),
        instance: mxr_config::app_instance_name(),
        config_dir: mxr_config::config_dir(),
        data_dir: mxr_config::data_dir(),
    };
    write_identity_record(&identity);
}

/// Whether the pid file's number still refers to the daemon that wrote it.
///
/// The file lives in this profile's data directory and only this profile's
/// daemon writes it, so the profile is already established — the one thing
/// left to rule out is the daemon having exited without clearing the file and
/// the OS having handed its number to something else.
///
/// The identity record answers that exactly, by start time. Without one (a
/// daemon older than the record, or a `ps` that would not report a start time)
/// the fallback is the weaker "is this still a process running the `daemon`
/// subcommand" — deliberately indifferent to flags and to which mxr binary it
/// is, because `mxr daemon --foreground`, `--no-bridge` and `--bridge-port`
/// are all daemons worth recovering, as is one left over from another build.
///
/// Anything unreadable counts as "still ours": deleting a live daemon's pid
/// file strands it, and the CLI then spawns a successor that dies on the
/// search-index lock.
fn pid_file_still_names_our_daemon(pid: u32) -> bool {
    let record = read_daemon_identity_file();
    let record = record
        .as_ref()
        .map(|identity| (identity, identity.describes_our_profile()));
    pid_file_verdict(
        pid,
        record,
        || crate::process_probe::start_time(pid),
        || {
            crate::process_probe::command_words(pid).map(|words| {
                command_runs_a_daemon(&words)
                    && !command_names_another_instance(&words, &mxr_config::app_instance_name())
            })
        },
    )
}

/// Decide whether a pid file's number still refers to the daemon that wrote it,
/// given what can be observed about that pid.
///
/// The probes are closures so the cheap answer short-circuits the expensive
/// one, and so the decision itself can be exercised without a process to point
/// at.
fn pid_file_verdict(
    pid: u32,
    record: Option<(&DaemonIdentity, bool)>,
    observed_start_time: impl FnOnce() -> Option<String>,
    runs_daemon_argv: impl FnOnce() -> Option<bool>,
) -> bool {
    match record {
        // A record that names a different pid, or another profile, contradicts
        // the pid file rather than saying nothing about it: the two are written
        // and cleared together, so disagreement means one of them is stale.
        // Falling back to the argv check here would let a neighbouring
        // profile's daemon inherit this profile's pid file and be signalled on
        // its behalf.
        Some((identity, profile_matches)) if identity.pid != pid || !profile_matches => {
            tracing::info!(
                pid,
                recorded_pid = identity.pid,
                profile_matches,
                "daemon identity record contradicts the pid file; treating it as stale"
            );
            false
        }
        Some((identity, _)) => match observed_start_time() {
            Some(started_at) => started_at == identity.started_at,
            // No start time to compare is not evidence against the record.
            None => true,
        },
        // No record at all: a daemon older than the record, or one whose write
        // failed. All that is left is the weak check — and an argv nobody will
        // report is not grounds for stranding a live daemon.
        None => runs_daemon_argv().unwrap_or(true),
    }
}

/// Whether a command line runs mxr's `daemon` subcommand.
///
/// Flags are deliberately not constrained: `--foreground`, `--no-bridge` and
/// `--bridge-port` are all daemons worth recovering, and requiring the shape
/// autostart happens to use (`<exe> daemon --instance <name>`) rejected every
/// daemon a person started by hand.
fn command_runs_a_daemon(words: &[String]) -> bool {
    words.get(1).is_some_and(|word| word == "daemon")
}

/// Whether a `daemon` command line spells out an instance name that is not
/// ours.
///
/// Evidence, not proof: `--instance` is a marker autostart passes so a process
/// can be recognised in `ps` (`cli/mod.rs`), and the daemon discards it — the
/// profile comes from the inherited environment. So a name that disagrees does
/// not *prove* a different profile; it is the one thing on a command line that
/// points that way, and on the pid-file path with no identity record there is
/// nothing else to weigh at all.
///
/// Used only where a wrong answer has somewhere to fall back to: it rules a
/// pid out of *verification*, after which the `ps` scan can still adopt the
/// daemon by canonical executable and full environment. The pre-signal
/// re-check does not consult it — see `DaemonEvidence::PidFile`.
///
/// Both spellings clap accepts are read, because a name that goes unnoticed
/// here misses toward signalling a neighbour. The scan path
/// (`daemon_command_exe`) reads only the spaced form on purpose: its default
/// is the opposite one — distrust unless proven — so a spelling it misses
/// declines to adopt, which is the safe answer there.
///
/// An instance name containing whitespace is not compared at all: `ps` output
/// is split on whitespace, so the name cannot be reassembled, and half of it
/// would compare unequal to all of it. (Truncation would do the same, which is
/// why both `ps` probes pass `-ww`.)
fn command_names_another_instance(words: &[String], instance: &str) -> bool {
    if instance.split_whitespace().count() != 1 {
        return false;
    }
    words.iter().enumerate().any(|(index, word)| {
        let named = match word.strip_prefix("--instance=") {
            Some(named) => Some(named),
            // A trailing `--instance` names nobody, and silence is not
            // evidence.
            None if word == "--instance" => words.get(index + 1).map(String::as_str),
            None => None,
        };
        named.is_some_and(|named| named != instance)
    })
}

/// Find this profile's daemon when the pid file is gone, by scanning `ps`.
///
/// `ps` reports command lines and nothing about which mxr profile a process
/// serves, so a command-line match is not identity: every mxr checkout builds
/// a binary called `mxr`, and `mxr demo` pins one instance name across every
/// profile. Adopting on that match alone had two checkouts (or two demo
/// profiles) restarting and killing each other's daemons. Every candidate is
/// therefore filtered down to the ones running our exact executable *in our
/// profile* before uniqueness is considered, and an unverifiable match counts
/// as no daemon at all.
fn fallback_live_daemon_pid_without_pid_file() -> Option<u32> {
    let current_exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    let current_instance = mxr_config::app_instance_name();

    let candidates = scan_daemon_processes(
        &crate::process_probe::all_command_lines()?,
        std::process::id(),
        &current_exe,
        &current_instance,
    );

    // Profile first, then uniqueness: two daemons of the same build in
    // different profiles would otherwise make each other unadoptable.
    let ours: Vec<u32> = candidates
        .into_iter()
        .filter(|pid| process_runs_our_profile(*pid))
        .collect();

    match ours.as_slice() {
        [pid] => Some(*pid),
        [] => None,
        more => {
            // Two live daemons for one profile is already broken; restarting
            // an arbitrary one of them would just pick a side.
            tracing::warn!(
                pids = ?more,
                "found several `{current_instance}` daemons for this profile; not adopting any"
            );
            None
        }
    }
}

/// The pids running *our* executable as `<exe> daemon [...]`.
///
/// The executable is matched by file, not by file name: two checkouts both
/// build a binary called `mxr`, and only the path tells them apart. Flags are
/// not constrained beyond `--instance`, so a daemon someone started by hand
/// with `--foreground` is still recoverable; the profile check that follows is
/// what establishes identity.
fn scan_daemon_processes(
    command_lines: &[(u32, String)],
    our_pid: u32,
    our_exe: &Path,
    instance: &str,
) -> Vec<u32> {
    command_lines
        .iter()
        .filter(|(pid, command)| {
            *pid != our_pid
                && daemon_command_exe(command, instance)
                    .is_some_and(|exe| crate::process_probe::exe_is(exe, our_exe))
        })
        .map(|(pid, _)| *pid)
        .collect()
}

/// The executable named by a command line, if that command line runs the
/// `daemon` subcommand and does not name a *different* instance.
///
/// A daemon started without `--instance` takes its instance from the
/// environment, which the profile check compares in full — so the flag being
/// absent is not a reason to skip the process.
fn daemon_command_exe<'a>(command: &'a str, instance: &str) -> Option<&'a str> {
    let mut words = command.split_whitespace();
    let exe = words.next()?;
    if words.next() != Some("daemon") {
        return None;
    }
    let mut words = words.peekable();
    while let Some(word) = words.next() {
        if word == "--instance" && words.peek() != Some(&instance) {
            return None;
        }
    }
    Some(exe)
}

/// Environment variables that select which mxr profile a process resolves:
/// the explicit overrides, plus the OS directory roots the defaults are built
/// from. A daemon inherits its environment from the client that spawned it, so
/// a process serving our profile agrees with us on all of them.
const PROFILE_ENV_KEYS: &[&str] = &[
    "MXR_INSTANCE",
    "MXR_CONFIG_DIR",
    "MXR_DATA_DIR",
    "MXR_SOCKET_PATH",
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
];

/// Whether `pid` resolves the same mxr profile as this process.
///
/// Unproven counts as "no". This is the only thing standing between a `ps`
/// match and a `SIGTERM`, so a probe that cannot answer must not be read as
/// agreement.
fn process_runs_our_profile(pid: u32) -> bool {
    crate::process_probe::environment(pid)
        .is_some_and(|environment| environment_selects_our_profile(&environment))
}

/// Whether a `KEY=VALUE` environment resolves the same profile as ours.
fn environment_selects_our_profile(environment: &[String]) -> bool {
    PROFILE_ENV_KEYS.iter().all(|key| {
        crate::process_probe::environment_value(environment, key)
            == std::env::var(key).ok().as_deref()
    })
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
    daemon: &VerifiedDaemon,
    message: &str,
) -> anyhow::Result<()> {
    eprint!("{message}");
    let daemon_pid = daemon.pid;

    // Last check before the signal: the daemon may have exited while we were
    // deciding — quite possibly because of the status request we sent it — and
    // its number may already belong to something else.
    if daemon.is_unchanged() {
        send_signal(daemon_pid, Signal::SIGTERM)?;
        // The wait for a graceful exit is the widest recycle window of the
        // lot: the whole point of it is that the process is expected to go
        // away, and its number is free the moment it does. Re-check before
        // escalating rather than sending SIGKILL to whatever holds it now.
        if !wait_for_process_exit(daemon_pid, ORPHAN_DAEMON_EXIT_TIMEOUT).await {
            if daemon.is_unchanged() {
                send_signal(daemon_pid, Signal::SIGKILL)?;
                if !wait_for_process_exit(daemon_pid, Duration::from_secs(1)).await {
                    eprintln!(" failed.");
                    anyhow::bail!(
                        "Broken daemon pid {daemon_pid} did not exit cleanly. Useful next steps: `mxr status`, `mxr logs --level error`, `mxr daemon --foreground`."
                    );
                }
            } else {
                // Something still holds the number, but it is no longer the
                // daemon we signalled — so that daemon is gone, which is what
                // the wait was for. Not killing a stranger is worth the small
                // chance that the successor now races a daemon whose identity
                // we simply could not read.
                tracing::info!(
                    pid = daemon_pid,
                    "daemon pid changed identity while shutting down; not escalating to SIGKILL"
                );
            }
        }
    } else {
        tracing::info!(
            pid = daemon_pid,
            "daemon pid no longer identifies the process we verified; starting a fresh daemon instead of signalling it"
        );
    }

    match recovery_cleanup(
        daemon_pid,
        inspect_socket_state(sock_path).await,
        read_daemon_pid_file(),
    ) {
        RecoveryCleanup::AlreadyRecovered => {
            eprintln!(" already recovered.");
            // Somebody else's replacement, so it is not necessarily this
            // binary's — the same check every other live-socket path makes.
            ensure_current_daemon_matches_binary(sock_path).await
        }
        RecoveryCleanup::ClearAndSpawn => {
            clear_daemon_pid_file();
            let _ = std::fs::remove_file(sock_path);
            spawn_daemon_process(sock_path, "").await
        }
        RecoveryCleanup::SpawnWithoutClearing => {
            tracing::info!(
                pid = daemon_pid,
                "the pid file names a replacement daemon; leaving its lifecycle files alone"
            );
            // The successor spawned here is the one that loses the
            // search-index lock to that replacement, and it exits fast enough
            // that `spawn_daemon_process` can give up before the replacement's
            // socket starts answering again. Losing that race is the
            // replacement proving it is alive, so re-probe before passing the
            // verdict on: telling someone to go debug a daemon that is running
            // sends them after nothing.
            match spawn_daemon_process(sock_path, "").await {
                Ok(()) => Ok(()),
                Err(error) => {
                    if matches!(
                        inspect_socket_state(sock_path).await,
                        SocketState::Reachable
                    ) {
                        eprintln!("Another client's daemon is serving the socket; continuing.");
                        ensure_current_daemon_matches_binary(sock_path).await
                    } else {
                        Err(error)
                    }
                }
            }
        }
    }
}

/// What a recovery may do with the lifecycle files once the daemon it
/// signalled is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryCleanup {
    /// A daemon answers on the socket again. Nothing here is ours to clear,
    /// and nothing is left to start.
    AlreadyRecovered,
    /// The files still describe the daemon we signalled. Clear them and start
    /// a successor.
    ClearAndSpawn,
    /// Somebody else's replacement wrote the pid file. Start a successor —
    /// only one of the two can take the search-index lock, so the loser exits
    /// on its own — but leave files that are no longer ours.
    SpawnWithoutClearing,
}

/// Decide what a recovery does after its daemon is gone.
///
/// Two clients can observe the same broken socket and both come here for the
/// same pid. The first finishes: it clears the files and its replacement binds
/// the socket and writes a new pid file. The second is still inside the
/// graceful-exit wait, which is seconds wide — and used to clear those files
/// unconditionally, unlinking a live replacement's socket and leaving a daemon
/// that is alive, holds the index lock, and no client can reach.
///
/// The socket settles it when it answers. The pid file is the second reading
/// because a live daemon's socket probe can fail while it is busy, and a
/// replacement writes its pid only after it has bound: a number that is not
/// the one we signalled was written by a daemon that is already up.
///
/// A replacement that binds between this reading and the unlink is still
/// unprotected, but that window is two statements rather than the
/// graceful-exit wait, and the successor's own `bind` clears a stale socket
/// while holding the search-index lock — which is the authoritative guard.
fn recovery_cleanup(
    signalled_pid: u32,
    socket_state: SocketState,
    pid_file_now: Option<u32>,
) -> RecoveryCleanup {
    if matches!(socket_state, SocketState::Reachable) {
        return RecoveryCleanup::AlreadyRecovered;
    }
    match pid_file_now {
        Some(pid) if pid != signalled_pid => RecoveryCleanup::SpawnWithoutClearing,
        // Absent is the ordinary case: a daemon that exits gracefully clears
        // its own pid file.
        _ => RecoveryCleanup::ClearAndSpawn,
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

    if indexed_messages != total_messages as u64 {
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
    }

    if state.warm_lexical_search(true).await? {
        tracing::info!("Lexical search index warmed");
    }

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

    fn verified(pid: u32) -> super::VerifiedDaemon {
        super::VerifiedDaemon {
            pid,
            evidence: super::DaemonEvidence::PidFile,
            started_at: Some("Wed Aug 20 00:11:22 2026".to_string()),
        }
    }

    #[test]
    fn broken_socket_leaves_processes_alone_while_the_socket_answers() {
        use super::{classify_broken_socket, BrokenSocketRecovery};

        // A daemon answering on our socket is not something to restart,
        // whichever pid the earlier scan turned up.
        assert_eq!(
            classify_broken_socket(|| Some(verified(4242)), Some(4242)),
            BrokenSocketRecovery::SocketAlive
        );
        assert_eq!(
            classify_broken_socket(
                || panic!("a live socket must settle it without a lookup"),
                Some(99)
            ),
            BrokenSocketRecovery::SocketAlive
        );
        assert_eq!(
            classify_broken_socket(
                || panic!("a live socket must settle it without a lookup"),
                Some(99)
            ),
            BrokenSocketRecovery::SocketAlive
        );
    }

    #[test]
    fn broken_socket_starts_fresh_rather_than_signalling_an_unidentified_process() {
        use super::{classify_broken_socket, BrokenSocketRecovery};

        assert_eq!(
            classify_broken_socket(|| None, None),
            BrokenSocketRecovery::StartFresh
        );
        assert_eq!(
            classify_broken_socket(|| Some(verified(4242)), None),
            BrokenSocketRecovery::RestartDaemon(verified(4242))
        );
    }

    /// A real second file with the same name as this test binary, so the
    /// scan's executable check has something to actually discriminate: a path
    /// that does not exist fails `canonicalize` and would pass the test
    /// without the check ever running.
    fn decoy_binary(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let ours = std::env::current_exe().expect("current exe");
        let decoy = dir.path().join(ours.file_name().expect("exe file name"));
        if std::fs::hard_link(&ours, &decoy).is_err() {
            std::fs::copy(&ours, &decoy).expect("copy test binary");
        }
        std::fs::canonicalize(&decoy).expect("canonical decoy")
    }

    #[test]
    fn daemon_scan_matches_only_our_own_executable() {
        use super::scan_daemon_processes;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let ours = std::fs::canonicalize(std::env::current_exe().expect("current exe"))
            .expect("canonical exe");
        // Another checkout's build of the same binary name: a real file, so
        // the path comparison is what rejects it.
        let neighbour = decoy_binary(&temp);
        let ours_display = ours.display();
        let neighbour_display = neighbour.display();

        let command_lines = vec![
            (
                100,
                format!("{neighbour_display} daemon --instance mxr-dev"),
            ),
            (200, format!("{ours_display} daemon --instance mxr-dev")),
            (300, format!("{ours_display} daemon --instance mxr-demo")),
            (400, "mxr daemon --instance mxr-dev".to_string()),
            (500, format!("{ours_display} tui")),
        ];

        assert_eq!(
            scan_daemon_processes(&command_lines, 999, &ours, "mxr-dev"),
            vec![200],
            "only this binary, run as a daemon for this instance, may be adopted"
        );
    }

    #[test]
    fn daemon_scan_accepts_a_daemon_started_by_hand() {
        use super::scan_daemon_processes;

        let ours = std::fs::canonicalize(std::env::current_exe().expect("current exe"))
            .expect("canonical exe");
        let exe = ours.display();
        // `mxr daemon --foreground` and friends take their instance from the
        // environment. Skipping them would leave a hand-started daemon that
        // lost its pid file unadoptable — and the successor dies on the
        // search-index lock.
        let command_lines = vec![
            (100, format!("{exe} daemon --foreground")),
            (200, format!("{exe} daemon --no-bridge --bridge-port 7777")),
            (300, format!("{exe} daemon --instance mxr-demo")),
        ];

        assert_eq!(
            scan_daemon_processes(&command_lines, 999, &ours, "mxr-dev"),
            vec![100, 200],
            "flags other than a foreign --instance must not disqualify a daemon"
        );
    }

    #[test]
    fn daemon_scan_skips_our_own_process() {
        use super::scan_daemon_processes;

        let ours = std::fs::canonicalize(std::env::current_exe().expect("current exe"))
            .expect("canonical exe");
        let command_lines = vec![(777, format!("{} daemon --instance mxr-dev", ours.display()))];

        assert!(
            scan_daemon_processes(&command_lines, 777, &ours, "mxr-dev").is_empty(),
            "the scanning process must never adopt itself"
        );
    }

    #[test]
    fn daemon_command_exe_accepts_any_daemon_flags_but_not_a_foreign_instance() {
        use super::daemon_command_exe;

        assert_eq!(
            daemon_command_exe("/opt/bin/mxr daemon --instance mxr", "mxr"),
            Some("/opt/bin/mxr")
        );
        assert_eq!(
            daemon_command_exe("/opt/bin/mxr daemon --foreground", "mxr"),
            Some("/opt/bin/mxr")
        );
        assert_eq!(
            daemon_command_exe("/opt/bin/mxr daemon --instance mxr-demo", "mxr"),
            None
        );
        assert_eq!(daemon_command_exe("/usr/bin/vim notes.txt", "mxr"), None);
        assert_eq!(daemon_command_exe("", "mxr"), None);
    }

    #[test]
    fn profile_check_accepts_our_own_environment_and_rejects_a_neighbour() {
        use super::{environment_selects_our_profile, PROFILE_ENV_KEYS};

        let ours: Vec<String> = PROFILE_ENV_KEYS
            .iter()
            .filter_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| format!("{key}={value}"))
            })
            .collect();
        assert!(environment_selects_our_profile(&ours));

        // The case that bit: same binary, same instance, different profile.
        let mut neighbour: Vec<String> = ours
            .iter()
            .filter(|entry| !entry.starts_with("MXR_DATA_DIR="))
            .cloned()
            .collect();
        neighbour.push("MXR_DATA_DIR=/tmp/some-other-profile".to_string());
        assert!(
            !environment_selects_our_profile(&neighbour),
            "a daemon pointed at another data dir must not be adoptable"
        );

        // And the reverse: an environment missing something ours sets.
        assert!(
            !environment_selects_our_profile(&["HOME=/nowhere".to_string()]),
            "an environment that disagrees about HOME is another profile"
        );
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(ToString::to_string).collect()
    }

    fn identity_for(pid: u32, started_at: &str) -> super::DaemonIdentity {
        super::DaemonIdentity {
            pid,
            started_at: started_at.to_string(),
            exe: "/opt/homebrew/bin/mxr".to_string(),
            instance: "mxr".to_string(),
            config_dir: "/Users/bk/.config/mxr".into(),
            data_dir: "/Users/bk/.local/share/mxr".into(),
        }
    }

    #[test]
    fn an_identity_record_that_matches_confirms_the_pid_file() {
        use super::pid_file_verdict;

        let record = identity_for(4242, "Wed Aug 20 00:11:22 2026");

        assert!(
            pid_file_verdict(
                4242,
                Some((&record, true)),
                || Some("Wed Aug 20 00:11:22 2026".to_string()),
                || panic!("the argv fallback must not run when a record decides it"),
            ),
            "a record naming this pid, this profile and this start time is our daemon"
        );
    }

    #[test]
    fn an_identity_record_that_contradicts_the_pid_file_rejects_it() {
        use super::pid_file_verdict;

        // Both ways a record can contradict: a different pid, and a different
        // profile. Neither may fall through to "is it running the `daemon`
        // subcommand", which a neighbouring profile's daemon would pass.
        let other_pid = identity_for(99, "Wed Aug 20 00:11:22 2026");
        assert!(!pid_file_verdict(
            4242,
            Some((&other_pid, true)),
            || panic!("a contradicted record must not be second-guessed by a start time"),
            || Some(true),
        ));

        let other_profile = identity_for(4242, "Wed Aug 20 00:11:22 2026");
        assert!(!pid_file_verdict(
            4242,
            Some((&other_profile, false)),
            || panic!("a contradicted record must not be second-guessed by a start time"),
            || Some(true),
        ));

        // And a matching record whose start time moved on: the number was
        // recycled.
        let recycled = identity_for(4242, "Wed Aug 20 00:11:22 2026");
        assert!(!pid_file_verdict(
            4242,
            Some((&recycled, true)),
            || Some("Wed Aug 20 09:00:00 2026".to_string()),
            || Some(true),
        ));
    }

    #[test]
    fn without_a_record_the_argv_check_decides_and_silence_keeps_the_pid_file() {
        use super::pid_file_verdict;

        assert!(pid_file_verdict(4242, None, || None, || Some(true)));
        assert!(!pid_file_verdict(4242, None, || None, || Some(false)));
        assert!(
            pid_file_verdict(4242, None, || None, || None),
            "an argv nobody will report is not grounds for stranding a live daemon"
        );
    }

    #[test]
    fn a_pid_recycled_between_the_decision_and_the_signal_is_not_signalled() {
        use super::{DaemonEvidence, VerifiedDaemon};

        // The window this closes: the daemon exits while we are asking it for
        // status, and its number is handed to something else before the
        // SIGTERM goes out.
        assert!(!VerifiedDaemon::still_the_same_process(
            Some("Wed Aug 20 00:11:22 2026"),
            DaemonEvidence::PidFile,
            Some("Wed Aug 20 09:00:00 2026"),
            || panic!("a changed start time settles it on its own"),
            || panic!("a changed start time settles it on its own"),
        ));

        // Gone entirely, or `ps` stopped answering: nothing here to recognise.
        assert!(!VerifiedDaemon::still_the_same_process(
            Some("Wed Aug 20 00:11:22 2026"),
            DaemonEvidence::PidFile,
            None,
            || panic!("a vanished process settles it on its own"),
            || panic!("a vanished process settles it on its own"),
        ));
    }

    #[test]
    fn an_indistinguishable_start_time_falls_back_on_the_rest_of_the_evidence() {
        use super::{DaemonEvidence, VerifiedDaemon};

        // Same second — `lstart` has one-second resolution, so a fast recycle
        // is indistinguishable by timestamp alone.
        let same_second = |evidence, runs_daemon, profile| {
            VerifiedDaemon::still_the_same_process(
                Some("Wed Aug 20 00:11:22 2026"),
                evidence,
                Some("Wed Aug 20 00:11:22 2026"),
                || runs_daemon,
                || profile,
            )
        };

        assert!(same_second(DaemonEvidence::PidFile, Some(true), false));
        assert!(!same_second(DaemonEvidence::PidFile, Some(false), true));
        // A scan-sourced pid was only ever identified by its profile, so that
        // is what has to hold up again.
        assert!(same_second(DaemonEvidence::ProcessScan, Some(true), true));
        assert!(!same_second(DaemonEvidence::ProcessScan, Some(true), false));
    }

    #[test]
    fn an_unreadable_argv_is_read_the_same_way_by_both_phases() {
        use super::{pid_file_verdict, DaemonEvidence, VerifiedDaemon};

        // A `ps` that will not answer is not evidence of anything. The two
        // phases used to disagree about that: verification kept the daemon,
        // the pre-signal re-check threw it away — so one transient `ps`
        // failure stranded a live daemon behind a successor that then died on
        // the search-index lock.
        let record = identity_for(4242, "Wed Aug 20 00:11:22 2026");

        // Verification, with no record to decide it.
        assert!(
            pid_file_verdict(4242, None, || None, || None),
            "verification keeps a pid whose argv nobody will report"
        );

        // The pre-signal re-check, at the same silence.
        assert!(
            VerifiedDaemon::still_the_same_process(
                Some(&record.started_at),
                DaemonEvidence::PidFile,
                Some(&record.started_at),
                || None,
                || panic!("a pid-file pid does not have to re-prove its profile"),
            ),
            "the re-check must read that silence the same way"
        );

        // The profile is different: it is what identified a scanned pid, so an
        // environment we cannot read still rejects.
        assert!(!VerifiedDaemon::still_the_same_process(
            Some(&record.started_at),
            DaemonEvidence::ProcessScan,
            Some(&record.started_at),
            || None,
            || false,
        ));
    }

    #[test]
    fn a_late_recovery_does_not_clear_a_replacement_daemon_s_files() {
        use super::{recovery_cleanup, RecoveryCleanup, SocketState};

        // The interleaving: two clients see the same broken socket and both
        // decide to restart pid 4242. The first gets there, and its
        // replacement (pid 5150) binds the socket and writes the pid file.
        // The second arrives out of the graceful-exit wait and must not
        // unlink what the replacement just wrote — a daemon alive behind a
        // deleted socket is unreachable to every client, and the successor
        // this client would then spawn dies on the search-index lock.
        assert_eq!(
            recovery_cleanup(4242, SocketState::Reachable, Some(5150)),
            RecoveryCleanup::AlreadyRecovered
        );
        // The same replacement, caught while its socket probe is failing (a
        // busy daemon answers late): the pid file still says it exists.
        assert_eq!(
            recovery_cleanup(4242, SocketState::Stale, Some(5150)),
            RecoveryCleanup::SpawnWithoutClearing
        );

        // And the ordinary case this must keep doing: the daemon we signalled
        // exited and cleared its own pid file, or died leaving it behind.
        assert_eq!(
            recovery_cleanup(4242, SocketState::Stale, None),
            RecoveryCleanup::ClearAndSpawn
        );
        assert_eq!(
            recovery_cleanup(4242, SocketState::Missing, Some(4242)),
            RecoveryCleanup::ClearAndSpawn
        );
    }

    #[tokio::test]
    async fn a_process_that_exits_during_the_shutdown_wait_is_not_escalated_to() {
        use super::{DaemonEvidence, VerifiedDaemon};

        // Stands in for the escalation window: SIGTERM lands, the daemon
        // exits during the wait, and its number is free before the SIGKILL
        // would go out. The re-check has to notice, or the kill goes to
        // whatever holds the number next.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn child");
        let daemon = VerifiedDaemon::verify(child.id(), DaemonEvidence::PidFile);
        assert!(
            daemon.started_at.is_some(),
            "the fixture needs a start time to compare against"
        );

        let _ = child.kill();
        let _ = child.wait();

        assert!(
            !daemon.is_unchanged(),
            "a process that has exited must not be signalled again"
        );
    }

    #[test]
    fn a_daemon_identity_record_round_trips() {
        use super::DaemonIdentity;

        // Paths with spaces are the norm on macOS, where every profile lives
        // under "Application Support".
        let identity = DaemonIdentity {
            pid: 4242,
            started_at: "Wed Aug 20 00:11:22 2026".to_string(),
            exe: "/opt/homebrew/bin/mxr".to_string(),
            instance: "mxr-demo".to_string(),
            config_dir: "/Users/bk/Library/Application Support/mxr-demo".into(),
            data_dir: "/Users/bk/Library/Application Support/mxr-demo".into(),
        };

        let rendered = toml::to_string(&identity).expect("render identity");
        assert_eq!(
            toml::from_str::<DaemonIdentity>(&rendered).ok(),
            Some(identity)
        );
    }

    #[test]
    fn a_pid_file_without_an_identity_record_is_not_rejected() {
        use super::DaemonIdentity;

        // Daemons older than the record wrote a bare pid file and no record at
        // all; the caller must fall back rather than treat that as a mismatch.
        assert!(toml::from_str::<DaemonIdentity>("4242").is_err());
        assert!(toml::from_str::<DaemonIdentity>("").is_err());
    }

    #[test]
    fn a_hand_started_daemon_is_still_this_profile_s_daemon() {
        use super::command_runs_a_daemon;

        // The wedge this reproduces: requiring the argv autostart happens to
        // use rejected every explicitly started daemon, so the CLI deleted a
        // live daemon's pid file and then spawned a successor that died on
        // the search-index lock.
        assert!(command_runs_a_daemon(&words("/opt/bin/mxr daemon")));
        assert!(command_runs_a_daemon(&words(
            "/opt/bin/mxr daemon --foreground"
        )));
        assert!(command_runs_a_daemon(&words(
            "/opt/bin/mxr daemon --no-bridge --bridge-port 7777"
        )));
        assert!(command_runs_a_daemon(&words(
            "/opt/bin/mxr daemon --instance mxr-demo"
        )));

        // The pid file established the profile; all this has to catch is the
        // number having been handed to something else entirely.
        assert!(!command_runs_a_daemon(&words("/usr/bin/vim notes.txt")));
        assert!(!command_runs_a_daemon(&words("/opt/bin/mxr status")));
        assert!(!command_runs_a_daemon(&words("/opt/bin/mxr")));
    }

    #[test]
    fn a_pid_file_with_no_record_weighs_the_instance_name_against_the_pid() {
        use super::{command_names_another_instance, command_runs_a_daemon, pid_file_verdict};

        // With no identity record, "is argv[1] `daemon`" says yes to every mxr
        // daemon on the machine — including one that inherited this pid while
        // serving another profile. The instance name is the only thing on the
        // command line that points the other way, and autostart always passes
        // it.
        let another_profile = words("/opt/bin/mxr daemon --instance mxr-dev");
        assert!(command_names_another_instance(&another_profile, "mxr"));
        assert!(
            !pid_file_verdict(
                4242,
                None,
                || None,
                || Some(
                    command_runs_a_daemon(&another_profile)
                        && !command_names_another_instance(&another_profile, "mxr")
                )
            ),
            "a pid now running another profile's daemon must not be signalled on this one's behalf"
        );

        // Clap takes the name either way round, and a spelling this does not
        // read misses toward signalling a neighbour.
        assert!(command_names_another_instance(
            &words("/opt/bin/mxr daemon --instance=mxr-dev"),
            "mxr"
        ));
        assert!(command_names_another_instance(
            &words("/opt/bin/mxr daemon --instance=mxr-demo --foreground"),
            "mxr"
        ));
        assert!(!command_names_another_instance(
            &words("/opt/bin/mxr daemon --instance=mxr"),
            "mxr"
        ));

        // Every way of naming nobody: no flag, a value-less flag, and a name
        // `ps` output cannot be reassembled into. None of them is evidence
        // that the daemon belongs to somebody else.
        let hand_started = words("/opt/bin/mxr daemon --foreground");
        assert!(!command_names_another_instance(&hand_started, "mxr"));
        assert!(!command_names_another_instance(
            &words("/opt/bin/mxr daemon --instance"),
            "mxr"
        ));
        assert!(
            !command_names_another_instance(
                &words("/opt/bin/mxr daemon --instance my profile"),
                "my profile"
            ),
            "a name with a space survives no round trip through `ps`; half of it is not a mismatch"
        );
        assert!(pid_file_verdict(
            4242,
            None,
            || None,
            || Some(command_runs_a_daemon(&hand_started))
        ));
    }

    #[test]
    fn the_pre_signal_recheck_does_not_weigh_the_instance_name() {
        use super::{command_names_another_instance, DaemonEvidence, VerifiedDaemon};

        // Verification can afford the instance heuristic: rejecting there
        // drops through to the `ps` scan, which can still adopt the daemon by
        // canonical executable and full environment. The re-check cannot — it
        // runs where the start times agree or are both missing, and its "no"
        // goes straight to clearing the pid file and unlinking the socket of a
        // daemon that was never signalled. `--instance` is a marker the daemon
        // discards, so it is not worth that.
        let foreign_name = words("/opt/bin/mxr daemon --instance mxr-dev");
        assert!(command_names_another_instance(&foreign_name, "mxr"));

        assert!(
            VerifiedDaemon::still_the_same_process(
                Some("Wed Aug 20 00:11:22 2026"),
                DaemonEvidence::PidFile,
                Some("Wed Aug 20 00:11:22 2026"),
                || Some(super::command_runs_a_daemon(&foreign_name)),
                || panic!("a pid-file pid does not have to re-prove its profile"),
            ),
            "the re-check keeps a daemon the pid file already vouched for"
        );
    }

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
            progress: None,
        }];

        assert_eq!(
            classify_health(&sync, false, true, false),
            DaemonHealthClass::RestartRequired
        );
        assert_eq!(
            classify_health(&sync, true, false, false),
            DaemonHealthClass::RepairRequired
        );

        let mut degraded = sync.to_vec();
        degraded[0].healthy = false;
        assert_eq!(
            classify_health(&degraded, false, false, false),
            DaemonHealthClass::Degraded
        );
    }

    #[test]
    fn a_degraded_snapshot_never_reads_as_healthy() {
        // A degraded snapshot empties `sync_statuses`, so the unhealthy-account
        // check has nothing left to look at: the same status object would
        // otherwise carry `degraded: true` and `health_class: "healthy"`.
        assert_eq!(
            classify_health(&[], false, false, true),
            DaemonHealthClass::Degraded
        );
        assert_eq!(
            classify_health(&[], false, false, false),
            DaemonHealthClass::Healthy
        );
        // The version fields are read without touching the database, so a
        // snapshot that timed out still knows the daemon is the wrong build.
        assert_eq!(
            classify_health(&[], false, true, true),
            DaemonHealthClass::RestartRequired
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
                                degraded: false,
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
