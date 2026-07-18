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
//! The conformance corpus ([`ipc_conformance`]) exercises this core over two
//! carriers — the UDS socketpair and an in-memory `tokio::io::duplex` — proving
//! the scenarios are carrier-independent, which is the phase-3 premise.

use crate::handler::{handle_request, request_lane, IpcLane};
use crate::state::AppState;
use futures::{FutureExt, SinkExt, StreamExt};
use mxr_protocol::{DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Response};
use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, watch, Semaphore};
use tokio::task::JoinSet;
use tokio_util::codec::Framed;

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
pub(crate) async fn serve_client_connection<S>(
    stream: S,
    state: Arc<AppState>,
    request_semaphore: Arc<Semaphore>,
    bulk_semaphore: Arc<Semaphore>,
    mut event_rx: broadcast::Receiver<IpcMessage>,
    mut shutdown_rx: watch::Receiver<bool>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut sink, mut stream) = Framed::new(stream, IpcCodec::new()).split();
    let mut request_tasks = JoinSet::new();
    let mut accept_requests = true;
    let mut can_send = true;
    let mut shutdown_requested = false;

    loop {
        tokio::select! {
            biased;

            joined = request_tasks.join_next(), if !request_tasks.is_empty() => {
                match joined {
                    Some(Ok(response)) if can_send => {
                        match sink.send(response).await {
                            Ok(()) => {}
                            Err(_) => {
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
                                handle_request(&state, &ipc_msg).await
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
            event = event_rx.recv(), if accept_requests && can_send && !shutdown_requested => {
                match event {
                    Ok(event_msg) => {
                        if sink.send(event_msg).await.is_err() {
                            can_send = false;
                            accept_requests = false;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // The per-client channel filled and the broadcast
                        // dropped `skipped` events for this client. It can't
                        // know what it missed, so tell it to resync rather
                        // than silently leaving its views stale. Sent only to
                        // this client — it is not a broadcast event.
                        tracing::debug!(skipped, "client event stream lagged; signalling resync");
                        let lagged = IpcMessage {
                            id: 0,
                            source: mxr_protocol::ClientKind::default(),
                            payload: IpcPayload::Event(DaemonEvent::EventsLagged { skipped }),
                        };
                        if sink.send(lagged).await.is_err() {
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
}
