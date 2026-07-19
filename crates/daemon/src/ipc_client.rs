use mxr_client::{ClientError, IpcConnection};
use mxr_protocol::*;
use std::path::Path;
use tokio::time::Duration;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

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
}

impl IpcClient {
    pub async fn connect() -> anyhow::Result<Self> {
        // Build the connector from MXR_DAEMON_ADDR (unix:// default, tcp://
        // loopback+token, or cmd:// spawn-and-pipe) so the request path agrees
        // with autostart / the liveness probe / doctor. A tcp:// connector
        // authenticates automatically inside `connect_with` when it has a token.
        let connector = crate::server::build_cli_connector()?;
        let conn = IpcConnection::connect_with(connector.as_ref(), ClientKind::Cli)
            .await
            .map_err(map_connect_error)?;
        Ok(Self { conn })
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
            })?;
        Ok(Self { conn })
    }

    pub async fn request(&mut self, req: Request) -> anyhow::Result<Response> {
        self.conn
            .request_response(req, |_| {}, Some(DEFAULT_REQUEST_TIMEOUT))
            .await
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
        self.conn
            .request_response(req, on_event, None)
            .await
            .map_err(map_request_error)
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
