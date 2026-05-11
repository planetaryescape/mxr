use crate::state::AppState;
use futures::{SinkExt, StreamExt};
use mxr_protocol::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};
use tokio_util::codec::Framed;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

pub struct IpcClient {
    framed: Framed<UnixStream, IpcCodec>,
    next_id: AtomicU64,
}

impl IpcClient {
    pub async fn connect() -> anyhow::Result<Self> {
        Self::connect_to(&AppState::socket_path()).await
    }

    pub async fn connect_to(socket_path: &Path) -> anyhow::Result<Self> {
        let stream = UnixStream::connect(&socket_path).await.map_err(|e| {
            anyhow::anyhow!(
                "Cannot connect to daemon at {}: {}. Is the daemon running? Try: mxr daemon",
                socket_path.display(),
                e
            )
        })?;
        Ok(Self {
            framed: Framed::new(stream, IpcCodec::new()),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn request(&mut self, req: Request) -> anyhow::Result<Response> {
        self.request_inner(req, |_| {}, Some(DEFAULT_REQUEST_TIMEOUT))
            .await
    }

    /// Like [`request`], but invokes `on_event` for every
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
        mut on_event: F,
    ) -> anyhow::Result<Response>
    where
        F: FnMut(DaemonEvent),
    {
        self.request_inner(req, &mut on_event, None).await
    }

    async fn request_inner<F>(
        &mut self,
        req: Request,
        mut on_event: F,
        request_timeout: Option<Duration>,
    ) -> anyhow::Result<Response>
    where
        F: FnMut(DaemonEvent),
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = IpcMessage {
            id,
            payload: IpcPayload::Request(req),
        };
        self.framed.send(msg).await?;

        let wait_for_response = async {
            loop {
                match self.framed.next().await {
                Some(Ok(resp_msg)) => match resp_msg.payload {
                    IpcPayload::Response(resp) if resp_msg.id == id => return Ok(resp),
                    IpcPayload::Event(event) => on_event(event),
                    IpcPayload::Response(_) => anyhow::bail!(
                        "IPC protocol error: received response id {} while waiting for {id}",
                        resp_msg.id
                    ),
                    _ => anyhow::bail!(
                        "IPC protocol error: received non-response frame while waiting for response {id}"
                    ),
                },
                Some(Err(e)) => anyhow::bail!("{}", describe_ipc_failure(&e.to_string())),
                None => anyhow::bail!(
                    "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading."
                ),
            }
            }
        };

        if let Some(duration) = request_timeout {
            timeout(duration, wait_for_response).await.map_err(|_| {
                anyhow::anyhow!("IPC request timed out after {} seconds", duration.as_secs())
            })?
        } else {
            wait_for_response.await
        }
    }

    pub async fn notify(&mut self, req: Request) -> anyhow::Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = IpcMessage {
            id,
            payload: IpcPayload::Request(req),
        };
        self.framed.send(msg).await?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> anyhow::Result<DaemonEvent> {
        loop {
            match self.framed.next().await {
                Some(Ok(msg)) => {
                    if let IpcPayload::Event(event) = msg.payload {
                        return Ok(event);
                    }
                }
                Some(Err(e)) => anyhow::bail!("{}", describe_ipc_failure(&e.to_string())),
                None => anyhow::bail!(
                    "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading."
                ),
            }
        }
    }
}

fn describe_ipc_failure(message: &str) -> String {
    if message.contains("unknown variant") || message.contains("missing field") {
        format!("IPC protocol mismatch: {message}. Restart the daemon after upgrading.")
    } else {
        format!("IPC error: {message}")
    }
}
