use crate::app::ConnectionState;
use crate::async_result::AsyncResult;
use crate::client::Client;
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, Request, Response};
use std::path::Path;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration, Instant};

/// Upper bound on any single request through the shared IPC worker. The
/// daemon answers everything on this path in well under a second when
/// healthy (slow LLM work uses dedicated connections); large list fetches
/// on a cold multi-GB store are the slowest legitimate case, still far
/// inside this bound. Only a dead or wedged peer ever reaches it.
pub(crate) const IPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// A request sent from the main loop to the background IPC worker.
pub(crate) struct IpcRequest {
    pub(crate) request: Request,
    pub(crate) reply: oneshot::Sender<Result<Response, MxrError>>,
}

/// Runs a single persistent daemon connection in a background task.
/// The main loop sends requests via channel — no new connections per operation.
/// Daemon events (SyncCompleted, LabelCountsUpdated, etc.) are forwarded to result_tx.
pub(crate) fn spawn_ipc_worker(
    socket_path: std::path::PathBuf,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<IpcRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();
    tokio::spawn(async move {
        // Create event channel — Client forwards daemon events here
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<DaemonEvent>();

        let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connecting));

        // Initial connect with retry. The previous behavior was to silently
        // exit the worker on first connect failure, leaving the UI hung at
        // "connecting" forever. Now we retry forever, emit state transitions
        // for the UI to render, and reply Err to any pending requests so the
        // main loop's mutation queue doesn't stall.
        let mut client = loop {
            match connect_ipc_client(&socket_path, event_tx.clone()).await {
                Ok(client) => {
                    let _ =
                        result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connected));
                    break client;
                }
                Err(error) => {
                    let _ = result_tx.send(AsyncResult::ConnectionState(
                        ConnectionState::Reconnecting {
                            since: std::time::Instant::now(),
                            reason: error.to_string(),
                        },
                    ));
                    // Drain any queued requests with an error so the main
                    // loop doesn't sit indefinitely waiting on oneshot replies.
                    while let Ok(req) = rx.try_recv() {
                        let _ = req
                            .reply
                            .send(Err(MxrError::Ipc("daemon not connected".into())));
                    }
                    sleep(Duration::from_secs(2)).await;
                    if rx.is_closed() {
                        return;
                    }
                }
            }
        };

        loop {
            tokio::select! {
                req = rx.recv() => {
                    match req {
                        Some(req) => {
                            // Bound every request. Without this, a daemon that
                            // accepts the connection but never replies (wedged,
                            // killed mid-request, orphaned socket) parks this
                            // worker forever: the in-flight oneshot never fires,
                            // the UI shows its queued status ("Archiving...")
                            // indefinitely, and every later request queues
                            // behind the stuck one. Slow LLM work doesn't go
                            // through this worker (see ipc_call_dedicated), so
                            // a generous bound only ever fires on a dead peer.
                            let mut result = match tokio::time::timeout(
                                IPC_REQUEST_TIMEOUT,
                                client.raw_request(req.request.clone()),
                            )
                            .await
                            {
                                Ok(result) => result,
                                Err(_) => {
                                    // The old connection has a stranded
                                    // in-flight request; a late reply would be
                                    // misattributed to the next request. Drop
                                    // it and connect fresh. If the reconnect
                                    // fails, keep going — the next request
                                    // re-enters the reconnect path.
                                    if let Ok(fresh) =
                                        connect_ipc_client(&socket_path, event_tx.clone()).await
                                    {
                                        client = fresh;
                                    }
                                    Err(MxrError::Ipc(format!(
                                        "daemon did not respond within {}s — the request may or may not have been applied",
                                        IPC_REQUEST_TIMEOUT.as_secs()
                                    )))
                                }
                            };
                            if should_reconnect_ipc(&result)
                                && request_supports_retry(&req.request)
                            {
                                match connect_ipc_client(&socket_path, event_tx.clone()).await {
                                    Ok(mut reconnected) => {
                                        let retry = match tokio::time::timeout(
                                            IPC_REQUEST_TIMEOUT,
                                            reconnected.raw_request(req.request.clone()),
                                        )
                                        .await
                                        {
                                            Ok(retry) => retry,
                                            Err(_) => Err(MxrError::Ipc(format!(
                                                "daemon did not respond within {}s — the request may or may not have been applied",
                                                IPC_REQUEST_TIMEOUT.as_secs()
                                            ))),
                                        };
                                        if retry.is_ok() {
                                            client = reconnected;
                                        }
                                        result = retry;
                                    }
                                    Err(error) => {
                                        result = Err(error);
                                    }
                                }
                            }
                            let _ = req.reply.send(result);
                        }
                        None => break,
                    }
                }
                event = event_rx.recv() => {
                    if let Some(event) = event {
                        let _ = result_tx.send(AsyncResult::DaemonEvent(event));
                    }
                }
                idle = client.read_idle_frame() => {
                    if let Err(error) = idle {
                        let _ = result_tx.send(AsyncResult::ConnectionState(
                            ConnectionState::Reconnecting {
                                since: std::time::Instant::now(),
                                reason: error.to_string(),
                            },
                        ));
                        match connect_ipc_client(&socket_path, event_tx.clone()).await {
                            Ok(fresh) => {
                                client = fresh;
                                let _ = result_tx
                                    .send(AsyncResult::ConnectionState(ConnectionState::Connected));
                            }
                            Err(_) => {
                                sleep(Duration::from_secs(2)).await;
                            }
                        }
                    }
                }
            }
        }
    });
    tx
}

async fn connect_ipc_client(
    socket_path: &std::path::Path,
    event_tx: mpsc::UnboundedSender<DaemonEvent>,
) -> Result<Client, MxrError> {
    match Client::connect(socket_path).await {
        Ok(client) => Ok(client.with_event_channel(event_tx)),
        Err(error) if should_autostart_daemon(&error) => {
            start_daemon_process(socket_path).await?;
            wait_for_daemon_client(socket_path, START_DAEMON_TIMEOUT)
                .await
                .map(|client| client.with_event_channel(event_tx))
        }
        Err(error) => Err(MxrError::Ipc(error.to_string())),
    }
}

pub(crate) fn should_reconnect_ipc(result: &Result<Response, MxrError>) -> bool {
    match result {
        Err(MxrError::Ipc(message)) => {
            let lower = message.to_lowercase();
            lower.contains("broken pipe")
                || lower.contains("connection closed")
                || lower.contains("connection refused")
                || lower.contains("connection reset")
        }
        _ => false,
    }
}

pub(crate) const START_DAEMON_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const START_DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn should_autostart_daemon(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
    )
}

async fn start_daemon_process(socket_path: &Path) -> Result<(), MxrError> {
    let exe = std::env::current_exe()
        .map_err(|error| MxrError::Ipc(format!("failed to locate mxr binary: {error}")))?;
    std::process::Command::new(exe)
        .arg("daemon")
        .arg("--instance")
        .arg(mxr_config::app_instance_name())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            MxrError::Ipc(format!(
                "failed to start daemon for {}: {error}",
                socket_path.display()
            ))
        })?;
    Ok(())
}

async fn wait_for_daemon_client(socket_path: &Path, timeout: Duration) -> Result<Client, MxrError> {
    let deadline = Instant::now() + timeout;
    let mut last_error: Option<MxrError> = None;

    loop {
        if Instant::now() >= deadline {
            let detail =
                last_error.unwrap_or_else(|| MxrError::Ipc("daemon did not become ready".into()));
            return Err(MxrError::Ipc(format!(
                "daemon restart did not become ready for {}: {}",
                socket_path.display(),
                detail
            )));
        }

        match Client::connect(socket_path).await {
            Ok(mut client) => match client.raw_request(Request::GetStatus).await {
                Ok(_) => return Ok(client),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(MxrError::Ipc(error.to_string())),
        }

        sleep(START_DAEMON_POLL_INTERVAL).await;
    }
}

pub(crate) fn request_supports_retry(request: &Request) -> bool {
    matches!(
        request,
        Request::ListEnvelopes { .. }
            | Request::ListEnvelopesByIds { .. }
            | Request::GetEnvelope { .. }
            | Request::GetBody { .. }
            | Request::GetHtmlImageAssets { .. }
            | Request::ListBodies { .. }
            | Request::GetThread { .. }
            | Request::ListThreads { .. }
            | Request::ListLabels { .. }
            | Request::ListRules
            | Request::ListAccounts
            | Request::ListAccountsConfig
            | Request::GetRule { .. }
            | Request::GetRuleForm { .. }
            | Request::DryRunRules { .. }
            | Request::ListEvents { .. }
            | Request::GetLogs { .. }
            | Request::GetDoctorReport
            | Request::GenerateBugReport { .. }
            | Request::ListRuleHistory { .. }
            | Request::Search { .. }
            | Request::GetSyncStatus { .. }
            | Request::Count { .. }
            | Request::SearchAggregation { .. }
            | Request::GetHeaders { .. }
            | Request::ListSavedSearches
            | Request::ListSavedSearchUnreadCounts
            | Request::ListSubscriptions { .. }
            | Request::RunSavedSearch { .. }
            | Request::ListSnoozed
            | Request::ResolveSignature { .. }
            | Request::PrepareReply { .. }
            | Request::PrepareForward { .. }
            | Request::ListDrafts
            | Request::GetStatus
            | Request::Ping
    )
}

/// Send a request to the IPC worker and get the response.
pub(crate) async fn ipc_call(
    tx: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
) -> Result<Response, MxrError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(IpcRequest {
        request,
        reply: reply_tx,
    })
    .map_err(|_| MxrError::Ipc("IPC worker closed".into()))?;
    reply_rx
        .await
        .map_err(|_| MxrError::Ipc("IPC worker dropped".into()))?
}

/// Send one request on a short-lived daemon connection.
///
/// Used for slow, user-triggered LLM work so the main TUI IPC worker can keep
/// serving body fetches, search, and mutations while the LLM request runs.
pub(crate) async fn ipc_call_dedicated(
    socket_path: &Path,
    request: Request,
) -> Result<Response, MxrError> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    drop(event_rx);
    let mut client = connect_ipc_client(socket_path, event_tx).await?;
    client.raw_request(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::SinkExt;
    use mxr_core::id::AccountId;
    use mxr_protocol::{IpcCodec, IpcMessage, IpcPayload};
    use tokio::io::AsyncReadExt;
    use tokio_util::codec::Framed;

    /// A daemon that accepts the connection but never replies must produce
    /// a timeout error, not park the worker (and the UI status) forever.
    #[tokio::test(start_paused = true)]
    async fn unresponsive_daemon_times_out_instead_of_hanging() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("mxr.sock");
        let listener = tokio::net::UnixListener::bind(&sock).expect("bind");

        // Accept and read forever, never writing a response.
        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut sink = [0u8; 1024];
            loop {
                match stream.read(&mut sink).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        let (result_tx, _result_rx) = mpsc::unbounded_channel();
        let worker = spawn_ipc_worker(sock, result_tx);

        let result = ipc_call(&worker, Request::Ping).await;
        let error = result.expect_err("must not hang or succeed");
        assert!(
            error.to_string().contains("did not respond"),
            "unexpected error: {error}"
        );
    }

    /// Daemon events pushed while the TUI is idle (no request in flight)
    /// must be forwarded to the result channel without any request trigger.
    /// This is a RED test — it fails against current code because the worker
    /// only reads events inside request(), not while idle.
    #[tokio::test]
    async fn idle_daemon_events_are_delivered_without_a_request() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("mxr_idle_events.sock");
        let listener = tokio::net::UnixListener::bind(&sock).expect("bind");

        // Fake daemon: accept, immediately push one SyncCompleted event,
        // then hold the connection open.
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut framed = Framed::new(stream, IpcCodec::new());
            let event_msg = IpcMessage {
                id: 0,
                source: mxr_protocol::ClientKind::Daemon,
                payload: IpcPayload::Event(DaemonEvent::SyncCompleted {
                    account_id: AccountId::new(),
                    messages_synced: 3,
                }),
            };
            let _ = framed.send(event_msg).await;
            // Hold the connection open so the worker doesn't see EOF.
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });

        let (result_tx, mut result_rx) = mpsc::unbounded_channel();
        let _worker = spawn_ipc_worker(sock, result_tx);

        // The worker should deliver the event without us sending any request.
        let received = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let msg = result_rx.recv().await?;
                if let AsyncResult::DaemonEvent(_) = msg {
                    return Some(msg);
                }
            }
        })
        .await;
        assert!(
            received.is_ok(),
            "timed out waiting for idle daemon event — worker does not read events while idle"
        );
        assert!(
            matches!(received.unwrap(), Some(AsyncResult::DaemonEvent(_))),
            "unexpected result"
        );
    }

    /// When the daemon closes the connection while the worker is idle,
    /// the worker must emit ConnectionState::Reconnecting and not hang.
    #[tokio::test(start_paused = true)]
    async fn idle_connection_close_triggers_reconnect_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("mxr_idle_close.sock");
        let listener = tokio::net::UnixListener::bind(&sock).expect("bind");

        // Fake daemon: accept the first connection then immediately drop it.
        // Also accept a second connection (from reconnect attempt) and hold it.
        tokio::spawn(async move {
            // First connection: accept and drop immediately.
            if let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
            // Second connection (reconnect): accept and hold open.
            if let Ok((_stream, _)) = listener.accept().await {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });

        let (result_tx, mut result_rx) = mpsc::unbounded_channel();
        let _worker = spawn_ipc_worker(sock, result_tx);

        // Drain until we see a Reconnecting state.
        let saw_reconnecting = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let msg = result_rx.recv().await?;
                if let AsyncResult::ConnectionState(ConnectionState::Reconnecting { .. }) = msg {
                    return Some(());
                }
            }
        })
        .await;
        assert!(
            saw_reconnecting.is_ok(),
            "timed out — worker did not emit Reconnecting after idle connection close"
        );
    }
}
