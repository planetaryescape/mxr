//! Shared IPC client for the mxr daemon.
//!
//! Every mxr client — the CLI/daemon-internal `IpcClient`, the TUI worker, the
//! web bridge, and the MCP server — needs the same three things: open the
//! daemon's Unix socket, send a length-delimited [`Request`] frame, and read
//! the correlated [`Response`] while forwarding any interleaved
//! [`DaemonEvent`]s. This crate is the single implementation of that
//! mechanism. Reconnect loops, autostart, retry classification, and per-client
//! error shaping stay *policy* owned by each consumer; this crate only owns the
//! connection.
//!
//! The transport is still a `UnixStream` — concentrating the four copies here
//! is the point, not abstracting the byte stream. [`IpcConnection::connect`] is
//! the seam a later transport-adapter phase can open.
//!
//! ```no_run
//! # async fn demo() -> Result<(), mxr_client::ClientError> {
//! use mxr_client::IpcConnection;
//! use mxr_protocol::{ClientKind, Request};
//! use std::path::Path;
//!
//! let mut conn = IpcConnection::connect(Path::new("/tmp/mxr.sock"), ClientKind::Cli).await?;
//! let data = conn.request(Request::Ping).await?;
//! # let _ = data;
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use mxr_protocol::{
    ClientKind, DaemonEvent, IpcCodec, IpcErrorKind, IpcMessage, IpcPayload, Request, Response,
    ResponseData,
};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

/// One vocabulary for every way a daemon exchange can fail.
///
/// Consumers map these onto their own error surfaces (the TUI's retry
/// classifier, the bridge's HTTP status mapping, the CLI's `anyhow` messages)
/// but the failure *classes* are shared: a connect failure, a socket I/O or
/// framing error, a kinded daemon-side error, a timeout, or a clean close.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Establishing the connection to the daemon socket failed.
    #[error("cannot connect to daemon at {path}: {source}")]
    Connect {
        /// The socket path that could not be reached.
        path: PathBuf,
        /// The underlying connect error (its [`std::io::ErrorKind`] drives
        /// autostart decisions in policy code).
        #[source]
        source: std::io::Error,
    },

    /// An I/O or framing error occurred sending or receiving a frame. The
    /// [`IpcCodec`] surfaces a malformed/undecodable frame as
    /// [`std::io::ErrorKind::InvalidData`], so protocol-version mismatches
    /// arrive here too.
    #[error("{0}")]
    Io(#[from] std::io::Error),

    /// The daemon answered with a [`Response::Error`]. Only produced by the
    /// [`IpcConnection::request`] / [`IpcConnection::request_with_events`]
    /// helpers that unwrap to [`ResponseData`]; callers wanting the error
    /// response verbatim use [`IpcConnection::request_response`].
    #[error("{message}")]
    Daemon {
        /// Human-readable daemon error message.
        message: String,
        /// Machine-readable failure class.
        kind: IpcErrorKind,
        /// Whether the daemon flagged the failure as retryable.
        retryable: bool,
    },

    /// A frame arrived that does not correlate to the in-flight request: a
    /// `Response` carrying a different id, or a non-response frame. This is
    /// fail-fast — at this point the framed stream is out of step with the
    /// request/response sequence, so the connection is unusable. Consumers that
    /// historically skipped such frames now surface this (a documented, and in
    /// practice unreachable, deviation); each maps it back onto its own error
    /// surface via the `frame_id`/`expected_id`/`is_response` fields.
    #[error("unexpected IPC frame (id {frame_id}) while awaiting response {expected_id}")]
    UnexpectedFrame {
        /// Id carried by the rogue frame.
        frame_id: u64,
        /// Id of the request still awaiting its response.
        expected_id: u64,
        /// True when the rogue frame was itself a `Response` (wrong id); false
        /// for a non-response frame.
        is_response: bool,
    },

    /// The request exceeded its configured timeout without a response.
    #[error("IPC request timed out after {} seconds", .0.as_secs())]
    Timeout(Duration),

    /// The connection closed before the response arrived.
    #[error("connection closed")]
    Closed,
}

/// A single framed connection to the daemon over its Unix socket.
///
/// Requests sent on one connection are correlated to their responses by an
/// internal monotonic id; interleaved [`DaemonEvent`] frames (id `0`) are
/// surfaced to the caller's callback or via [`Self::next_event`]. A connection
/// carries exactly one in-flight request at a time — the model every mxr client
/// already uses.
pub struct IpcConnection {
    framed: Framed<UnixStream, IpcCodec>,
    next_id: AtomicU64,
    source: ClientKind,
    default_timeout: Option<Duration>,
}

impl IpcConnection {
    /// Connect to the daemon at `path`, tagging every request with `source`.
    ///
    /// The `source` tag rides on each [`IpcMessage`] so the daemon's activity
    /// recorder attributes traffic to the right surface — making the class of
    /// bug where a client mislabels itself structurally impossible.
    pub async fn connect(path: &Path, source: ClientKind) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|error| ClientError::Connect {
                path: path.to_path_buf(),
                source: error,
            })?;
        Ok(Self {
            framed: Framed::new(stream, IpcCodec::new()),
            next_id: AtomicU64::new(1),
            source,
            default_timeout: None,
        })
    }

    /// Set the timeout applied by [`Self::request`] (per-connection default).
    /// `None` — the default — means no timeout is applied by the connection
    /// itself.
    #[must_use]
    pub fn with_default_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Seed the correlation-id counter. Connection-per-request callers that
    /// carry an externally-chosen request id (e.g. the web bridge's log/wire
    /// correlation id) use this so the id on the wire matches.
    #[must_use]
    pub fn with_start_id(mut self, start: u64) -> Self {
        self.next_id = AtomicU64::new(start);
        self
    }

    fn take_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn envelope(&self, id: u64, request: Request) -> IpcMessage {
        IpcMessage {
            id,
            source: self.source,
            payload: IpcPayload::Request(request),
        }
    }

    /// Send `request` and return the daemon's correlated [`Response`] verbatim
    /// (both `Ok` and `Error` variants), forwarding any interleaved
    /// [`DaemonEvent`] frames to `on_event`. Applies `request_timeout` when
    /// `Some`.
    ///
    /// `on_event` runs synchronously between frame reads — keep it fast (write
    /// to a stream or push into a channel; never block).
    ///
    /// This is the low-level primitive: connection, timeout, and framing
    /// failures map to [`ClientError`], but a `Response::Error` from the daemon
    /// is returned as `Ok(Response::Error { .. })` so callers can inspect its
    /// full envelope. Use [`Self::request`] for the [`ResponseData`]-or-
    /// [`ClientError::Daemon`] shape.
    pub async fn request_response<F>(
        &mut self,
        request: Request,
        mut on_event: F,
        request_timeout: Option<Duration>,
    ) -> Result<Response, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        let id = self.take_id();
        let message = self.envelope(id, request);
        self.framed.send(message).await?;

        let wait_for_response = async {
            loop {
                match self.framed.next().await {
                    Some(Ok(frame)) => match frame.payload {
                        IpcPayload::Response(response) if frame.id == id => return Ok(response),
                        IpcPayload::Event(event) => on_event(event),
                        IpcPayload::Response(_) => {
                            return Err(ClientError::UnexpectedFrame {
                                frame_id: frame.id,
                                expected_id: id,
                                is_response: true,
                            })
                        }
                        _ => {
                            return Err(ClientError::UnexpectedFrame {
                                frame_id: frame.id,
                                expected_id: id,
                                is_response: false,
                            })
                        }
                    },
                    Some(Err(error)) => return Err(ClientError::Io(error)),
                    None => return Err(ClientError::Closed),
                }
            }
        };

        match request_timeout {
            Some(duration) => match tokio::time::timeout(duration, wait_for_response).await {
                Ok(result) => result,
                Err(_) => Err(ClientError::Timeout(duration)),
            },
            None => wait_for_response.await,
        }
    }

    /// Send `request` and return its [`ResponseData`] payload, mapping a
    /// `Response::Error` to [`ClientError::Daemon`]. Applies the connection's
    /// [default timeout](Self::with_default_timeout).
    pub async fn request(&mut self, request: Request) -> Result<ResponseData, ClientError> {
        let request_timeout = self.default_timeout;
        into_data(
            self.request_response(request, drop_event, request_timeout)
                .await?,
        )
    }

    /// Like [`Self::request`], but forwards interleaved [`DaemonEvent`] frames
    /// to `on_event` — the long-running-operation progress pattern (sync,
    /// reindex, rebuild). No timeout is applied.
    pub async fn request_with_events<F>(
        &mut self,
        request: Request,
        on_event: F,
    ) -> Result<ResponseData, ClientError>
    where
        F: FnMut(DaemonEvent),
    {
        into_data(self.request_response(request, on_event, None).await?)
    }

    /// Fire-and-forget: send `request` without awaiting a response.
    pub async fn notify(&mut self, request: Request) -> Result<(), ClientError> {
        let id = self.take_id();
        let message = self.envelope(id, request);
        self.framed.send(message).await?;
        Ok(())
    }

    /// Read the next frame from the connection (a [`Response`] or an
    /// unsolicited [`DaemonEvent`], surfaced as the raw [`IpcMessage`]).
    ///
    /// Cancel-safe: dropping the returned future leaves any partially-read
    /// frame buffered inside the connection, so it is safe to use as a
    /// `tokio::select!` branch.
    pub async fn next_event(&mut self) -> Result<IpcMessage, ClientError> {
        match self.framed.next().await {
            Some(Ok(message)) => Ok(message),
            Some(Err(error)) => Err(ClientError::Io(error)),
            None => Err(ClientError::Closed),
        }
    }
}

fn drop_event(_event: DaemonEvent) {}

fn into_data(response: Response) -> Result<ResponseData, ClientError> {
    match response {
        Response::Ok { data } => Ok(data),
        Response::Error {
            message,
            kind,
            retryable,
            ..
        } => Err(ClientError::Daemon {
            message,
            kind,
            retryable,
        }),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests assert directly on fixtures"
    )]

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tokio::net::UnixListener;

    /// A minimal daemon stand-in that speaks the real [`IpcCodec`] over a Unix
    /// socket. `responder` maps a decoded [`Request`] to an optional
    /// [`Response`] (return `None` to stay silent — e.g. to exercise the
    /// timeout path); `preamble` frames are pushed to every accepted
    /// connection before any request is read (used for event-stream tests).
    async fn spawn_fake_daemon<F>(
        socket_path: &Path,
        preamble: Vec<IpcMessage>,
        responder: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: Fn(&IpcMessage) -> Option<Response> + Send + Sync + 'static,
    {
        let responder = Arc::new(responder);
        let listener = UnixListener::bind(socket_path).unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let responder = responder.clone();
                let preamble = preamble.clone();
                tokio::spawn(async move {
                    let mut framed = Framed::new(stream, IpcCodec::new());
                    for frame in preamble {
                        if framed.send(frame).await.is_err() {
                            return;
                        }
                    }
                    while let Some(Ok(message)) = framed.next().await {
                        if let Some(response) = responder(&message) {
                            let reply = IpcMessage {
                                id: message.id,
                                source: ClientKind::Daemon,
                                payload: IpcPayload::Response(response),
                            };
                            if framed.send(reply).await.is_err() {
                                return;
                            }
                        }
                    }
                });
            }
        })
    }

    fn event_frame(event: DaemonEvent) -> IpcMessage {
        IpcMessage {
            id: 0,
            source: ClientKind::Daemon,
            payload: IpcPayload::Event(event),
        }
    }

    fn sync_completed() -> DaemonEvent {
        DaemonEvent::SyncCompleted {
            account_id: mxr_core::id::AccountId::new(),
            messages_synced: 3,
        }
    }

    #[tokio::test]
    async fn request_correlates_response_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |message| match &message.payload {
            IpcPayload::Request(Request::Ping) => Some(Response::Ok {
                data: ResponseData::Pong,
            }),
            _ => None,
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        // Two sequential requests: each must receive its own correlated reply.
        assert!(matches!(
            conn.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
        assert!(matches!(
            conn.request(Request::Ping).await.unwrap(),
            ResponseData::Pong
        ));
    }

    #[tokio::test]
    async fn tags_requests_with_the_configured_client_kind() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            seen_writer.lock().unwrap().push(message.source);
            Some(Response::Ok {
                data: ResponseData::Pong,
            })
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Web)
            .await
            .unwrap();
        conn.request(Request::Ping).await.unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), &[ClientKind::Web]);
    }

    #[tokio::test]
    async fn with_start_id_puts_exact_id_on_the_wire() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            seen_writer.lock().unwrap().push(message.id);
            Some(Response::Ok {
                data: ResponseData::Pong,
            })
        })
        .await;

        // The web bridge's correlation seam: `with_start_id(N)` must put exactly
        // `N` on the wire so daemon logs line up with the bridge's request id.
        let mut conn = IpcConnection::connect(&sock, ClientKind::Web)
            .await
            .unwrap()
            .with_start_id(4242);
        conn.request(Request::Ping).await.unwrap();
        assert_eq!(seen.lock().unwrap().as_slice(), &[4242]);
    }

    #[tokio::test]
    async fn unexpected_response_id_is_a_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Reply with a mismatched correlation id (never happens against the real
        // daemon, but pins the typed fail-fast disposition).
        let listener = UnixListener::bind(&sock).unwrap();
        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let mut framed = Framed::new(stream, IpcCodec::new());
                if let Some(Ok(message)) = framed.next().await {
                    let _ = framed
                        .send(IpcMessage {
                            id: message.id.wrapping_add(100),
                            source: ClientKind::Daemon,
                            payload: IpcPayload::Response(Response::Ok {
                                data: ResponseData::Pong,
                            }),
                        })
                        .await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        match conn.request(Request::Ping).await {
            Err(ClientError::UnexpectedFrame {
                is_response,
                expected_id,
                frame_id,
            }) => {
                assert!(is_response);
                assert_eq!(expected_id, 1);
                assert_eq!(frame_id, 101);
            }
            other => panic!("expected UnexpectedFrame, got {other:?}"),
        }
        server.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn request_times_out_when_daemon_never_replies() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Accept and read forever, never replying.
        let _server = spawn_fake_daemon(&sock, Vec::new(), |_| None).await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap()
            .with_default_timeout(Some(Duration::from_secs(120)));
        let error = conn.request(Request::Ping).await.unwrap_err();
        assert!(
            matches!(error, ClientError::Timeout(d) if d == Duration::from_secs(120)),
            "unexpected error: {error:?}"
        );
    }

    #[tokio::test]
    async fn daemon_error_maps_to_kinded_client_error() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |_| {
            Some(Response::error_kinded(
                "mailbox not found",
                IpcErrorKind::NotFound,
            ))
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        let error = conn.request(Request::Ping).await.unwrap_err();
        match error {
            ClientError::Daemon {
                message,
                kind,
                retryable,
            } => {
                assert_eq!(message, "mailbox not found");
                assert_eq!(kind, IpcErrorKind::NotFound);
                assert!(!retryable);
            }
            other => panic!("expected Daemon error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn request_response_returns_error_variant_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, Vec::new(), |_| {
            Some(Response::error_kinded("boom", IpcErrorKind::Provider))
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Mcp)
            .await
            .unwrap();
        // The raw primitive does NOT collapse Response::Error into ClientError.
        let response = conn
            .request_response(Request::Ping, drop_event, None)
            .await
            .unwrap();
        match response {
            Response::Error {
                message,
                kind,
                retryable,
                code,
                ..
            } => {
                assert_eq!(message, "boom");
                assert_eq!(kind, IpcErrorKind::Provider);
                assert!(retryable, "Provider errors are retryable");
                assert_eq!(code, "provider");
            }
            other => panic!("expected verbatim Error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn events_interleaved_before_response_are_forwarded_then_response_returned() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Push two events, then reply to the request.
        let _server = spawn_fake_daemon(
            &sock,
            vec![event_frame(sync_completed()), event_frame(sync_completed())],
            |message| match &message.payload {
                IpcPayload::Request(Request::Ping) => Some(Response::Ok {
                    data: ResponseData::Pong,
                }),
                _ => None,
            },
        )
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        let count = Arc::new(AtomicU64::new(0));
        let sink = count.clone();
        let data = conn
            .request_with_events(Request::Ping, move |_event| {
                sink.fetch_add(1, Ordering::Relaxed);
            })
            .await
            .unwrap();
        assert!(matches!(data, ResponseData::Pong));
        assert_eq!(count.load(Ordering::Relaxed), 2, "both events forwarded");
    }

    #[tokio::test]
    async fn notify_sends_without_awaiting_a_response() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let writer = seen.clone();
        let _server = spawn_fake_daemon(&sock, Vec::new(), move |message| {
            if let IpcPayload::Request(request) = &message.payload {
                writer.lock().unwrap().push(request.clone());
            }
            // Deliberately never reply — notify must not block on a response.
            None
        })
        .await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Cli)
            .await
            .unwrap();
        conn.notify(Request::Ping).await.unwrap();

        // The server observes the request even though no response was sent.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if !seen.lock().unwrap().is_empty() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("server received the notify request");
    }

    #[tokio::test]
    async fn next_event_yields_unsolicited_event_frames() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        let _server = spawn_fake_daemon(&sock, vec![event_frame(sync_completed())], |_| None).await;

        let mut conn = IpcConnection::connect(&sock, ClientKind::Tui)
            .await
            .unwrap();
        let message = conn.next_event().await.unwrap();
        assert!(
            matches!(
                message.payload,
                IpcPayload::Event(DaemonEvent::SyncCompleted { .. })
            ),
            "expected a SyncCompleted event frame"
        );
    }

    #[tokio::test]
    async fn next_event_reports_closed_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("mxr.sock");
        // Accept, then immediately drop the connection (no preamble, no reply).
        let listener = UnixListener::bind(&sock).unwrap();
        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                drop(stream);
            }
        });

        let mut conn = IpcConnection::connect(&sock, ClientKind::Tui)
            .await
            .unwrap();
        let error = conn.next_event().await.unwrap_err();
        assert!(
            matches!(error, ClientError::Closed),
            "unexpected error: {error:?}"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_failure_surfaces_connect_error_with_io_kind() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.sock");
        match IpcConnection::connect(&missing, ClientKind::Cli).await {
            Err(ClientError::Connect { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            Err(other) => panic!("expected Connect error, got {other:?}"),
            Ok(_) => panic!("expected connect to a missing socket to fail"),
        }
    }
}
