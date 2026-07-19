use mxr_client::{ClientError, IpcConnection};
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::*;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::warn;

pub struct Client {
    conn: IpcConnection,
    event_tx: Option<mpsc::UnboundedSender<DaemonEvent>>,
}

impl Client {
    pub async fn connect(socket_path: &Path) -> std::io::Result<Self> {
        // Surface the raw `io::Error` so the worker's autostart classifier
        // (`should_autostart_daemon`) can inspect its `ErrorKind`.
        let conn = IpcConnection::connect(socket_path, ClientKind::Tui)
            .await
            .map_err(|error| match error {
                ClientError::Connect { source, .. } => source,
                other => std::io::Error::other(other.to_string()),
            })?;
        Ok(Self {
            conn,
            event_tx: None,
        })
    }

    pub fn with_event_channel(mut self, tx: mpsc::UnboundedSender<DaemonEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub async fn raw_request(&mut self, req: Request) -> Result<Response, MxrError> {
        self.request(req).await
    }

    async fn request(&mut self, req: Request) -> Result<Response, MxrError> {
        // No connection-level timeout: the IPC worker bounds every request
        // (see `ipc::IPC_REQUEST_TIMEOUT`) and owns the reconnect-on-timeout
        // policy, so the mechanism here stays a plain awaited exchange.
        let event_tx = self.event_tx.clone();
        self.conn
            .request_response(
                req,
                |event| {
                    if let Some(ref tx) = event_tx {
                        let _ = tx.send(event);
                    }
                },
                None,
            )
            .await
            .map_err(map_request_error)
    }

    pub async fn list_envelopes(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, MxrError> {
        let resp = self
            .request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit,
                offset,
            })
            .await?;

        match resp {
            Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } => Ok(envelopes),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_labels(&mut self) -> Result<Vec<Label>, MxrError> {
        let resp = self
            .request(Request::ListLabels { account_id: None })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Labels { labels },
            } => Ok(labels),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Phase F: paginated thread list. Provides the TUI's read path
    /// for `Request::ListThreads`. The TUI's existing mailbox screen
    /// renders messages grouped client-side by `thread_id`; a future
    /// dedicated thread-pane view can call this to render threads
    /// directly (each `Thread` carries its `message_ids`).
    pub async fn list_threads(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<mxr_core::types::Thread>, MxrError> {
        let resp = self
            .request(Request::ListThreads {
                account_id: None,
                label_id: None,
                limit,
                offset,
                sort: None,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Threads { threads },
            } => Ok(threads),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn search(
        &mut self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SearchResultItem>, MxrError> {
        let resp = self
            .request(Request::Search {
                query: query.to_string(),
                limit,
                offset: 0,
                account_id: None,
                mode: None,
                sort: Some(mxr_core::types::SortOrder::DateDesc),
                explain: false,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            } => Ok(results),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_body(&mut self, message_id: &MessageId) -> Result<MessageBody, MxrError> {
        let resp = self
            .request(Request::GetBody {
                message_id: message_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Body { body },
            } => Ok(body),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn get_thread(
        &mut self,
        thread_id: &ThreadId,
    ) -> Result<(Thread, Vec<Envelope>), MxrError> {
        let resp = self
            .request(Request::GetThread {
                thread_id: thread_id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data:
                    ResponseData::Thread {
                        thread, messages, ..
                    },
            } => Ok((thread, messages)),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_saved_searches(
        &mut self,
    ) -> Result<Vec<mxr_core::types::SavedSearch>, MxrError> {
        let resp = self.request(Request::ListSavedSearches).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SavedSearches { searches },
            } => Ok(searches),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    pub async fn list_subscriptions(
        &mut self,
        limit: u32,
    ) -> Result<Vec<mxr_core::types::SubscriptionSummary>, MxrError> {
        let resp = self
            .request(Request::ListSubscriptions {
                account_id: None,
                limit,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Subscriptions { subscriptions },
            } => Ok(subscriptions),
            Response::Error { message, .. } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Read one frame while no request is in flight, forwarding events.
    /// Returns `Err` when the connection is closed or broken so the caller can
    /// trigger its reconnect path. Cancel-safe: dropping the future leaves
    /// any partial frame buffered inside `framed`.
    pub(crate) async fn read_idle_frame(&mut self) -> Result<(), MxrError> {
        match self.conn.next_event().await {
            Ok(msg) => {
                match msg.payload {
                    IpcPayload::Event(event) => {
                        if let Some(ref tx) = self.event_tx {
                            let _ = tx.send(event);
                        }
                    }
                    other => {
                        warn!(?other, "unexpected idle IPC frame; dropping");
                    }
                }
                Ok(())
            }
            Err(ClientError::Closed) => Err(MxrError::Ipc("connection closed".into())),
            Err(ClientError::Io(source)) => {
                Err(MxrError::Ipc(describe_ipc_failure(&source.to_string())))
            }
            Err(other) => Err(map_request_error(other)),
        }
    }

    pub async fn ping(&mut self) -> Result<(), MxrError> {
        let resp = self.request(Request::Ping).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Pong,
            } => Ok(()),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }
}

/// Map a `mxr-client` failure onto the `MxrError::Ipc` strings the TUI worker
/// classifies on (`should_reconnect_ipc` matches substrings such as
/// "connection closed" / "broken pipe" / "connection reset"). The `Io` arm
/// keeps the protocol-mismatch hint via [`describe_ipc_failure`], which is
/// otherwise a pass-through so those substrings survive.
fn map_request_error(error: ClientError) -> MxrError {
    match error {
        ClientError::Closed => MxrError::Ipc(
            "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading.".into(),
        ),
        ClientError::Io(source) => MxrError::Ipc(describe_ipc_failure(&source.to_string())),
        ClientError::Daemon { message, .. } => MxrError::Ipc(message),
        // Intentional deviation: the old `request` loop silently skipped a
        // rogue frame and kept reading; we now surface it as an error. A frame
        // that does not correlate means the connection is out of step, and
        // skipping risks parking the worker; it is also unreachable in practice
        // (one in-flight request per connection, and the worker drops the
        // connection on timeout — see ipc.rs). Not a reconnect trigger.
        ClientError::UnexpectedFrame {
            frame_id,
            expected_id,
            ..
        } => MxrError::Ipc(format!(
            "IPC protocol error: unexpected frame id {frame_id} while awaiting response {expected_id}"
        )),
        ClientError::Timeout(duration) => MxrError::Ipc(format!(
            "IPC request timed out after {} seconds",
            duration.as_secs()
        )),
        ClientError::Connect { source, .. } => MxrError::Ipc(source.to_string()),
        // Transport-level connect failure (generic-connector path). The TUI
        // dials via the path constructor, so this is unreachable here; the arm
        // keeps the match exhaustive.
        ClientError::Transport(error) => MxrError::Ipc(error.to_string()),
    }
}

fn describe_ipc_failure(message: &str) -> String {
    if message.contains("unknown variant") || message.contains("missing field") {
        format!("IPC protocol mismatch: {message}. Restart the daemon after upgrading.")
    } else {
        message.to_string()
    }
}
