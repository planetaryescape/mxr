use mxr_client::{ClientError, IpcConnection};
use mxr_protocol::*;
use std::path::Path;
use tokio::time::Duration;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Wall-clock cap for ordinary (short) requests: status polls, list/read
/// calls, single mutations.
///
/// Long-running operations must not use this path at all — they go through
/// [`IpcClient::request_with_events`], which has no deadline and streams
/// progress instead — no value of `MXR_IPC_TIMEOUT_SECS` bounds those, so
/// Ctrl-C is the only way out of a long one.
///
/// `MXR_IPC_TIMEOUT_SECS` is the escape hatch for a host where even a short
/// call needs longer (or `0` to wait indefinitely); an unparseable value warns
/// and falls back to the default rather than failing the call.
fn request_timeout() -> Option<Duration> {
    // Resolved once: the value cannot change usefully mid-process, and every
    // request would otherwise pay for a `getenv` plus a parse. `mxr demo` sets
    // its own env before any request goes out, so first-use resolution is
    // late enough.
    static TIMEOUT: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *TIMEOUT.get_or_init(|| {
        let Ok(raw) = std::env::var("MXR_IPC_TIMEOUT_SECS") else {
            return Some(DEFAULT_REQUEST_TIMEOUT);
        };
        match raw.trim().parse::<u64>() {
            Ok(0) => None,
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => {
                eprintln!(
                    "Ignoring MXR_IPC_TIMEOUT_SECS={raw}: expected whole seconds (0 waits indefinitely). Using {}s.",
                    DEFAULT_REQUEST_TIMEOUT.as_secs()
                );
                Some(DEFAULT_REQUEST_TIMEOUT)
            }
        }
    })
}

/// CLI / daemon-internal IPC client.
///
/// A thin facade over [`mxr_client::IpcConnection`] that preserves this crate's
/// long-standing `IpcClient` surface — the shape ~58 command files call. The
/// connection mechanism (connect, frame, correlate, event forwarding, timeout)
/// lives in `mxr-client`; this type only maps that crate's [`ClientError`] onto
/// the `anyhow` messages the CLI has always produced (including the
/// protocol-mismatch guidance).
pub struct IpcClient {
    conn: IpcConnection,
    workspace: Option<mxr_protocol::ArchiveWorkspaceData>,
}

impl IpcClient {
    pub async fn transfer_attachment_to(
        &mut self,
        message_id: &mxr_core::MessageId,
        attachment_id: &mxr_core::AttachmentId,
        destination: &Path,
        replace_existing: bool,
    ) -> anyhow::Result<mxr_client::DownloadedAttachment> {
        mxr_client::download_attachment_to(
            &mut self.conn,
            message_id,
            attachment_id,
            destination,
            replace_existing,
        )
        .await
        .map_err(map_request_error)
    }

    pub async fn connect() -> anyhow::Result<Self> {
        // Build the connector from MXR_DAEMON_ADDR (unix:// default, tcp://
        // loopback+token, or cmd:// spawn-and-pipe) so the request path agrees
        // with autostart / the liveness probe / doctor. A tcp:// connector
        // authenticates automatically inside `connect_with` when it has a token.
        let is_cmd = matches!(
            crate::server::resolve_daemon_addr()?,
            mxr_transport::TransportAddr::Cmd(_)
        );
        let connector = crate::server::build_cli_connector()?;
        let conn = IpcConnection::connect_with(connector.as_ref(), ClientKind::Cli)
            .await
            .map_err(|error| {
                if is_cmd {
                    anyhow::anyhow!("Cannot connect through the selected cmd:// daemon bridge. Check that the bridge command is available and speaks mxr IPC.")
                } else {
                    map_connect_error(error)
                }
            })?
            .with_default_timeout(request_timeout());
        Ok(Self { conn, workspace: None })
    }

    pub async fn connect_to(socket_path: &Path) -> anyhow::Result<Self> {
        let conn = IpcConnection::connect(socket_path, ClientKind::Cli)
            .await
            .map_err(|error| match error {
                ClientError::Connect { path, source } => anyhow::anyhow!(
                    "Cannot connect to daemon at {}: {}. Is the daemon running? Try: mxr daemon",
                    path.display(),
                    source
                ),
                other => anyhow::Error::new(other),
            })?
            .with_default_timeout(request_timeout());
        Ok(Self { conn, workspace: None })
    }

    pub async fn capture_workspace(&mut self) -> anyhow::Result<mxr_protocol::ArchiveWorkspaceData> {
        if let Some(owner) = &self.workspace { return Ok(owner.clone()); }
        let owner = self.conn.capture_workspace().await.map_err(map_request_error)?;
        self.workspace = Some(owner.clone());
        Ok(owner)
    }

    pub async fn expect_workspace(&mut self, owner: mxr_protocol::ArchiveWorkspaceData) -> anyhow::Result<()> {
        self.conn.verify_workspace(&owner).await.map_err(map_request_error)?;
        self.workspace = Some(owner);
        Ok(())
    }

    pub async fn request(&mut self, req: Request) -> anyhow::Result<Response> {
        let owner = if req.requires_workspace_owner() { Some(self.capture_workspace().await?) } else { self.workspace.clone() };
        self.conn
            .request_response_owned(req, owner.as_ref(), |_| {}, request_timeout())
            .await
            .map(crate::output::sanitize_response)
            .map_err(map_request_error)
    }

    /// Like [`Self::request`], but invokes `on_event` for every
    /// `DaemonEvent` frame that arrives on the connection while
    /// waiting for the response. Use for long-running operations
    /// (sync, rebuild-analytics, reindex) where the daemon emits
    /// `OperationProgress` events the user wants to see live.
    ///
    /// `on_event` runs synchronously between frame reads — keep it
    /// fast (write to stdout/stderr or push into a channel; don't
    /// block).
    pub async fn request_with_events<F>(
        &mut self,
        req: Request,
        on_event: F,
    ) -> anyhow::Result<Response>
    where
        F: FnMut(DaemonEvent),
    {
        let owner = if req.requires_workspace_owner() { Some(self.capture_workspace().await?) } else { self.workspace.clone() };
        self.conn
            .request_response_owned(req, owner.as_ref(), on_event, None)
            .await
            .map(crate::output::sanitize_response)
            .map_err(map_request_error)
    }

    pub async fn preview_archive(
        &mut self,
        mutation: MutationCommand,
    ) -> anyhow::Result<ArchiveOperationData> {
        let journal = mxr_client::ArchiveJournal::new(&mxr_config::data_dir())?;
        Ok(self.conn.preview_archive(&journal, mutation).await?)
    }
    pub async fn submit_archive(
        &mut self,
        operation: &ArchiveOperationData,
        wait: bool,
    ) -> anyhow::Result<ArchiveOperationData> {
        let journal = mxr_client::ArchiveJournal::new(&mxr_config::data_dir())?;
        let accepted = self.conn.submit_archive(&journal, operation).await?;
        if wait {
            Ok(self.conn.wait_archive(accepted.identity).await?)
        } else {
            Ok(accepted)
        }
    }

    pub async fn notify(&mut self, req: Request) -> anyhow::Result<()> {
        self.conn.notify(req).await.map_err(map_request_error)
    }

    pub async fn next_event(&mut self) -> anyhow::Result<DaemonEvent> {
        loop {
            let message = self.conn.next_event().await.map_err(map_request_error)?;
            if let IpcPayload::Event(event) = message.payload {
                return Ok(event);
            }
        }
    }
}

/// Map a connect-time failure (over any transport) onto CLI-friendly guidance.
/// A `unix://` connect failure keeps the "Is the daemon running?" hint; a
/// `tcp://` auth rejection points at the token; everything else falls back to
/// the request-error mapper.
fn map_connect_error(error: ClientError) -> anyhow::Error {
    match error {
        ClientError::Connect { path, source } => anyhow::anyhow!(
            "Cannot connect to daemon at {}: {}. Is the daemon running? Try: mxr daemon",
            path.display(),
            source
        ),
        ClientError::Transport(source) => anyhow::anyhow!(
            "{source}. Is the daemon running and reachable at MXR_DAEMON_ADDR?"
        ),
        ClientError::Daemon {
            kind: IpcErrorKind::Auth,
            message,
            ..
        } => anyhow::anyhow!(
            "Daemon authentication failed: {message}. Set MXR_DAEMON_TOKEN (or check the daemon token file)."
        ),
        other => map_request_error(other),
    }
}

/// Map a `mxr-client` failure onto the `anyhow` messages the CLI has always
/// surfaced. Framing/decode errors keep the protocol-mismatch hint via
/// [`describe_ipc_failure`]; a clean close keeps the upgrade-and-restart
/// guidance.
fn map_request_error(error: ClientError) -> anyhow::Error {
    match error {
        ClientError::Closed => anyhow::anyhow!(
            "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading."
        ),
        ClientError::Timeout(duration) => {
            anyhow::anyhow!("IPC request timed out after {} seconds", duration.as_secs())
        }
        ClientError::Io(source) => anyhow::anyhow!("{}", describe_ipc_failure(&source.to_string())),
        // Recreate the exact pre-refactor protocol-error strings.
        ClientError::UnexpectedFrame {
            frame_id,
            expected_id,
            is_response: true,
        } => anyhow::anyhow!(
            "IPC protocol error: received response id {frame_id} while waiting for {expected_id}"
        ),
        ClientError::UnexpectedFrame { expected_id, .. } => anyhow::anyhow!(
            "IPC protocol error: received non-response frame while waiting for response {expected_id}"
        ),
        other => anyhow::Error::new(other),
    }
}

fn describe_ipc_failure(message: &str) -> String {
    if message.contains("unknown variant") || message.contains("missing field") {
        format!("IPC protocol mismatch: {message}. Restart the daemon after upgrading.")
    } else {
        format!("IPC error: {message}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHILD_TEST: &str = "ipc_client::tests::archive_request_uses_cli_connect_timeout";
    const CHILD_MODE: &str = "MXR_IPC_TIMEOUT_SUBPROCESS";

    #[tokio::test]
    async fn archive_request_uses_cli_connect_timeout() {
        if std::env::var_os(CHILD_MODE).is_none() {
            let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
                .args(["--exact", CHILD_TEST, "--nocapture"])
                .env(CHILD_MODE, "1")
                .env("MXR_IPC_TIMEOUT_SECS", "1")
                .output()
                .expect("run timeout test in an isolated process");
            assert!(
                output.status.success(),
                "isolated timeout test failed\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let temp = tempfile::tempdir().expect("temporary IPC directory");
        std::env::set_var("MXR_DATA_DIR", temp.path());
        let socket = temp.path().join("silent-daemon.sock");
        std::env::set_var("MXR_DAEMON_ADDR", format!("unix://{}", socket.display()));
        let listener = tokio::net::UnixListener::bind(&socket).expect("bind silent daemon");
        let peer = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept CLI connection");
            tokio::time::sleep(Duration::from_secs(2)).await;
            drop(stream);
        });

        let mut client = IpcClient::connect().await.expect("connect CLI client");
        let error = client
            .preview_archive(MutationCommand::Archive {
                message_ids: Vec::new(),
            })
            .await
            .expect_err("silent daemon must hit the CLI request timeout");
        assert!(
            error.to_string().contains("timed out after 1 seconds"),
            "unexpected error: {error:#}"
        );
        peer.abort();
    }
}
