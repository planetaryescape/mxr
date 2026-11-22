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

use crate::handler::{handle_request_with_peer, request_is_abandonable, request_lane, IpcLane};
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
/// withheld until then so a pre-auth peer never observes daemon state. Client
/// declaration is still required after authentication before events can flow.
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

fn events_lagged_frame(skipped: u64) -> IpcMessage {
    IpcMessage {
        expected_workspace: None,
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
///
/// That rests on two things, which is why this takes a concrete `FramedWrite`
/// rather than any `Sink`. First, `FramedWrite::feed` encodes inline — a
/// futures `SplitSink` instead parks the item and encodes during the *next*
/// flush, which would put encode errors straight back on the flush path.
/// Second, `FramedWrite::poll_ready` flushes when the write buffer is over its
/// backpressure mark, and such a flush could report a transport error out of
/// `feed`; that cannot happen here because this is the connection's only
/// writer and it always flushes before returning, so the buffer is empty at
/// every `feed`. The `debug_assert` keeps that invariant honest.
async fn send_frame<W>(sink: &mut FramedWrite<W, IpcCodec>, message: IpcMessage) -> FrameSend
where
    W: tokio::io::AsyncWrite + Unpin,
{
    debug_assert!(
        sink.write_buffer().is_empty(),
        "send_frame must leave the write buffer empty, or feed's backpressure flush could report a transport error as an encode failure"
    );
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
        expected_workspace: None,
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
    let (reader, writer) = tokio::io::split(stream);
    let mut sink = FramedWrite::new(writer, IpcCodec::new());
    let mut stream = FramedRead::new(reader, IpcCodec::new());
    let mut request_tasks: JoinSet<IpcMessage> = JoinSet::new();
    let mut accept_requests = true;
    let mut can_send = true;
    let mut shutdown_requested = false;
    let mut client_closed = false;
    let mut abandonable: Vec<tokio::task::AbortHandle> = Vec::new();
    let mut auth = ConnectionAuth::new(&peer, auth_token);
    let mut client_kind: Option<ClientKind> = None;
    let mut event_profile: Option<&'static str> = None;
    let setup_deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    loop {
        tokio::select! {
            biased;

            _ = tokio::time::sleep_until(setup_deadline), if client_kind.is_none() => {
                tracing::debug!("client declaration timed out");
                break;
            }
            joined = request_tasks.join_next(), if !request_tasks.is_empty() => {
                match joined {
                    Some(Ok(response)) if can_send => {
                        let id = response.id;
                        if accept_requests
                            && !shutdown_requested
                            && auth.is_authenticated()
                            && client_kind.is_some()
                            && !flush_queued_events(&mut sink, &state, event_profile, &mut event_rx).await
                        {
                            can_send = false;
                            accept_requests = false;
                            if request_tasks.is_empty() {
                                break;
                            }
                            continue;
                        }
                        match send_frame(&mut sink, response).await {
                            FrameSend::Sent => {}
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
                    Some(Err(error)) if error.is_cancelled() => {}
                    Some(Err(error)) => {
                        tracing::warn!("ipc request task failed: {error}");
                    }
                    None => {}
                }
            }
            changed = shutdown_rx.changed(), if !shutdown_requested => {
                match changed {
                    Ok(()) if *shutdown_rx.borrow_and_update() => {
                        if can_send && auth.is_authenticated() && !flush_queued_events(&mut sink, &state, event_profile, &mut event_rx).await {can_send=false;}
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
                    Some(Ok(ipc_msg)) if !auth.is_authenticated() => {
                        if let Some(canned) = auth.gate(&ipc_msg) {
                            if can_send && sink.send(canned).await.is_err() {
                                can_send = false;
                                accept_requests = false;
                            }
                        }
                    }
                    Some(Ok(ipc_msg)) => {
                        let setup_response = match (&ipc_msg.payload, client_kind) {
                            (IpcPayload::Request(Request::DeclareClient { kind, protocol_version }), None)
                                if *kind == ipc_msg.source
                                    && *protocol_version == mxr_protocol::IPC_PROTOCOL_VERSION => {
                                let name = crate::handler::account_scope::profile_name(*kind);
                                if name.is_some_and(|name| !state.config_snapshot().agent_surfaces.profiles.contains_key(name)) {
                                    Some(Response::error_kinded(format!("{} clients require [agents.profiles.{}] on the daemon host; configure that profile and reconnect", kind.as_str(), kind.as_str()), IpcErrorKind::Policy))
                                } else {
                                    client_kind = Some(*kind);
                                    event_profile = name;
                                    Some(Response::Ok { data: ResponseData::Ack })
                                }
                            }
                            (_, None) => Some(Response::error_kinded(
                                format!("send DeclareClient with matching source and IPC protocol {} before ordinary requests", mxr_protocol::IPC_PROTOCOL_VERSION),
                                IpcErrorKind::Unsupported,
                            )),
                            (IpcPayload::Request(Request::DeclareClient { .. }), Some(_)) => Some(Response::error_kinded(
                                "client identity is already declared for this connection", IpcErrorKind::InvalidRequest,
                            )),
                            (_, Some(kind)) if kind != ipc_msg.source => Some(Response::error_kinded(
                                "request source conflicts with the declared client identity", IpcErrorKind::Policy,
                            )),
                            _ => None,
                        };
                        if let Some(response) = setup_response {
                            if sink.send(response_frame(ipc_msg.id, response)).await.is_err() || client_kind.is_none() {
                                break;
                            }
                            continue;
                        }
                        let permit_wait_started = std::time::Instant::now();
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
                        let peer = peer.clone();
                        let abandonable_request = matches!(
                            &ipc_msg.payload,
                            mxr_protocol::IpcPayload::Request(req) if request_is_abandonable(req)
                        );
                        let task = request_tasks.spawn(async move {
                            let _permit = permit;
                            tracing::trace!(
                                wait_ms = permit_wait_started.elapsed().as_secs_f64() * 1000.0,
                                lane = ?lane,
                                "ipc request permit acquired"
                            );
                            guard_ipc_response(ipc_msg.id, async {
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
                        abandonable.retain(|handle| !handle.is_finished());
                        if abandonable_request {
                            abandonable.push(task);
                        }
                    }
                    Some(Err(error)) => {
                        tracing::error!("IPC decode error: {}", error);
                        accept_requests = false;
                    }
                    None => {
                        client_closed = true;
                        accept_requests = false;
                    }
                }
            }
            event = event_rx.recv(), if accept_requests && can_send && !shutdown_requested && auth.is_authenticated() && client_kind.is_some() => {
                let delivered = match event {
                    Ok(event_msg) => forward_event(&mut sink, &state, event_profile, event_msg).await,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        signal_lagged(&mut sink, event_profile, skipped).await
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        accept_requests = false;
                        true
                    }
                };
                if !delivered {
                    can_send = false;
                    accept_requests = false;
                }
            }
        }

        if client_closed || !can_send {
            for handle in abandonable.drain(..) {
                handle.abort();
            }
        }

        if !accept_requests && request_tasks.is_empty() {
            break;
        }
    }

    tracing::debug!("Client disconnected");
}

/// Send one daemon event to this client, cut to the client's profile. Returns
/// `false` when the connection is gone.
async fn forward_event<W>(
    sink: &mut FramedWrite<W, IpcCodec>,
    state: &AppState,
    event_profile: Option<&'static str>,
    event_msg: IpcMessage,
) -> bool
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let event_msg = match event_profile {
        Some(name) => match scope_event_frame(state, name, event_msg).await {
            Some(event_msg) => event_msg,
            None => return true,
        },
        None => event_msg,
    };
    match send_frame(sink, event_msg).await {
        FrameSend::Sent => true,
        FrameSend::Unencodable(error) => {
            tracing::warn!(%error, "daemon event could not be framed; dropped it and signalled resync");
            matches!(
                send_frame(
                    sink,
                    events_lagged_frame(scoped_lag_count(event_profile, 1))
                )
                .await,
                FrameSend::Sent
            )
        }
        FrameSend::Disconnected(error) => {
            tracing::warn!(%error, "dropping client connection: event send failed");
            false
        }
    }
}

/// The per-client channel filled and the broadcast dropped `skipped` events
/// for this client. It can't know what it missed, so tell it to resync rather
/// than silently leaving its views stale. Sent only to this client — it is not
/// a broadcast event. Returns `false` when the connection is gone.
async fn signal_lagged<W>(
    sink: &mut FramedWrite<W, IpcCodec>,
    event_profile: Option<&'static str>,
    skipped: u64,
) -> bool
where
    W: tokio::io::AsyncWrite + Unpin,
{
    tracing::debug!(skipped, "client event stream lagged; signalling resync");
    matches!(
        send_frame(
            sink,
            events_lagged_frame(scoped_lag_count(event_profile, skipped))
        )
        .await,
        FrameSend::Sent
    )
}

/// Send the events already waiting for this client, then return. Returns
/// `false` when the connection is gone.
async fn flush_queued_events<W>(
    sink: &mut FramedWrite<W, IpcCodec>,
    state: &AppState,
    event_profile: Option<&'static str>,
    event_rx: &mut broadcast::Receiver<IpcMessage>,
) -> bool
where
    W: tokio::io::AsyncWrite + Unpin,
{
    flush_queued_events_up_to(
        sink,
        state,
        event_profile,
        event_rx,
        crate::state::EVENT_CHANNEL_CAPACITY,
    )
    .await
}

/// [`flush_queued_events`] with its bound spelled out. The bound is what was
/// queued when the response was ready: a busy sync keeps emitting, and draining
/// until empty could hold the response back. `len()` counts the positions a
/// lagging receiver has missed as well as the ones it can still read, so it is
/// capped at the channel's capacity, the most a receiver can hold. A lagged
/// receiver gets the resync signal once and the flush stops there.
async fn flush_queued_events_up_to<W>(
    sink: &mut FramedWrite<W, IpcCodec>,
    state: &AppState,
    event_profile: Option<&'static str>,
    event_rx: &mut broadcast::Receiver<IpcMessage>,
    capacity: usize,
) -> bool
where
    W: tokio::io::AsyncWrite + Unpin,
{
    for _ in 0..event_rx.len().min(capacity) {
        match event_rx.try_recv() {
            Ok(event_msg) => {
                if !forward_event(sink, state, event_profile, event_msg).await {
                    return false;
                }
            }
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                return signal_lagged(sink, event_profile, skipped).await;
            }
            Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                return true;
            }
        }
    }
    true
}

/// A lag count covers every account's events, so a profiled connection gets
/// the resync signal without it.
fn scoped_lag_count(event_profile: Option<&str>, skipped: u64) -> u64 {
    if event_profile.is_some() {
        0
    } else {
        skipped
    }
}

/// An event frame cut to what the named profile may see, or `None` to drop
/// it. A missing profile allows no account, and a failed lookup drops the
/// event rather than send it unchecked.
async fn scope_event_frame(
    state: &AppState,
    profile_name: &str,
    event_msg: IpcMessage,
) -> Option<IpcMessage> {
    let IpcMessage {
        expected_workspace,
        id,
        source,
        payload,
    } = event_msg;
    let IpcPayload::Event(event) = payload else {
        return Some(IpcMessage {
            expected_workspace,
            id,
            source,
            payload,
        });
    };
    let profile = state
        .config_snapshot()
        .agent_surfaces
        .profiles
        .get(profile_name)
        .cloned()
        .unwrap_or_default();
    match crate::handler::account_scope::scope_event(state, &profile, event).await {
        Ok(Some(event)) => Some(IpcMessage {
            expected_workspace: None,
            id,
            source,
            payload: IpcPayload::Event(event),
        }),
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, profile = profile_name, "dropped a daemon event the profile scope could not resolve");
            None
        }
    }
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
                expected_workspace: None,
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
    use super::{serve_client_connection, BULK_CONCURRENCY_LIMIT, REQUEST_CONCURRENCY_LIMIT};
    use crate::state::AppState;
    use futures::{SinkExt, StreamExt};
    use mxr_protocol::{IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::UnixStream;
    use tokio::sync::Semaphore;
    use tokio_util::codec::Framed;

    async fn declare(client: &mut Framed<UnixStream, IpcCodec>) {
        client
            .send(IpcMessage {
                expected_workspace: None,
                id: u64::MAX,
                source: mxr_protocol::ClientKind::Cli,
                payload: IpcPayload::Request(Request::DeclareClient {
                    kind: mxr_protocol::ClientKind::Cli,
                    protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                }),
            })
            .await
            .expect("declare");
        assert!(matches!(
            client
                .next()
                .await
                .expect("declaration ack")
                .expect("decode")
                .payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));
    }

    #[test]
    fn registration_proof_cannot_unlock_legacy_daemon_auth() {
        let peer = mxr_transport::PeerInfo {
            auth: mxr_transport::PeerAuth::TokenRequired,
        };
        let mut auth = super::ConnectionAuth::new(&peer, Some(Arc::from("local-daemon-token")));
        let request = IpcMessage {
            id: 1,
            expected_workspace: None,
            source: mxr_protocol::ClientKind::Cli,
            payload: IpcPayload::Request(Request::RemoteAuthority {
                request: mxr_protocol::RemoteRequest::Authenticate {
                    client_id: "client".into(),
                    mail_secret: mxr_core::RemoteSecret::new("a".repeat(64)),
                },
            }),
        };
        assert!(auth.gate(&request).is_some());
        assert!(!auth.is_authenticated());
        let ping = IpcMessage {
            payload: IpcPayload::Request(Request::Ping),
            ..request
        };
        assert!(auth.gate(&ping).is_some());
    }

    /// A mutation already running when the client walks away must run to
    /// completion.
    ///
    /// `mxr demo` tells the user they can Ctrl-C during the prewarm phase and
    /// "the daemon finishes the rest in the background". That promise is only
    /// true if a disconnect does not abort the in-flight `RebuildAnalytics` /
    /// `Wrapped` / voice + decision rebuilds, so pin it: with the request
    /// parked inside the conformance gate, closing the connection must not end
    /// the connection task or cancel the request; only releasing the request
    /// does, and it then completes.
    #[tokio::test]
    async fn disconnect_does_not_abort_an_in_flight_mutation() {
        let gate = super::ipc_conformance::gate::install().await;
        let (server, client, state) = start_gated_request(Request::RebuildAnalytics).await;
        let mut server = server;
        gate.wait_until_entered(1).await;

        drop(client);

        let still_running = tokio::time::timeout(Duration::from_millis(300), &mut server).await;
        assert!(
            still_running.is_err(),
            "the connection task must not exit while its request is in flight"
        );
        assert_eq!(gate.cancelled(), 0, "a mutation must not be cancelled");

        gate.open();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("connection task exits once the request finishes")
            .expect("connection task join");
        assert_eq!(gate.completed(), 1, "the mutation must run to completion");
        assert_eq!(gate.cancelled(), 0);

        state.shutdown_runtime_tasks(Duration::from_secs(1)).await;
    }

    /// Serve one connection, send `request` on it and hand back the server
    /// task, the client end and the state.
    async fn start_gated_request(
        request: Request,
    ) -> (
        tokio::task::JoinHandle<()>,
        Framed<UnixStream, IpcCodec>,
        Arc<AppState>,
    ) {
        let state = Arc::new(AppState::in_memory().await.expect("in-memory state"));
        let (server_stream, client_stream) = UnixStream::pair().expect("unix stream pair");
        let request_semaphore = Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT));
        let bulk_semaphore = Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT));
        let event_rx = state.event_tx.subscribe();
        let shutdown_rx = state.shutdown_receiver();
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            serve_client_connection(
                server_stream,
                server_state,
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
        declare(&mut client).await;
        client
            .send(IpcMessage {
                expected_workspace: None,
                id: 12,
                source: ::mxr_protocol::ClientKind::default(),
                payload: IpcPayload::Request(request),
            })
            .await
            .expect("send request");
        (server, client, state)
    }
}
