use crate::mxr_protocol::*;
use crate::state::AppState;
use futures::{SinkExt, StreamExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

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
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = IpcMessage {
            id,
            payload: IpcPayload::Request(req),
        };
        self.framed.send(msg).await?;

        loop {
            match self.framed.next().await {
                Some(Ok(resp_msg)) => {
                    if resp_msg.id == id {
                        match resp_msg.payload {
                            IpcPayload::Response(resp) => return Ok(resp),
                            _ => continue,
                        }
                    }
                }
                Some(Err(e)) => anyhow::bail!("{}", describe_ipc_failure(&e.to_string())),
                None => anyhow::bail!(
                    "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading."
                ),
            }
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
