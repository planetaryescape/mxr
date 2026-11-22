use crate::app::ConnectionState;
use crate::async_result::AsyncResult;
use crate::client::{Client, DaemonSession};
use mxr_core::MxrError;
use mxr_protocol::{DaemonEvent, Request, Response};
use std::path::Path;
use std::process::Stdio;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration, Instant};

tokio::task_local! { pub(crate) static COMMAND_OWNER: Option<mxr_protocol::ArchiveWorkspaceData>; }

pub(crate) const IPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) struct IpcRequest {
    pub(crate) expected_workspace: Option<mxr_protocol::ArchiveWorkspaceData>,
    pub(crate) request: Request,
    pub(crate) attachment_transfer: Option<AttachmentTransferRequest>,
    pub(crate) reply: oneshot::Sender<Result<Response, MxrError>>,
}

pub(crate) struct AttachmentTransferRequest {
    pub(crate) message_id: mxr_core::MessageId,
    pub(crate) attachment_id: mxr_core::AttachmentId,
    pub(crate) destination: std::path::PathBuf,
    pub(crate) replace_existing: bool,
}

#[cfg(test)]
pub(crate) fn spawn_ipc_worker(
    socket_path: std::path::PathBuf,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<IpcRequest> {
    spawn_ipc_worker_selected(socket_path.into(), None, result_tx)
}

pub(crate) fn spawn_ipc_worker_for_session(
    session: DaemonSession,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<IpcRequest> {
    spawn_ipc_worker_selected(session.target, Some(session.owner), result_tx)
}

fn retire_selected_session(
    result_tx: &mpsc::UnboundedSender<AsyncResult>,
    rx: &mut mpsc::UnboundedReceiver<IpcRequest>,
) {
    rx.close();
    while let Ok(request) = rx.try_recv() {
        let _ = request.reply.send(Err(MxrError::Ipc(
            mxr_client::selected_daemon::SELECTION_CHANGED_MESSAGE.into(),
        )));
    }
    let _ = result_tx.send(AsyncResult::ConnectionState(
        ConnectionState::SelectionChanged,
    ));
}

fn spawn_ipc_worker_selected(
    target: mxr_client::SelectedDaemonTarget,
    initial_owner: Option<Option<mxr_protocol::ArchiveWorkspaceData>>,
    result_tx: mpsc::UnboundedSender<AsyncResult>,
) -> mpsc::UnboundedSender<IpcRequest> {
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();
    tokio::spawn(async move {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<DaemonEvent>();

        let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connecting));

        let (mut client, session_workspace) = loop {
            if matches!(target.selection_is_current().await, Ok(false)) {
                retire_selected_session(&result_tx, &mut rx);
                return;
            }
            match connect_ipc_client(&target, event_tx.clone()).await {
                Ok(mut client) => {
                    let owner = if let Some(owner) = &initial_owner {
                        owner.clone()
                    } else {
                        let capture =
                            tokio::time::timeout(IPC_REQUEST_TIMEOUT, client.capture_workspace());
                        tokio::pin!(capture);
                        loop {
                            tokio::select! {
                                result = &mut capture => break result.ok().and_then(Result::ok),
                                Some(event) = event_rx.recv() => { let _ = result_tx.send(AsyncResult::DaemonEvent(event)); }
                            }
                        }
                    };
                    if let Some(owner) = &owner {
                        let _ = result_tx.send(AsyncResult::WorkspaceOwner(owner.clone()));
                    }
                    let _ =
                        result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connected));
                    break (client, owner);
                }
                Err(error) => {
                    let _ = result_tx.send(AsyncResult::ConnectionState(
                        ConnectionState::Reconnecting {
                            since: std::time::Instant::now(),
                            reason: error.to_string(),
                        },
                    ));
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
            if matches!(target.selection_is_current().await, Ok(false)) {
                retire_selected_session(&result_tx, &mut rx);
                return;
            }
            tokio::select! {
                req = rx.recv() => {
                    match req {
                        Some(req) => {
                            if req.reply.is_closed() && request_supports_retry(&req.request) {
                                continue;
                            }
                            let is_transfer = req.attachment_transfer.is_some();
                            let transfer = req.attachment_transfer;
                            let completed = {
                            let operation = async {
                                if session_workspace.is_none() && req.expected_workspace.is_none() && matches!(req.request, Request::GetArchiveWorkspace) {
                                    return Err(MxrError::Ipc("This native session has no original workspace owner. Preserve retained work and explicitly reopen the client to review the selected workspace.".into()));
                                }
                                if let Some(transfer) = transfer {
                                    let owner = req.expected_workspace.as_ref().or(session_workspace.as_ref()).ok_or_else(|| MxrError::Ipc("Attachment transfer has no original workspace owner; reopen the client before continuing".into()))?;
                                    client.raw_request_owned(Request::Ping, Some(owner)).await?;
                                    let file = client.transfer_attachment_to(
                                        &transfer.message_id,
                                        &transfer.attachment_id,
                                        &transfer.destination,
                                        transfer.replace_existing,
                                    ).await?;
                                    Ok(Response::Ok { data: mxr_protocol::ResponseData::AttachmentFile { file } })
                                } else {
                                    client.raw_request_owned(req.request.clone(), req.expected_workspace.as_ref().or(session_workspace.as_ref())).await
                                }
                            };
                            let operation = tokio::time::timeout(
                                IPC_REQUEST_TIMEOUT,
                                operation,
                            );
                            tokio::pin!(operation);
                            loop {
                                tokio::select! {
                                    result = &mut operation => break result,
                                    Some(event) = event_rx.recv() => {
                                        let _ = result_tx.send(AsyncResult::DaemonEvent(event));
                                    }
                                }
                            }
                            };
                            let mut result = match completed {
                                Ok(result) => result,
                                Err(_) => {
                                    let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Reconnecting {
                                        since: std::time::Instant::now(), reason: "request timed out; reconnecting without replay".into(),
                                    }));
                                    if let Ok(fresh) = connect_ipc_client(&target, event_tx.clone()).await {
                                        if client.replace_connection(fresh).is_ok() {
                                            let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connected));
                                        }
                                    }
                                    Err(MxrError::Ipc(format!(
                                        "daemon did not respond within {}s — the request may or may not have been applied",
                                        IPC_REQUEST_TIMEOUT.as_secs()
                                    )))
                                }
                            };
                            if !is_transfer && should_reconnect_ipc(&result)
                                && request_supports_retry(&req.request)
                            {
                                let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Reconnecting {
                                    since: std::time::Instant::now(), reason: "connection failed; retrying a safe read".into(),
                                }));
                                match connect_ipc_client(&target, event_tx.clone()).await {
                                    Ok(mut reconnected) => {
                                        let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connected));
                                        let retry = match tokio::time::timeout(
                                            IPC_REQUEST_TIMEOUT,
                                            reconnected.raw_request_owned(req.request.clone(), req.expected_workspace.as_ref().or(session_workspace.as_ref())),
                                        )
                                        .await
                                        {
                                            Ok(retry) => retry,
                                            Err(_) => Err(MxrError::Ipc(format!(
                                                "daemon did not respond within {}s — the request may or may not have been applied",
                                                IPC_REQUEST_TIMEOUT.as_secs()
                                            ))),
                                        };
                                        result = if retry.is_ok() {
                                            client.replace_connection(reconnected).and(retry)
                                        } else {
                                            retry
                                        };
                                    }
                                    Err(error) => {
                                        let _ = result_tx.send(AsyncResult::ConnectionState(
                                            ConnectionState::Reconnecting {
                                                since: std::time::Instant::now(),
                                                reason: error.to_string(),
                                            },
                                        ));
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
                        if matches!(target.selection_is_current().await,Ok(false)) {
                            retire_selected_session(&result_tx,&mut rx);return;
                        }
                        let _ = result_tx.send(AsyncResult::ConnectionState(
                            ConnectionState::Reconnecting {
                                since: std::time::Instant::now(),
                                reason: error.to_string(),
                            },
                        ));
                        match connect_ipc_client(&target, event_tx.clone()).await {
                            Ok(fresh) => {
                                match client.replace_connection(fresh) {
                                    Ok(()) => {
                                        let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connected));
                                    }
                                    Err(error) => {
                                        let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Reconnecting {
                                            since: std::time::Instant::now(), reason: error.to_string(),
                                        }));
                                        sleep(Duration::from_secs(2)).await;
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = result_tx.send(AsyncResult::ConnectionState(
                                    ConnectionState::Reconnecting {
                                        since: std::time::Instant::now(),
                                        reason: error.to_string(),
                                    },
                                ));
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
    target: &mxr_client::SelectedDaemonTarget,
    event_tx: mpsc::UnboundedSender<DaemonEvent>,
) -> Result<Client, MxrError> {
    match Client::connect_target(target).await {
        Ok(client) => Ok(client.with_event_channel(event_tx)),
        Err(error) if target.unix_path().is_some() && should_autostart_daemon(&error) => {
            let Some(socket_path) = target.unix_path() else {
                return Err(MxrError::Ipc(error.to_string()));
            };
            if !mxr_client::daemon_autostart_enabled() {
                return Err(MxrError::Ipc(format!(
                    "{} ({})",
                    mxr_client::daemon_autostart_disabled_message(),
                    socket_path.display()
                )));
            }
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
    if !mxr_client::daemon_autostart_enabled() {
        return Err(MxrError::Ipc(
            mxr_client::daemon_autostart_disabled_message().into(),
        ));
    }
    let exe = std::env::current_exe()
        .map_err(|error| MxrError::Ipc(format!("failed to locate mxr binary: {error}")))?;
    let mut command = std::process::Command::new(exe);
    let inherited = mxr_client::TransportAddr::resolve(mxr_config::socket_path())
        .map_err(|error| MxrError::Ipc(error.to_string()))?;
    if !matches!(inherited, mxr_client::TransportAddr::Unix(ref path) if path == socket_path) {
        command
            .env_remove("MXR_DAEMON_ADDR")
            .env("MXR_SOCKET_PATH", socket_path);
    }
    command
        .arg("daemon")
        .env("MXR_DAEMON_CLIENT_AUTOSTART", "1")
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
            | Request::SystemMailboxCounts { .. }
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
            | Request::ListMessages { .. }
            | Request::GetPerson { .. }
            | Request::GetModeGuide { .. }
            | Request::GetStatus
            | Request::Ping
    )
}

pub(crate) async fn ipc_call(
    tx: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
) -> Result<Response, MxrError> {
    let retained = crate::send_recovery_state::owner_for_request(&request)
        .map_err(|error| MxrError::Ipc(error.to_string()))?;
    let captured = COMMAND_OWNER.try_with(Clone::clone).ok().flatten();
    if retained.is_some() && captured.is_some() && retained != captured {
        return Err(MxrError::Ipc(
            "Queued command belongs to another workspace; retained private work was not applied"
                .into(),
        ));
    }
    let mut owner = retained.or(captured);
    if owner.is_none() && request.requires_workspace_owner() {
        owner = Some(capture_workspace(tx).await?);
    }
    ipc_call_owned(tx, request, owner).await
}

pub(crate) async fn capture_workspace(
    tx: &mpsc::UnboundedSender<IpcRequest>,
) -> Result<mxr_protocol::ArchiveWorkspaceData, MxrError> {
    match ipc_call_owned(tx, Request::GetArchiveWorkspace, None).await? {
        Response::Ok {
            data: mxr_protocol::ResponseData::ArchiveWorkspace { workspace },
        } => Ok(workspace),
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc(
            "Selected daemon does not provide workspace identity".into(),
        )),
    }
}

pub(crate) async fn ipc_call_owned(
    tx: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
    expected_workspace: Option<mxr_protocol::ArchiveWorkspaceData>,
) -> Result<Response, MxrError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(IpcRequest {
        expected_workspace,
        request,
        attachment_transfer: None,
        reply: reply_tx,
    })
    .map_err(|_| MxrError::Ipc("IPC worker closed".into()))?;
    reply_rx
        .await
        .map_err(|_| MxrError::Ipc("IPC worker dropped".into()))?
}

pub(crate) async fn ipc_transfer_attachment(
    tx: &mpsc::UnboundedSender<IpcRequest>,
    transfer: AttachmentTransferRequest,
) -> Result<Response, MxrError> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(IpcRequest {
        expected_workspace: None,
        request: Request::GetAttachmentTransferMetadata {
            message_id: transfer.message_id.clone(),
            attachment_id: transfer.attachment_id.clone(),
        },
        attachment_transfer: Some(transfer),
        reply: reply_tx,
    })
    .map_err(|_| MxrError::Ipc("IPC worker closed".into()))?;
    reply_rx
        .await
        .map_err(|_| MxrError::Ipc("IPC worker dropped".into()))?
}

pub(crate) async fn ipc_call_dedicated(
    session: &DaemonSession,
    request: Request,
) -> Result<Response, MxrError> {
    let owner = session.owner.as_ref().ok_or_else(|| MxrError::Ipc("This native session has no original workspace owner; reopen the client before continuing".into()))?;
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    drop(event_rx);
    let mut client = connect_ipc_client(&session.target, event_tx).await?;
    client.raw_request_owned(request, Some(owner)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_stopped_host_is_not_started_by_tui_reconnect() {
        let directory = tempfile::tempdir().unwrap();
        let state = directory.path().join("hosting.json");
        let original = r#"{"version":1,"daemon_enabled":false}"#;
        std::fs::write(&state, original).unwrap();
        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", directory.path().to_str()),
                ("MXR_DAEMON_AUTOSTART", None),
                ("MXR_DAEMON_ADDR", None),
            ],
            || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        let (events, _) = mpsc::unbounded_channel();
                        let target = mxr_client::SelectedDaemonTarget::from(
                            directory.path().join("absent.sock"),
                        );
                        let result = tokio::time::timeout(
                            Duration::from_secs(2),
                            connect_ipc_client(&target, events),
                        )
                        .await
                        .unwrap();
                        assert!(result
                            .err()
                            .unwrap()
                            .to_string()
                            .contains("explicitly stopped"));
                        assert!(start_daemon_process(&directory.path().join("absent.sock"))
                            .await
                            .unwrap_err()
                            .to_string()
                            .contains("explicitly stopped"));
                        assert_eq!(std::fs::read_to_string(&state).unwrap(), original);
                    });
            },
        );
    }

    use futures::StreamExt;

    use mxr_protocol::IpcCodec;
    use tokio_util::codec::Framed;

    #[tokio::test(start_paused = true)]
    async fn unresponsive_daemon_times_out_instead_of_hanging() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join("mxr.sock");
        let listener = tokio::net::UnixListener::bind(&sock).expect("bind");

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut framed = Framed::new(stream, IpcCodec::new());
            crate::selected_daemon_tests::accept_declaration(&mut framed).await;
            while let Some(Ok(_)) = framed.next().await {}
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
}
