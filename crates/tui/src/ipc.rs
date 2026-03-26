use crate::mxr_core::MxrError;
use crate::mxr_protocol::{DaemonEvent, Request, Response};
use crate::mxr_tui::async_result::AsyncResult;
use crate::mxr_tui::client::Client;
use std::path::Path;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration, Instant};

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
        let mut client = match connect_ipc_client(&socket_path, event_tx.clone()).await {
            Ok(client) => client,
            Err(_) => return,
        };

        loop {
            tokio::select! {
                req = rx.recv() => {
                    match req {
                        Some(req) => {
                            let mut result = client.raw_request(req.request.clone()).await;
                            if should_reconnect_ipc(&result)
                                && request_supports_retry(&req.request)
                            {
                                match connect_ipc_client(&socket_path, event_tx.clone()).await {
                                    Ok(mut reconnected) => {
                                        let retry = reconnected.raw_request(req.request.clone()).await;
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
            | Request::GetHeaders { .. }
            | Request::ListSavedSearches
            | Request::ListSubscriptions { .. }
            | Request::RunSavedSearch { .. }
            | Request::ListSnoozed
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
