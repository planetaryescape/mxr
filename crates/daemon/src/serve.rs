#![cfg_attr(
    test,
    expect(
        clippy::panic,
        reason = "tests panic with diagnostic context for direct failures"
    )
)]
//! Generic serve core (Phase 3, transport-adapter initiative).
//!
//! The daemon's per-connection machinery, generic over the byte stream:
//! [`serve_client_connection`] is `<S: AsyncRead + AsyncWrite + Unpin + Send +
//! 'static>`, so a served connection is "anything that reads and writes bytes"
//! rather than a `UnixStream`. The Unix lifecycle — bind, permissions,
//! stale-socket handling, pid file, and the accept loop — stays in
//! [`crate::server`]; everything a live connection owns lives here: Hot/Bulk
//! lane routing, the per-request `JoinSet`, event subscription fan-out with
//! `EventsLagged` resync, the `guard_ipc_response` panic guard, the biased
//! drain/shutdown/read/event `select!`, and the connection-drain helper. For
//! `UnixStream` it monomorphizes to exactly the previous code, so there is no
//! runtime cost and no client-visible change; adapters (phase 4) reuse it
//! unchanged over other carriers.
//!
//! The conformance corpus ([`ipc_conformance`]) exercises this core over four
//! harnesses — the UDS socketpair and in-memory `tokio::io::duplex` carriers,
//! plus the real `UdsServerTransport` and `MemoryTransport` through their
//! production `bind`/`accept`/`connect` path — proving the scenarios are both
//! carrier- and transport-independent.

use crate::handler::{handle_request_with_peer, request_lane, IpcLane};
use crate::state::AppState;
use futures::{FutureExt, SinkExt, StreamExt};
use mxr_protocol::{
    ClientKind, DaemonEvent, IpcCodec, IpcErrorKind, IpcMessage, IpcPayload, Request, Response,
    ResponseData,
};
use mxr_transport::{PeerAuth, PeerInfo};
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, watch, Semaphore};
use tokio::task::JoinSet;
use tokio_util::codec::{FramedRead, FramedWrite};

/// Connection-scoped authentication state for the serve core (phase 5, token
/// transports). Transports with implicit trust (UDS peer creds, in-process,
/// stdio) start authenticated and this is inert. A [`PeerAuth::TokenRequired`]
/// transport (the TCP-loopback adapter) starts UNauthenticated: every request
/// is gated behind a successful `Authenticate` handshake, and events are
/// withheld until then so a pre-auth peer never observes daemon state.
///
/// The gate lives here — in the serve core's per-connection state, not in the
/// transport (which stays protocol-free) and not in the stateless request
/// dispatcher (which has no notion of a connection).
pub(crate) struct ConnectionAuth {
    /// Whether this connection is currently trusted. For non-token transports
    /// this is `true` from the start and never changes.
    authenticated: bool,
    /// The daemon's expected bearer token, when a token transport is
    /// configured. `None` while unauthenticated means "fail closed" — no token
    /// can ever match, so every request is rejected.
    expected: Option<Arc<str>>,
}

impl ConnectionAuth {
    /// Build the auth state for one accepted connection from its peer evidence
    /// and the daemon's expected token (if any).
    pub(crate) fn new(peer: &PeerInfo, expected: Option<Arc<str>>) -> Self {
        let authenticated = !matches!(peer.auth, PeerAuth::TokenRequired);
        Self {
            authenticated,
            expected,
        }
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    /// Decide the fate of an incoming request. `None` means "dispatch normally"
    /// (the connection is trusted). `Some(response)` short-circuits dispatch
    /// with a canned frame — either the `Authenticated` ack for a valid
    /// handshake (which also flips this connection to trusted) or an `Auth`
    /// error for anything sent before a successful `Authenticate`.
    fn gate(&mut self, msg: &IpcMessage) -> Option<IpcMessage> {
        if self.authenticated {
            // Trusted already: even a redundant `Authenticate` flows to the
            // dispatcher, which answers it as a harmless no-op.
            return None;
        }
        match &msg.payload {
            IpcPayload::Request(Request::Authenticate { token }) => {
                if token_matches(self.expected.as_deref(), token) {
                    self.authenticated = true;
                    Some(response_frame(
                        msg.id,
                        Response::Ok {
                            data: ResponseData::Authenticated,
                        },
                    ))
                } else {
                    Some(response_frame(
                        msg.id,
                        Response::error_kinded("invalid daemon token", IpcErrorKind::Auth),
                    ))
                }
            }
            _ => Some(response_frame(
                msg.id,
                Response::error_kinded(
                    "authentication required: send Authenticate before any other request",
                    IpcErrorKind::Auth,
                ),
            )),
        }
    }
}

/// Compare the presented token against the expected one in constant time.
/// `expected == None` (no token configured) can never match, so the connection
/// stays closed. `constant_time_eq` compares the full byte range without an
/// early return, so a same-length wrong guess cannot be distinguished from a
/// right one by timing (it does reveal length, which is not secret here).
fn token_matches(expected: Option<&str>, presented: &str) -> bool {
    match expected {
        Some(expected) => {
            constant_time_eq::constant_time_eq(expected.as_bytes(), presented.as_bytes())
        }
        None => false,
    }
}

/// Point-to-point "you missed events, resync" signal for one client.
fn events_lagged_frame(skipped: u64) -> IpcMessage {
    IpcMessage {
        id: 0,
        source: ClientKind::default(),
        payload: IpcPayload::Event(DaemonEvent::EventsLagged { skipped }),
    }
}

/// What happened to one outbound frame.
enum FrameSend {
    Sent,
    /// The frame could not be encoded. The connection is untouched: the
    /// codec's length check and the JSON serialisation both fail before a byte
    /// reaches the write buffer, so only this frame is lost.
    Unencodable(std::io::Error),
    /// The write failed. The connection is over.
    Disconnected(std::io::Error),
}

/// Send one frame, telling an encode failure apart from a transport failure by
/// construction rather than by sniffing `io::ErrorKind`.
///
/// `SinkExt::send` is ready + start_send + flush, so a transport error raised
/// during its flush is indistinguishable from an encode error by kind alone —
/// a TLS stack reporting `InvalidData` on a broken write would be mistaken for
/// an oversized frame, and the recovery frame would be appended behind a
/// half-flushed one. `feed` performs only the encode step, so every error it
/// returns provably came from `IpcCodec::encode`; the flush that follows is
/// the only place transport errors can appear.
async fn send_frame<Si>(sink: &mut Si, message: IpcMessage) -> FrameSend
where
    Si: futures::Sink<IpcMessage, Error = std::io::Error> + Unpin,
{
    if let Err(error) = sink.feed(message).await {
        return FrameSend::Unencodable(error);
    }
    match sink.flush().await {
        Ok(()) => FrameSend::Sent,
        Err(error) => FrameSend::Disconnected(error),
    }
}

/// Explain an unencodable frame to the client that asked for it.
///
/// `InvalidInput` is the codec refusing a frame past its 16 MiB cap — the
/// caller can act on that by asking for less. `InvalidData` is a serialisation
/// failure, which is ours, not theirs.
fn describe_unencodable(error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::InvalidInput {
        "response payload exceeded the IPC frame limit; narrow the request (for example a smaller --limit)".to_string()
    } else {
        format!("response could not be encoded: {error}")
    }
}

fn response_frame(id: u64, response: Response) -> IpcMessage {
    IpcMessage {
        id,
        source: ClientKind::Daemon,
        payload: IpcPayload::Response(response),
    }
}

/// Hot-lane concurrency: fast user-initiated commands (lists, gets,
/// mutations, sync status). Sized large enough that realistic burst
/// traffic never queues. See `crate::handler::request_lane`.
pub(crate) const REQUEST_CONCURRENCY_LIMIT: usize = 64;
/// Bulk-lane concurrency: long-running operations (LLM inference,
/// network attachments, full-store rebuilds). Bounded so a burst of
/// slow ops can't starve hot commands of CPU/permits or spawn
/// unbounded parallel LLM / network work.
pub(crate) const BULK_CONCURRENCY_LIMIT: usize = 8;
pub(crate) const CONNECTION_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Serve a single client connection to completion over any byte stream.
///
/// Generic over `S`: the accept loop passes a `UnixStream` today (the fn
/// monomorphizes to the previous concrete code), the conformance corpus also
/// drives it over `tokio::io::duplex`, and phase-4 adapters feed it other
/// carriers. Everything below the framing layer is transport-neutral.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is a distinct per-connection input the accept loop already holds (stream, shared state, the two lane semaphores, peer evidence, the optional auth token, and the event + shutdown receivers); bundling them would only move the wiring, not remove it"
)]
pub(crate) async fn serve_client_connection<S>(
    stream: S,
    state: Arc<AppState>,
    request_semaphore: Arc<Semaphore>,
    bulk_semaphore: Arc<Semaphore>,
    peer: PeerInfo,
    auth_token: Option<Arc<str>>,
    mut event_rx: broadcast::Receiver<IpcMessage>,
    mut shutdown_rx: watch::Receiver<bool>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    // Framed read/write halves rather than `Framed::split()`: a futures
    // `SplitSink` buffers one item and defers the inner `start_send` — and so
    // the codec's `encode` — until the next flush, which would put encode
    // failures back on the flush path that `send_frame` exists to keep them
    // off. `FramedWrite::feed` encodes inline.
    let (reader, writer) = tokio::io::split(stream);
    let mut sink = FramedWrite::new(writer, IpcCodec::new());
    let mut stream = FramedRead::new(reader, IpcCodec::new());
    let mut request_tasks: JoinSet<IpcMessage> = JoinSet::new();
    let mut accept_requests = true;
    let mut can_send = true;
    let mut shutdown_requested = false;
    // Connection-scoped auth state. Token transports start unauthenticated and
    // gate every request; every other transport starts trusted.
    let mut auth = ConnectionAuth::new(&peer, auth_token);

    loop {
        tokio::select! {
            biased;

            joined = request_tasks.join_next(), if !request_tasks.is_empty() => {
                match joined {
                    Some(Ok(response)) if can_send => {
                        let id = response.id;
                        match send_frame(&mut sink, response).await {
                            FrameSend::Sent => {}
                            // The response could not be framed — almost always
                            // a result set past the codec's cap. Nothing was
                            // written, so the caller is still there waiting:
                            // answer it instead of hanging up on it.
                            FrameSend::Unencodable(error) => {
                                tracing::warn!(%error, id, "response could not be framed; replying with an error");
                                let replacement = response_frame(
                                    id,
                                    Response::error_kinded(
                                        describe_unencodable(&error),
                                        IpcErrorKind::Internal,
                                    ),
                                );
                                if !matches!(
                                    send_frame(&mut sink, replacement).await,
                                    FrameSend::Sent
                                ) {
                                    can_send = false;
                                    accept_requests = false;
                                }
                            }
                            FrameSend::Disconnected(error) => {
                                tracing::warn!(%error, "dropping client connection: response send failed");
                                can_send = false;
                                accept_requests = false;
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        tracing::warn!("ipc request task failed: {error}");
                    }
                    None => {}
                }
            }
            changed = shutdown_rx.changed(), if !shutdown_requested => {
                match changed {
                    Ok(()) if *shutdown_rx.borrow_and_update() => {
                        shutdown_requested = true;
                        accept_requests = false;
                    }
                    Ok(()) => {}
                    Err(_) => {
                        shutdown_requested = true;
                        accept_requests = false;
                    }
                }
            }
            msg = stream.next(), if accept_requests => {
                match msg {
                    // Token gate: on a `TokenRequired` transport before a
                    // successful handshake, `gate` returns a canned frame (an
                    // `Authenticated` ack that flips this connection to trusted,
                    // or an `Auth` error). Sent INLINE on the sink — not via a
                    // spawned task — so it is guaranteed to precede any event the
                    // now-enabled event branch would deliver (a pre-auth
                    // `EventsLagged` must never race ahead of `Authenticated`).
                    // No lane permit or dispatch is involved. This arm only runs
                    // while unauthenticated, so `gate` always returns `Some`;
                    // UDS/memory/stdio start trusted and dispatch unchanged below.
                    Some(Ok(ipc_msg)) if !auth.is_authenticated() => {
                        if let Some(canned) = auth.gate(&ipc_msg) {
                            if can_send && sink.send(canned).await.is_err() {
                                can_send = false;
                                accept_requests = false;
                            }
                        }
                    }
                    Some(Ok(ipc_msg)) => {
                        let permit_wait_started = std::time::Instant::now();
                        // Route the request to its lane semaphore before
                        // spawning. Slow operations (LLM inference,
                        // network downloads, full-store rebuilds) drain a
                        // bounded bulk pool; everything else uses the hot
                        // pool. Net effect: a burst of LLM calls can't
                        // starve fast list/get/mutation commands of
                        // permits.
                        let lane = match &ipc_msg.payload {
                            mxr_protocol::IpcPayload::Request(req) => request_lane(req),
                            _ => IpcLane::Hot,
                        };
                        let semaphore = match lane {
                            IpcLane::Hot => request_semaphore.clone(),
                            IpcLane::Bulk => bulk_semaphore.clone(),
                        };
                        let permit = match semaphore.acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => {
                                accept_requests = false;
                                continue;
                            }
                        };
                        let state = state.clone();
                        // The connection's peer identity rides into every
                        // request's dispatch context. No policy reads it this
                        // phase (UDS keeps implicit trust); the plumbing point
                        // exists for phase 5's token gate.
                        let peer = peer.clone();
                        request_tasks.spawn(async move {
                            let _permit = permit;
                            tracing::trace!(
                                wait_ms = permit_wait_started.elapsed().as_secs_f64() * 1000.0,
                                lane = ?lane,
                                "ipc request permit acquired"
                            );
                            guard_ipc_response(ipc_msg.id, async {
                                // Test-only hook: lets a conformance scenario
                                // hold a Bulk-lane request in flight
                                // deterministically. Compiled out of every
                                // non-test build; only intercepts the gated
                                // sentinel request when a test installs a gate.
                                #[cfg(test)]
                                if let Some(response) =
                                    ipc_conformance::gate::maybe_intercept(&ipc_msg).await
                                {
                                    return response;
                                }
                                handle_request_with_peer(&state, &ipc_msg, peer).await
                            })
                            .await
                        });
                    }
                    Some(Err(error)) => {
                        tracing::error!("IPC decode error: {}", error);
                        accept_requests = false;
                    }
                    None => {
                        accept_requests = false;
                    }
                }
            }
            event = event_rx.recv(), if accept_requests && can_send && !shutdown_requested && auth.is_authenticated() => {
                match event {
                    Ok(event_msg) => {
                        match send_frame(&mut sink, event_msg).await {
                            FrameSend::Sent => {}
                            // An event whose payload will not fit a frame is
                            // the event's problem, not the connection's — and
                            // dropping the connection over it also drops the
                            // in-flight request's response, which is how a
                            // 27k-envelope `NewMessages` used to kill
                            // `mxr demo` (#179). Skip the event and tell the
                            // client to resync: same contract as a lagged
                            // stream, which is what this effectively is.
                            FrameSend::Unencodable(error) => {
                                tracing::warn!(%error, "daemon event could not be framed; dropped it and signalled resync");
                                if !matches!(
                                    send_frame(&mut sink, events_lagged_frame(1)).await,
                                    FrameSend::Sent
                                ) {
                                    can_send = false;
                                    accept_requests = false;
                                }
                            }
                            FrameSend::Disconnected(error) => {
                                tracing::warn!(%error, "dropping client connection: event send failed");
                                can_send = false;
                                accept_requests = false;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // The per-client channel filled and the broadcast
                        // dropped `skipped` events for this client. It can't
                        // know what it missed, so tell it to resync rather
                        // than silently leaving its views stale. Sent only to
                        // this client — it is not a broadcast event.
                        tracing::debug!(skipped, "client event stream lagged; signalling resync");
                        if !matches!(
                            send_frame(&mut sink, events_lagged_frame(skipped)).await,
                            FrameSend::Sent
                        ) {
                            can_send = false;
                            accept_requests = false;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        accept_requests = false;
                    }
                }
            }
        }

        if !accept_requests && request_tasks.is_empty() {
            break;
        }
    }

    tracing::debug!("Client disconnected");
}

pub(crate) async fn drain_connection_tasks(connections: &mut JoinSet<()>, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while !connections.is_empty() {
        let Some(remaining) = deadline.checked_duration_since(tokio::time::Instant::now()) else {
            tracing::warn!("client connection drain timed out");
            connections.abort_all();
            while let Some(joined) = connections.join_next().await {
                if let Err(error) = joined {
                    tracing::trace!("aborted client connection task: {error}");
                }
            }
            return;
        };

        match tokio::time::timeout(remaining, connections.join_next()).await {
            Ok(Some(Ok(()))) => {}
            Ok(Some(Err(error))) => tracing::warn!("client connection task failed: {error}"),
            Ok(None) => break,
            Err(_) => {
                tracing::warn!("client connection drain timed out");
                connections.abort_all();
                while let Some(joined) = connections.join_next().await {
                    if let Err(error) = joined {
                        tracing::trace!("aborted client connection task: {error}");
                    }
                }
                return;
            }
        }
    }
}

async fn guard_ipc_response<F>(msg_id: u64, future: F) -> IpcMessage
where
    F: std::future::Future<Output = IpcMessage>,
{
    match AssertUnwindSafe(future).catch_unwind().await {
        Ok(response) => response,
        Err(panic_payload) => {
            let panic_message = panic_payload_message(&*panic_payload);
            tracing::error!(
                request_id = msg_id,
                "Daemon handler panicked: {panic_message}"
            );
            IpcMessage {
                id: msg_id,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Response(Response::error(format!(
                    "Daemon handler panicked while processing the request: {panic_message}"
                ))),
            }
        }
    }
}

fn panic_payload_message(panic_payload: &(dyn Any + Send)) -> String {
    if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = panic_payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// IPC conformance corpus (transport-adapter initiative): an executable
/// characterization of the serve loop's connection-level behavior, run over
/// both the UDS and in-memory-duplex carriers. In-crate `#[cfg(test)]` module
/// (not `tests/`) because it drives the private, generic `serve_client_connection`
/// directly. See the file's module docs.
#[cfg(test)]
mod ipc_conformance;

#[cfg(test)]
mod tests {
    use super::{
        guard_ipc_response, serve_client_connection, BULK_CONCURRENCY_LIMIT,
        REQUEST_CONCURRENCY_LIMIT,
    };
    use crate::state::AppState;
    use futures::{SinkExt, StreamExt};
    use mxr_core::id::AccountId;
    use mxr_protocol::{
        DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
    };
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::UnixStream;
    use tokio::sync::Semaphore;
    use tokio_util::codec::Framed;

    #[tokio::test]
    async fn handler_panic_returns_error_response() {
        let response = guard_ipc_response(7, async {
            panic!("boom");
            #[allow(unreachable_code)]
            IpcMessage {
                id: 7,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Response(Response::Ok {
                    data: ResponseData::Pong,
                }),
            }
        })
        .await;

        match response.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("boom"));
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn client_connection_acknowledges_shutdown_before_exiting() {
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let state_for_cleanup = state.clone();
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();

        let server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                state,
                request_semaphore,
                bulk_semaphore,
                mxr_transport::PeerInfo::local(),
                None,
                event_rx,
                shutdown_rx,
            )
            .await;
        });

        let mut client = Framed::new(client_stream, IpcCodec::new());
        client
            .send(IpcMessage {
                id: 44,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::Shutdown),
            })
            .await
            .expect("send shutdown request");

        let response = tokio::time::timeout(Duration::from_secs(1), client.next())
            .await
            .expect("response should arrive")
            .expect("response frame")
            .expect("response should decode");

        match response.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("expected shutdown ack, got {other:?}"),
        }

        drop(client);

        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .expect("connection task should exit")
            .expect("connection task join");

        state_for_cleanup
            .shutdown_runtime_tasks(Duration::from_secs(1))
            .await;
    }

    #[tokio::test]
    async fn lagged_event_stream_signals_resync_to_client() {
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let state_for_cleanup = state.clone();
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();

        // Overflow the 256-slot broadcast channel BEFORE the connection
        // task starts draining it, so the first `recv()` returns
        // `Lagged`. The account id is irrelevant — these events only exist
        // to fill the channel.
        let account_id = AccountId::new();
        for _ in 0..400u32 {
            let _ = state.event_tx.send(IpcMessage {
                id: 0,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Event(DaemonEvent::SyncCompleted {
                    account_id: account_id.clone(),
                    messages_synced: 0,
                }),
            });
        }

        let server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                state,
                request_semaphore,
                bulk_semaphore,
                mxr_transport::PeerInfo::local(),
                None,
                event_rx,
                shutdown_rx,
            )
            .await;
        });

        let mut client = Framed::new(client_stream, IpcCodec::new());
        // The first frame the client sees must be the resync signal, not a
        // silently-truncated event stream.
        let frame = tokio::time::timeout(Duration::from_secs(1), client.next())
            .await
            .expect("a frame should arrive")
            .expect("frame present")
            .expect("frame decodes");
        match frame.payload {
            IpcPayload::Event(DaemonEvent::EventsLagged { skipped }) => {
                assert!(skipped > 0, "skipped count should be positive");
            }
            other => panic!("expected EventsLagged resync signal, got {other:?}"),
        }

        drop(client);
        state_for_cleanup.request_shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;
        state_for_cleanup
            .shutdown_runtime_tasks(Duration::from_secs(1))
            .await;
    }

    /// A request already running when the client walks away must run to
    /// completion.
    ///
    /// `mxr demo` tells the user they can Ctrl-C during the prewarm phase and
    /// "the daemon finishes the rest in the background". That promise is only
    /// true if a disconnect does not abort the in-flight `RebuildAnalytics` /
    /// `Wrapped` / voice + decision rebuilds, so pin it: with the request
    /// parked inside the conformance gate, closing the connection must not end
    /// the connection task — only releasing the request does.
    #[tokio::test]
    async fn disconnect_does_not_abort_an_in_flight_request() {
        let gate = super::ipc_conformance::gate::install().await;
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let state_for_cleanup = state.clone();
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();

        let mut server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                state,
                request_semaphore,
                bulk_semaphore,
                mxr_transport::PeerInfo::local(),
                None,
                event_rx,
                shutdown_rx,
            )
            .await;
        });

        let mut client = Framed::new(client_stream, IpcCodec::new());
        client
            .send(IpcMessage {
                id: 12,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::RebuildAnalytics),
            })
            .await
            .expect("send rebuild-analytics");
        gate.wait_until_entered(1).await;

        // The user's Ctrl-C.
        drop(client);

        // The connection task must still be alive: its request is running.
        let still_running = tokio::time::timeout(Duration::from_millis(300), &mut server).await;
        assert!(
            still_running.is_err(),
            "the connection task must not exit while its request is in flight"
        );

        gate.open();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("connection task exits once the request finishes")
            .expect("connection task join");

        state_for_cleanup
            .shutdown_runtime_tasks(Duration::from_secs(1))
            .await;
    }

    /// Read frames until the connection goes quiet, reporting whether a
    /// resync signal and a `Pong` were among them.
    async fn drain_frames(client: &mut Framed<UnixStream, IpcCodec>) -> (bool, bool) {
        let mut saw_resync = false;
        let mut saw_pong = false;
        while let Ok(Some(Ok(frame))) =
            tokio::time::timeout(Duration::from_millis(500), client.next()).await
        {
            match frame.payload {
                IpcPayload::Event(DaemonEvent::EventsLagged { .. }) => saw_resync = true,
                IpcPayload::Response(Response::Ok {
                    data: ResponseData::Pong,
                }) => saw_pong = true,
                _ => {}
            }
        }
        (saw_resync, saw_pong)
    }

    /// A response the codec cannot frame must reach the caller as an error.
    ///
    /// Sibling of the event case: nothing was written, so the client is still
    /// there waiting for an answer to a request it sent. Silently closing the
    /// connection turns "your query was too big" into "the daemon died".
    #[tokio::test]
    async fn oversized_response_answers_with_an_error() {
        let _gate = super::ipc_conformance::gate::install_oversized_response().await;
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let state_for_cleanup = state.clone();
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();

        let server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                state,
                request_semaphore,
                bulk_semaphore,
                mxr_transport::PeerInfo::local(),
                None,
                event_rx,
                shutdown_rx,
            )
            .await;
        });

        let mut client = Framed::new(client_stream, IpcCodec::new());
        client
            .send(IpcMessage {
                id: 55,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::RebuildAnalytics),
            })
            .await
            .expect("send rebuild-analytics");

        let frame = tokio::time::timeout(Duration::from_secs(2), client.next())
            .await
            .expect("a frame should arrive")
            .expect("connection must stay open")
            .expect("frame decodes");
        assert_eq!(frame.id, 55, "the replacement must answer the same request");
        match frame.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.contains("frame limit"),
                    "the error should name the cause, got: {message}"
                );
            }
            other => panic!("expected an error response, got {other:?}"),
        }

        // Still usable afterwards.
        client
            .send(IpcMessage {
                id: 56,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::Ping),
            })
            .await
            .expect("send ping");
        let (_, saw_pong) = drain_frames(&mut client).await;
        assert!(
            saw_pong,
            "the connection must survive the oversized response"
        );

        drop(client);
        state_for_cleanup.request_shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;
        state_for_cleanup
            .shutdown_runtime_tasks(Duration::from_secs(1))
            .await;
    }

    /// An event the codec cannot frame must cost the client that event, not
    /// its connection.
    ///
    /// A `NewMessages` carrying every envelope of a 27k-message initial
    /// backfill serialises past the 16 MiB frame cap. The daemon used to treat
    /// the failed encode as a dead socket and tear the connection down, which
    /// also threw away the in-flight request's response — the second way
    /// `mxr demo --messages 50000` died in #179. The connection must survive
    /// and the client must still get its answer.
    #[tokio::test]
    async fn oversized_event_is_dropped_without_closing_the_connection() {
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let state_for_cleanup = state.clone();
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();

        let server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                state,
                request_semaphore,
                bulk_semaphore,
                mxr_transport::PeerInfo::local(),
                None,
                event_rx,
                shutdown_rx,
            )
            .await;
        });

        // Comfortably past the codec's 16 MiB cap, so `encode` refuses it.
        let _ = state_for_cleanup.event_tx.send(IpcMessage {
            id: 0,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Event(DaemonEvent::SyncError {
                account_id: AccountId::new(),
                error: "x".repeat(17 * 1024 * 1024),
            }),
        });

        let mut client = Framed::new(client_stream, IpcCodec::new());
        client
            .send(IpcMessage {
                id: 91,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(Request::Ping),
            })
            .await
            .expect("send ping");

        // Drain everything the daemon has to say rather than stopping at the
        // first frame of interest: the order of the resync signal and the
        // response is an implementation detail of the select loop.
        let (saw_resync, saw_pong) = drain_frames(&mut client).await;
        assert!(saw_pong, "the request must still be answered");
        assert!(
            saw_resync,
            "dropping an event must tell the client to resync"
        );

        drop(client);
        state_for_cleanup.request_shutdown();
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;
        state_for_cleanup
            .shutdown_runtime_tasks(Duration::from_secs(1))
            .await;
    }
}
