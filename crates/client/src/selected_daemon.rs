//! One resolved target is reused for every connection in a client session.
use crate::{ClientError, IpcConnection, TransportAddr};
use mxr_protocol::{ClientKind, Request, ResponseData, IPC_PROTOCOL_VERSION};
use mxr_transport::{CmdConnector, Connector, TcpConnector, UnixConnector};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const SETUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct SelectedDaemonTarget {
    address: TransportAddr,
    token: Option<String>,
    generation: Option<u64>,
    saved: Option<(mxr_config::connection_state::ConnectionRegistration, u64)>,
}

impl std::fmt::Debug for SelectedDaemonTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectedDaemonTarget")
            .field("address", &self.description())
            .finish_non_exhaustive()
    }
}

impl From<PathBuf> for SelectedDaemonTarget {
    fn from(path: PathBuf) -> Self {
        Self::new(TransportAddr::Unix(path), None)
    }
}

impl SelectedDaemonTarget {
    pub fn new(address: TransportAddr, token: Option<String>) -> Self {
        Self {
            address,
            token,
            saved: None,
            generation: None,
        }
    }

    /// Resolve credentials only for TCP. The caller owns its configuration.
    pub fn resolve(
        default_socket: PathBuf,
        token: impl FnOnce() -> Result<Option<String>, std::io::Error>,
    ) -> Result<Self, ClientError> {
        if std::env::var("MXR_DAEMON_ADDR")
            .ok()
            .is_none_or(|value| value.trim().is_empty())
        {
            let state = mxr_config::connection_state::read(&mxr_config::connection_state::path())?;
            if let Some(saved) = state.selected {
                return Ok(Self {
                    address: TransportAddr::Unix(default_socket),
                    token: None,
                    saved: Some((saved, state.generation)),
                    generation: Some(state.generation),
                });
            }
            return Ok(Self {
                address: TransportAddr::Unix(default_socket),
                token: None,
                saved: None,
                generation: Some(state.generation),
            });
        }
        let address = TransportAddr::resolve(default_socket)?;
        let token = if matches!(address, TransportAddr::Tcp(_)) {
            token()?
        } else {
            None
        };
        Ok(Self::new(address, token))
    }

    pub fn description(&self) -> String {
        if let Some((saved, _)) = &self.saved {
            return saved.origin.clone();
        }
        match &self.address {
            TransportAddr::Cmd(_) => "cmd://<command hidden>".into(),
            TransportAddr::Unix(path) => format!("unix://{}", path.display()),
            TransportAddr::Tcp(address) => format!("tcp://{address}"),
        }
    }

    pub fn unix_path(&self) -> Option<&Path> {
        if self.saved.is_some() {
            return None;
        }
        match &self.address {
            TransportAddr::Unix(path) => Some(path),
            _ => None,
        }
    }

    pub fn is_paired_remote(&self) -> bool {
        self.saved.is_some()
    }

    pub fn is_remote(&self) -> bool {
        self.unix_path().is_none()
    }

    fn connector(&self) -> Box<dyn Connector> {
        match &self.address {
            TransportAddr::Unix(path) => Box::new(UnixConnector::new(path.clone())),
            TransportAddr::Tcp(address) => {
                Box::new(TcpConnector::new(*address, self.token.clone()))
            }
            TransportAddr::Cmd(argv) => Box::new(CmdConnector::new(argv.clone())),
        }
    }

    pub async fn connect(&self, source: ClientKind) -> Result<IpcConnection, ClientError> {
        crate::remote_connection::schedule_cleanup();
        if let Some((saved, generation)) = &self.saved {
            let connection = connect_registration(saved, source).await?;
            return Ok(connection.with_selection_generation(*generation));
        }
        if let Some(path) = self.unix_path() {
            let connection = if crate::daemon_autostart_enabled() {
                IpcConnection::connect(path, source).await
            } else {
                IpcConnection::connect_supervised(path, source).await
            }?;
            return Ok(match self.generation {
                Some(generation) => connection.with_selection_generation(generation),
                None => connection,
            });
        }
        let connector = self.connector();
        let setup = async {
            let mut connection = IpcConnection::connect_with(connector.as_ref(), source).await?;
            match connection.request(Request::GetStatus).await? {
                ResponseData::Status { protocol_version, .. } if protocol_version == IPC_PROTOCOL_VERSION => {
                    connection.selected_remote = true;
                    Ok(connection)
                }
                ResponseData::Status { protocol_version, .. } => Err(self.setup_error(&format!("daemon reports protocol {protocol_version}; this client supports protocol {IPC_PROTOCOL_VERSION}"))),
                _ => Err(self.setup_error("daemon returned an unexpected status response")),
            }
        };
        tokio::time::timeout(SETUP_TIMEOUT, setup)
            .await
            .map_err(|_| {
                self.setup_error(
                    "connect/authentication/status handshake timed out after 5 seconds",
                )
            })?
            .map_err(|error| {
                let reason = if matches!(self.address, TransportAddr::Cmd(_))
                    && matches!(error, ClientError::Transport(_))
                {
                    "Cannot open the selected cmd:// daemon bridge; check that its executable is available"
                        .to_string()
                } else {
                    error.to_string()
                };
                self.setup_error(&reason)
            })
    }

    fn setup_error(&self, reason: &str) -> ClientError {
        ClientError::Io(std::io::Error::other(format!("Cannot connect to selected {}: {reason}. Check the selected daemon/bridge and supported IPC protocol {IPC_PROTOCOL_VERSION}; no local daemon was used", self.description())))
    }
}

pub(crate) fn requires_local_account_setup(request: &Request) -> bool {
    matches!(
        request,
        Request::AuthorizeAccountConfig { .. }
            | Request::StartAuthSession { .. }
            | Request::GetAuthSession { .. }
            | Request::CancelAuthSession { .. }
            | Request::CompleteAuthSession { .. }
            | Request::UpsertAccountConfig { .. }
            | Request::SetDefaultAccount { .. }
            | Request::TestAccountConfig { .. }
            | Request::DisableAccountConfig { .. }
            | Request::RemoveAccountConfig { .. }
            | Request::RepairAccountConfig { .. }
    )
}

/// Verify the authenticated host, workspace epoch and exact wire revision before use.
pub async fn connect_registration(
    saved: &mxr_config::connection_state::ConnectionRegistration,
    source: ClientKind,
) -> Result<IpcConnection, ClientError> {
    let connector = mxr_transport::WssConnector::new(
        saved.origin.clone(),
        &saved.client_id,
        saved.mail_secret.expose(),
    )?;
    let setup = async {
        let mut connection = IpcConnection::connect_with(&connector, source).await?;
        match connection.request(Request::GetRemoteReady).await? {
            ResponseData::RemoteReady {protocol_version, host_id, workspace, client_id, capabilities, ..}
                if protocol_version == IPC_PROTOCOL_VERSION && client_id == saved.client_id
                    && saved.capabilities.iter().all(|grant| capabilities.contains(grant))
                    && (saved.host_id.is_empty() || (host_id == saved.host_id && workspace.workspace_id == saved.workspace_id && workspace.recovery_generation == saved.recovery_generation)) => {
                        connection.selected_remote = true;
                        connection.remote_workspace = Some(workspace);
                        connection.remote_registration_identity = Some((host_id, client_id));
                        Ok(connection)
                    }
            ResponseData::RemoteReady { protocol_version, .. } if protocol_version != IPC_PROTOCOL_VERSION => Err(ClientError::Io(std::io::Error::other(format!("Client supports IPC protocol {IPC_PROTOCOL_VERSION}; remote host reports {protocol_version}. Upgrade host and clients to matching mxr versions; saved selection was preserved")))),
            _ => Err(ClientError::Io(std::io::Error::other("Remote host/workspace/grant identity mismatch; retain work and reconnect explicitly"))),
        }
    };
    tokio::time::timeout(SETUP_TIMEOUT, setup)
        .await
        .map_err(|_| ClientError::Timeout(SETUP_TIMEOUT))?
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "owned transport fixtures assert directly"
    )]
    use super::*;
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{IpcCodec, IpcMessage, IpcPayload, Response};
    use tokio::net::TcpListener;
    use tokio_util::codec::Framed;

    fn status(protocol: Option<u32>, capabilities: bool) -> ResponseData {
        let mut value = serde_json::json!({"kind":"Status", "uptime_secs":0, "accounts":[], "total_messages":0, "daemon_version":"older-package", "daemon_build_id":"another-host-build", "workspace_owner_supported": capabilities, "future_optional_field":{"ignored":true}});
        if let Some(protocol) = protocol {
            value["protocol_version"] = protocol.into();
        }
        serde_json::from_value(value).unwrap()
    }
    async fn fixture(
        data: ResponseData,
    ) -> (SelectedDaemonTarget, tokio::task::JoinHandle<Vec<Request>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = SelectedDaemonTarget::new(
            TransportAddr::Tcp(listener.local_addr().unwrap()),
            Some("fixture-token".into()),
        );
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut framed = Framed::new(stream, IpcCodec::new());
            let mut requests = Vec::new();
            while let Some(Ok(message)) = framed.next().await {
                let IpcPayload::Request(request) = message.payload else {
                    panic!("request")
                };
                let response = match &request {
                    Request::Authenticate { token } => {
                        assert_eq!(token, "fixture-token");
                        ResponseData::Authenticated
                    }
                    Request::GetStatus => data.clone(),
                    Request::Ping => ResponseData::Pong,
                    _ => panic!("unexpected business request {request:?}"),
                };
                requests.push(request);
                framed
                    .send(IpcMessage {
                        expected_workspace: None,
                        id: message.id,
                        source: ClientKind::Daemon,
                        payload: IpcPayload::Response(Response::Ok { data: response }),
                    })
                    .await
                    .unwrap();
            }
            requests
        });
        (target, task)
    }

    #[tokio::test]
    async fn authenticates_first_accepts_equal_protocol_build_skew_and_blocks_account_setup() {
        let (target, server) = fixture(status(Some(IPC_PROTOCOL_VERSION), true)).await;
        assert!(!format!("{target:?}").contains("fixture-token"));
        let mut connection = target.connect(ClientKind::Cli).await.unwrap();
        let error = connection
            .request(Request::SetDefaultAccount {
                key: "secret-account".into(),
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unsupported"));
        assert!(matches!(
            connection.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
        drop(connection);
        let requests = server.await.unwrap();
        assert!(matches!(
            requests.as_slice(),
            [
                Request::Authenticate { .. },
                Request::GetStatus,
                Request::Ping
            ]
        ));
    }

    #[tokio::test]
    async fn missing_older_and_newer_protocol_fail_before_business() {
        for protocol in [
            None,
            Some(IPC_PROTOCOL_VERSION - 1),
            Some(IPC_PROTOCOL_VERSION + 1),
        ] {
            let (target, server) = fixture(status(protocol, true)).await;
            let error = target.connect(ClientKind::Mcp).await.err().unwrap();
            assert!(
                error
                    .to_string()
                    .contains(&format!("supports protocol {IPC_PROTOCOL_VERSION}")),
                "{error}"
            );
            assert!(matches!(
                server.await.unwrap().as_slice(),
                [Request::Authenticate { .. }, Request::GetStatus]
            ));
        }
    }

    #[tokio::test]
    async fn malformed_status_is_rejected_and_missing_capability_cannot_capture_owner() {
        let (target, server) = fixture(ResponseData::Pong).await;
        assert!(target
            .connect(ClientKind::Tui)
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("unexpected status"));
        assert_eq!(server.await.unwrap().len(), 2);
        let (target, server) = fixture(status(Some(IPC_PROTOCOL_VERSION), false)).await;
        let mut connection = target.connect(ClientKind::Tui).await.unwrap();
        assert!(connection
            .capture_workspace()
            .await
            .unwrap_err()
            .to_string()
            .contains("workspace"));
        drop(connection);
        assert!(server
            .await
            .unwrap()
            .iter()
            .all(|request| matches!(request, Request::Authenticate { .. } | Request::GetStatus)));
    }

    #[tokio::test]
    async fn complete_handshake_timeout_includes_unanswered_authentication() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = SelectedDaemonTarget::new(
            TransportAddr::Tcp(listener.local_addr().unwrap()),
            Some("fixture-token".into()),
        );
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut framed = Framed::new(stream, IpcCodec::new());
            let message = framed.next().await.unwrap().unwrap();
            assert!(matches!(
                message.payload,
                IpcPayload::Request(Request::Authenticate { .. })
            ));
            assert!(framed.next().await.is_none());
        });
        let error = target.connect(ClientKind::Cli).await.err().unwrap();
        assert!(error.to_string().contains("handshake timed out"), "{error}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn timed_out_cmd_handshake_drops_owned_dialer() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("pid");
        let target = SelectedDaemonTarget::new(
            TransportAddr::Cmd(vec![
                "/bin/sh".into(),
                "-c".into(),
                "echo $$ > \"$1\"; exec sleep 60".into(),
                "fixture".into(),
                pid_path.display().to_string(),
            ]),
            None,
        );
        let error = target.connect(ClientKind::Cli).await.err().unwrap();
        assert!(error.to_string().contains("handshake timed out"));
        let pid = std::fs::read_to_string(pid_path).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let running = std::process::Command::new("/bin/kill")
                    .args(["-0", pid.trim()])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .unwrap()
                    .success();
                if !running {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn unavailable_cmd_does_not_surface_private_arguments() {
        let target = SelectedDaemonTarget::new(
            TransportAddr::Cmd(vec![
                "/missing-mxr-fixture-program".into(),
                "PRIVATE_ARG_SENTINEL".into(),
            ]),
            None,
        );
        let error = target
            .connect(ClientKind::Cli)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("selected cmd://"));
        assert!(!error.contains("PRIVATE_ARG_SENTINEL"));
        assert!(!error.contains("missing-mxr-fixture-program"));
    }

    #[test]
    fn token_lookup_is_lazy_and_target_debug_hides_command_arguments() {
        let target = SelectedDaemonTarget::from(PathBuf::from("/not-local.sock"));
        assert!(!target.is_remote());
        let command = SelectedDaemonTarget::new(
            TransportAddr::Cmd(vec!["secret-wrapper".into(), "private-argument".into()]),
            None,
        );
        assert!(!format!("{command:?}").contains("private-argument"));
    }
}
