//! IPC conformance corpus (Phase 2, transport-adapter initiative).
//!
//! An executable specification of the daemon's *connection-level* behavior on
//! protocol v4: id correlation, out-of-order completion, lane back-pressure,
//! event fan-out, framing edges, disconnect handling, panic recovery, and the
//! current (UDS) auth posture. It is characterization — every test PINS what
//! the serve loop does today so the phase-3 "serve core generic over
//! AsyncRead+AsyncWrite" refactor lands guarded, and (phases 3–6) the same
//! scenarios run unchanged against every transport carrier.
//!
//! Structural note: this lives in-crate as a `#[cfg(test)]` child module of
//! `serve` rather than in `crates/daemon/tests/` because the behavior under
//! test is only reachable in-process — `serve_client_connection` is private
//! and generic over the byte stream, the lane-limit constants are private, and the
//! state constructors (`AppState::in_memory`, `add_sync_provider_for_test`)
//! are `#[cfg(test)]` and so are never compiled for a black-box integration
//! test. The existing temp-socket tests in `server.rs` establish this pattern.
//!
//! Carrier seam: every scenario obtains its connection through [`spawn_server`]
//! (framed) or its raw sibling, both generic over a [`Carrier`]. Each scenario
//! is written once and run over BOTH carriers — the UDS socketpair
//! ([`UdsCarrier`]) and an in-memory `tokio::io::duplex` ([`DuplexCarrier`]) —
//! via the `run_on_both_carriers!` invocation at the end of the file.
//!
//! Determinism: scenarios are driven by explicit synchronization — oneshots,
//! `watch` channels, JoinSet completion — never wall-clock sleeps. The one
//! test-only production hook is a `#[cfg(test)]` request gate (see [`gate`]),
//! used where a scenario needs a Bulk-lane request held in flight
//! deterministically (out-of-order, saturation, event-interleave, in-flight
//! disconnect). It gates only `Request::RebuildAnalytics`, which no other
//! scenario issues.

use super::{serve_client_connection, BULK_CONCURRENCY_LIMIT, REQUEST_CONCURRENCY_LIMIT};
use crate::state::AppState;
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use mxr_core::id::AccountId;
use mxr_core::types::{Label, Mutation, SyncBatch, SyncCursor};
use mxr_core::{MailSyncProvider, MxrError, SyncCapabilities};
use mxr_protocol::{
    ClientKind, DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream};
use tokio::net::UnixStream;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;

/// Upper bound on any single "a frame should arrive" wait. Generous: the
/// scenarios are synchronization-driven, so this only trips on a genuine hang.
const RECV_TIMEOUT: Duration = Duration::from_secs(5);
/// Longer bound for the multi-MiB frame round-trip: transferring ~15.5 MiB over
/// the socket pair can be slow on a loaded CI box.
const BIG_FRAME_RECV_TIMEOUT: Duration = Duration::from_secs(30);
/// Upper bound on joining a serve task during teardown.
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
/// `IpcCodec`'s frame cap (`crates/protocol/src/codec.rs`): 16 MiB.
const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Test-only request gate (the single `#[cfg(test)]` production hook).
// ---------------------------------------------------------------------------

/// A `#[cfg(test)]`-only mechanism to hold a Bulk-lane request in flight under
/// the test's control. `serve_client_connection` consults
/// [`gate::maybe_intercept`] before dispatching each request; when a gate is
/// installed it short-circuits `Request::RebuildAnalytics` — pausing on a
/// `watch` "open" flag, then returning a canned `Ack`. This replaces
/// sleep-and-hope for the out-of-order, saturation, event-interleave, and
/// in-flight-disconnect scenarios with exact synchronization. Only one gate is
/// live at a time (a process-wide async mutex serializes gate-using tests), and
/// only `RebuildAnalytics` is affected, so no other scenario is perturbed.
pub(crate) mod gate {
    use super::{Arc, ClientKind, IpcMessage, IpcPayload, Request, Response, ResponseData};
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use tokio::sync::{watch, Mutex as AsyncMutex, OwnedMutexGuard};

    #[derive(Clone)]
    struct GateShared {
        open_rx: watch::Receiver<bool>,
        entered_tx: watch::Sender<usize>,
    }

    static GATE: Lazy<Mutex<Option<GateShared>>> = Lazy::new(|| Mutex::new(None));
    static SERIAL: Lazy<Arc<AsyncMutex<()>>> = Lazy::new(|| Arc::new(AsyncMutex::new(())));

    /// Called from the serve loop before dispatch. Returns `Some(response)` to
    /// short-circuit the gated sentinel request, or `None` to let normal
    /// dispatch proceed (the only value in production-shaped behavior).
    pub(crate) async fn maybe_intercept(msg: &IpcMessage) -> Option<IpcMessage> {
        let IpcPayload::Request(Request::RebuildAnalytics) = &msg.payload else {
            return None;
        };
        let shared = GATE.lock().clone()?;
        // Record entry so the test can wait for the lane to be saturated
        // before releasing the gate.
        shared.entered_tx.send_modify(|count| *count += 1);
        let mut open_rx = shared.open_rx;
        loop {
            if *open_rx.borrow_and_update() {
                break;
            }
            if open_rx.changed().await.is_err() {
                break;
            }
        }
        Some(IpcMessage {
            id: msg.id,
            source: ClientKind::default(),
            payload: IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }),
        })
    }

    /// A live gate. Dropping it uninstalls the gate and releases the
    /// serialization lock.
    pub(crate) struct GateHandle {
        open_tx: watch::Sender<bool>,
        entered_rx: watch::Receiver<usize>,
        _serial: OwnedMutexGuard<()>,
    }

    impl GateHandle {
        /// Release every gated request (current and future while installed).
        pub(crate) fn open(&self) {
            let _ = self.open_tx.send(true);
        }

        /// Resolve once at least `n` gated requests have entered the gate.
        pub(crate) async fn wait_until_entered(&self, n: usize) {
            let mut rx = self.entered_rx.clone();
            loop {
                if *rx.borrow_and_update() >= n {
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        }

        /// Current count of requests that have entered the gate.
        pub(crate) fn entered(&self) -> usize {
            *self.entered_rx.borrow()
        }
    }

    impl Drop for GateHandle {
        fn drop(&mut self) {
            *GATE.lock() = None;
        }
    }

    /// Install a fresh gate, serializing against any other gate-using test.
    pub(crate) async fn install() -> GateHandle {
        let serial = SERIAL.clone().lock_owned().await;
        let (open_tx, open_rx) = watch::channel(false);
        let (entered_tx, entered_rx) = watch::channel(0usize);
        *GATE.lock() = Some(GateShared {
            open_rx,
            entered_tx,
        });
        GateHandle {
            open_tx,
            entered_rx,
            _serial: serial,
        }
    }
}

// ---------------------------------------------------------------------------
// Test provider: a sync provider whose `create_label` panics, for the
// handler-panic-recovery scenario. Zero production change — registered via the
// existing `add_sync_provider_for_test` hook. Only `create_label` is exercised;
// the other required methods are never reached in that scenario.
// ---------------------------------------------------------------------------

struct PanicOnCreateLabel {
    account_id: AccountId,
}

#[async_trait]
impl MailSyncProvider for PanicOnCreateLabel {
    fn name(&self) -> &str {
        "panic"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities::default()
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(Vec::new())
    }

    async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        unreachable!("panic scenario never triggers a sync")
    }

    async fn fetch_attachment(&self, _msg: &str, _att: &str) -> Result<Vec<u8>, MxrError> {
        unreachable!("panic scenario never fetches an attachment")
    }

    async fn apply_mutation(&self, _id: &str, _mutation: &Mutation) -> Result<(), MxrError> {
        unreachable!("panic scenario never applies a mutation")
    }

    async fn create_label(&self, _name: &str, _color: Option<&str>) -> Result<Label, MxrError> {
        panic!("conformance: injected handler panic in create_label");
    }
}

// ---------------------------------------------------------------------------
// Carriers + client helpers.
// ---------------------------------------------------------------------------

/// A byte-stream carrier for the conformance corpus. Each carrier builds a
/// connected pair whose server end feeds `serve_client_connection` and whose
/// client end the scenario drives. The corpus runs every scenario over each
/// carrier (see the `run_on_both_carriers!` invocation at the bottom), proving
/// the serve core is carrier-independent — the whole phase-3 premise, checked
/// cheaply before any transport trait exists.
trait Carrier: 'static {
    /// Both ends share one stream type per carrier (a UDS socketpair, an
    /// in-memory duplex). Bounds mirror `serve_client_connection`'s.
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Build a fresh, connected `(server, client)` pair.
    fn pair() -> (Self::Stream, Self::Stream);
}

/// The production transport: a Unix `socketpair(2)` — the real fd / kernel path
/// the daemon serves in production.
struct UdsCarrier;

impl Carrier for UdsCarrier {
    type Stream = UnixStream;

    fn pair() -> (UnixStream, UnixStream) {
        UnixStream::pair().unwrap()
    }
}

/// An in-memory `tokio::io::duplex` pipe: no socket file, no fd, no kernel
/// round-trip — hermetic and fast. The buffer is sized past the 16 MiB frame
/// cap so the near-limit-frame scenario transfers without backpressure churn.
struct DuplexCarrier;

impl Carrier for DuplexCarrier {
    type Stream = DuplexStream;

    fn pair() -> (DuplexStream, DuplexStream) {
        tokio::io::duplex(MAX_FRAME_LEN + 1024)
    }
}

/// Hot/Bulk lane semaphores at the daemon's real production sizes.
fn lanes() -> (Arc<Semaphore>, Arc<Semaphore>) {
    (
        Arc::new(Semaphore::new(REQUEST_CONCURRENCY_LIMIT)),
        Arc::new(Semaphore::new(BULK_CONCURRENCY_LIMIT)),
    )
}

/// THE single carrier-construction point. Builds a connected stream pair, wires
/// the server end into `serve_client_connection` exactly as the accept loop
/// does (fresh event subscription, shared lane semaphores, shutdown receiver),
/// and hands back the raw client end plus the server task. `prep` runs after
/// the event receiver has subscribed but before the serve loop starts draining
/// it — the seam the lag scenario needs.
async fn spawn_server<C: Carrier>(
    state: Arc<AppState>,
    hot: Arc<Semaphore>,
    bulk: Arc<Semaphore>,
    prep: impl FnOnce(&Arc<AppState>),
) -> (C::Stream, JoinHandle<()>) {
    let (server_stream, client_stream) = C::pair();
    let event_rx = state.event_tx.subscribe();
    prep(&state);
    let shutdown_rx = state.shutdown_receiver();
    let server = tokio::spawn(async move {
        serve_client_connection(server_stream, state, hot, bulk, event_rx, shutdown_rx).await;
    });
    (client_stream, server)
}

/// A framed client over a served connection, generic over the carrier's stream.
struct Served<S> {
    client: Framed<S, IpcCodec>,
    server: JoinHandle<()>,
}

async fn serve<C: Carrier>(
    state: Arc<AppState>,
    hot: Arc<Semaphore>,
    bulk: Arc<Semaphore>,
) -> Served<C::Stream> {
    serve_with_prep::<C>(state, hot, bulk, |_| {}).await
}

async fn serve_with_prep<C: Carrier>(
    state: Arc<AppState>,
    hot: Arc<Semaphore>,
    bulk: Arc<Semaphore>,
    prep: impl FnOnce(&Arc<AppState>),
) -> Served<C::Stream> {
    let (client, server) = spawn_server::<C>(state, hot, bulk, prep).await;
    Served {
        client: Framed::new(client, IpcCodec::new()),
        server,
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Served<S> {
    async fn send(&mut self, id: u64, req: Request) {
        self.client.send(request(id, req)).await.unwrap();
    }

    /// Read the next frame within `timeout`, asserting one arrives and decodes.
    async fn recv_within(&mut self, timeout: Duration) -> IpcMessage {
        tokio::time::timeout(timeout, self.client.next())
            .await
            .expect("a frame should arrive before timeout")
            .expect("stream should not be closed")
            .expect("frame should decode")
    }

    /// Read the next frame within the default timeout.
    async fn recv(&mut self) -> IpcMessage {
        self.recv_within(RECV_TIMEOUT).await
    }

    /// Read frames, skipping unsolicited events, until the `Response` for `id`.
    async fn recv_response(&mut self, id: u64) -> IpcMessage {
        self.recv_response_within(id, RECV_TIMEOUT).await
    }

    /// Like [`Self::recv_response`] but with a caller-chosen per-frame timeout —
    /// for the multi-MiB frame scenario, where transfer can be slow on CI.
    async fn recv_response_within(&mut self, id: u64, timeout: Duration) -> IpcMessage {
        loop {
            let msg = self.recv_within(timeout).await;
            if msg.id == id && matches!(msg.payload, IpcPayload::Response(_)) {
                return msg;
            }
        }
    }

    /// Assert the connection closes with a strict, clean EOF (`None`) — a
    /// graceful shutdown must NOT surface as an I/O error (reset/truncation).
    async fn expect_eof(&mut self) {
        let next = tokio::time::timeout(RECV_TIMEOUT, self.client.next())
            .await
            .expect("close should be observed before timeout");
        assert!(
            next.is_none(),
            "graceful shutdown must close with a clean EOF (None), got {next:?}"
        );
    }
}

fn request(id: u64, req: Request) -> IpcMessage {
    IpcMessage {
        id,
        source: ClientKind::Cli,
        payload: IpcPayload::Request(req),
    }
}

/// Emit an event through the REAL production path — `chimes::emit_daemon_event`,
/// one of the daemon's actual emitters (`diagnostics::emit_operation_event`
/// constructs the frame identically). Scenarios that assert on the delivered
/// frame use this so they pin the emitter's actual output (notably `source`),
/// not a value the test wrote itself.
fn emit(state: &Arc<AppState>) {
    crate::chimes::emit_daemon_event(state, sample_event());
}

/// A synthetic event frame. Used ONLY as overflow filler for the `EventsLagged`
/// scenario, where the content is irrelevant — it just fills the broadcast
/// channel. Scenario 4 is the test that pins real emitter behavior (source,
/// id); never use this helper to assert on emitter output.
fn daemon_event(event: DaemonEvent) -> IpcMessage {
    IpcMessage {
        id: 0,
        source: ClientKind::default(),
        payload: IpcPayload::Event(event),
    }
}

fn sample_event() -> DaemonEvent {
    DaemonEvent::SyncCompleted {
        account_id: AccountId::new(),
        messages_synced: 0,
    }
}

fn pong(msg: &IpcMessage) -> bool {
    matches!(
        msg.payload,
        IpcPayload::Response(Response::Ok {
            data: ResponseData::Pong
        })
    )
}

/// Close one connection: drop the framed client so the serve loop hits EOF and
/// terminates, then join it — asserting the serve task actually finished and
/// did not panic (no silently-detached tasks). Deliberately does NOT touch the
/// shared shutdown signal, so a test can close one connection while others stay
/// up.
async fn close<S>(served: Served<S>) {
    drop(served.client);
    join_server(served.server).await;
}

/// Join a serve task, requiring it to terminate within the timeout and not
/// panic. A hung or panicked serve task is a test failure, never swallowed.
async fn join_server(server: JoinHandle<()>) {
    tokio::time::timeout(JOIN_TIMEOUT, server)
        .await
        .expect("serve task should terminate within the timeout")
        .expect("serve task should not panic");
}

/// Full single-connection teardown: close the connection, then drain the
/// state's background workers.
async fn finish<S>(state: &Arc<AppState>, served: Served<S>) {
    close(served).await;
    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Write a length-prefixed frame (4-byte big-endian length + payload), matching
/// `IpcCodec`'s framing, straight onto a raw stream — for byte-level edge tests.
async fn write_raw_frame<S: AsyncWrite + Unpin>(stream: &mut S, payload: &[u8]) {
    let len = u32::try_from(payload.len()).unwrap();
    stream.write_all(&len.to_be_bytes()).await.unwrap();
    stream.write_all(payload).await.unwrap();
    stream.flush().await.unwrap();
}

/// Assert a raw stream is closed by the peer within the timeout. Unlike the
/// strict framed EOF in [`Served::expect_eof`] (graceful shutdown), this helper
/// is used for the framing-error scenarios (oversized/malformed frames) where
/// the daemon aborts the connection — there, a clean EOF (`Ok(0)`) and a
/// connection reset (`Err`) are genuinely equivalent "the daemon dropped us"
/// outcomes. What must NOT happen is more frame bytes arriving.
async fn expect_raw_closed<S: AsyncRead + Unpin>(stream: &mut S) {
    let mut buf = [0u8; 64];
    let result = tokio::time::timeout(RECV_TIMEOUT, stream.read(&mut buf))
        .await
        .expect("close should be observed before timeout");
    match result {
        // A clean EOF (`Ok(0)`) and a connection reset (`Err`) are both "the
        // daemon dropped us"; only more frame bytes would be wrong.
        Ok(0) | Err(_) => {}
        Ok(n) => panic!(
            "expected the daemon to close, got {n} bytes: {:?}",
            &buf[..n]
        ),
    }
}

/// A fresh framed connection that answers `Ping` — used to prove the daemon is
/// still alive after another connection hit a framing/protocol edge. Leaves
/// worker teardown to the caller.
async fn assert_daemon_alive<C: Carrier>(
    state: &Arc<AppState>,
    hot: Arc<Semaphore>,
    bulk: Arc<Semaphore>,
) {
    let mut probe = serve::<C>(state.clone(), hot, bulk).await;
    probe.send(1, Request::Ping).await;
    let response = probe.recv_response(1).await;
    assert!(pong(&response), "daemon should still answer Ping");
    close(probe).await;
}

// ---------------------------------------------------------------------------
// Scenario corpus.
// ---------------------------------------------------------------------------

/// Scenario 1 — request/response id correlation. Client-chosen ids are echoed;
/// several requests in flight on one connection each get their own response.
async fn scenario_01_id_correlation_multiplexed<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    let ids = [7u64, 42, 1000, 3];
    for id in ids {
        served.send(id, Request::Ping).await;
    }

    let mut seen = std::collections::BTreeSet::new();
    for _ in ids {
        let msg = served.recv().await;
        assert!(pong(&msg), "each response should be a Pong, got {msg:?}");
        assert!(seen.insert(msg.id), "duplicate response id {}", msg.id);
    }
    assert_eq!(
        seen,
        ids.into_iter().collect(),
        "every client-chosen id should be echoed exactly once"
    );

    finish(&state, served).await;
}

/// Scenario 2 — out-of-order completion. A slow Bulk-lane request does not block
/// a fast Hot-lane request on the same connection; responses arrive by
/// completion order, matched by id.
async fn scenario_02_out_of_order_completion<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let gate = gate::install().await;
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    // Bulk request (id 1) enters the gate and blocks there.
    served.send(1, Request::RebuildAnalytics).await;
    gate.wait_until_entered(1).await;

    // Hot request (id 2) is issued while the bulk one is still in flight.
    served.send(2, Request::Ping).await;

    // The fast Hot response arrives first, correlated to its id.
    let first = served.recv().await;
    assert_eq!(
        first.id, 2,
        "hot response should arrive before the gated bulk"
    );
    assert!(pong(&first));

    // Releasing the gate lets the bulk response follow, matched by its id.
    gate.open();
    let second = served.recv_response(1).await;
    assert_eq!(second.id, 1);
    assert!(matches!(
        second.payload,
        IpcPayload::Response(Response::Ok { .. })
    ));

    // Release the serial gate lock before the (slower) teardown.
    drop(gate);
    finish(&state, served).await;
}

/// Scenario 3 — lane saturation. More concurrent Bulk requests than
/// `BULK_CONCURRENCY_LIMIT` (8) queue rather than fail: exactly the limit run
/// concurrently, the rest wait for a permit, and all complete once released.
async fn scenario_03_lane_saturation_queues<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let gate = gate::install().await;
    let mut served = serve::<C>(state.clone(), hot, bulk.clone()).await;

    let total = BULK_CONCURRENCY_LIMIT + 4;
    for id in 1..=total as u64 {
        served.send(id, Request::RebuildAnalytics).await;
    }

    // The bulk lane admits exactly its limit; the remainder are blocked on a
    // permit (or unread) — the daemon caps concurrency, it does not reject.
    gate.wait_until_entered(BULK_CONCURRENCY_LIMIT).await;
    // Permit exhaustion deterministically proves the Bulk lane is the limiter
    // (and catches any misrouting of the sentinel to the Hot lane): all
    // BULK_CONCURRENCY_LIMIT permits are held, so a further request cannot run.
    assert_eq!(
        bulk.available_permits(),
        0,
        "the bulk lane should be fully saturated (no free permits) while gated"
    );
    assert_eq!(
        gate.entered(),
        BULK_CONCURRENCY_LIMIT,
        "no more than the lane limit may run concurrently while gated"
    );

    // Release: the queued requests drain, none having failed.
    gate.open();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 1..=total {
        let msg = served.recv().await;
        assert!(
            matches!(msg.payload, IpcPayload::Response(Response::Ok { .. })),
            "queued request should complete Ok, got {msg:?}"
        );
        seen.insert(msg.id);
    }
    assert_eq!(
        seen,
        (1..=total as u64).collect(),
        "every queued request should eventually complete"
    );

    drop(gate);
    finish(&state, served).await;
}

/// Scenario 4 — a broadcast event reaches every connected client, with `id: 0`.
/// This is the test that PINS the emitter's output: it drives a real production
/// emitter (`chimes::emit_daemon_event` via [`emit`]) and asserts on the frame
/// that arrives at the client, not on a value the test wrote.
///
/// PINNED SURPRISE: the source is `ClientKind::default()` (== `Cli`), NOT
/// `Daemon`. Every production emitter (`chimes::emit_daemon_event`,
/// `diagnostics::emit_operation_event`, the server's own `SyncCompleted`) sets
/// `ClientKind::default()`. Pinned as-is; see the spec's "Pinned findings".
async fn scenario_04_broadcast_reaches_every_client<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();

    let mut a = serve::<C>(state.clone(), hot.clone(), bulk.clone()).await;
    let mut b = serve::<C>(state.clone(), hot.clone(), bulk.clone()).await;
    let mut c = serve::<C>(state.clone(), hot.clone(), bulk.clone()).await;

    // Real production emitter — the frame's `source` is whatever the daemon
    // actually stamps, so the assertions below pin production behavior.
    emit(&state);

    for client in [&mut a, &mut b, &mut c] {
        let frame = client.recv().await;
        assert_eq!(frame.id, 0, "broadcast events carry id 0");
        assert_eq!(
            frame.source,
            ClientKind::default(),
            "pinned: daemon events carry ClientKind::default() (Cli), not Daemon"
        );
        assert_ne!(
            frame.source,
            ClientKind::Daemon,
            "documents the divergence from the spec's 'source: Daemon' text"
        );
        assert!(matches!(
            frame.payload,
            IpcPayload::Event(DaemonEvent::SyncCompleted { .. })
        ));
    }

    for served in [a, b, c] {
        close(served).await;
    }
    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Scenario 5 — an event interleaves with an in-flight request on the same
/// connection without corrupting correlation (the `request_with_events` path).
async fn scenario_05_event_interleaves_with_inflight_request<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let gate = gate::install().await;
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    // Hold a request in flight.
    served.send(1, Request::RebuildAnalytics).await;
    gate.wait_until_entered(1).await;

    // An event pushed now must reach the client while the request is pending.
    emit(&state);
    let event = served.recv().await;
    assert_eq!(event.id, 0, "the interleaved event keeps id 0");
    assert!(matches!(event.payload, IpcPayload::Event(_)));

    // The in-flight request still completes, correlated to its own id.
    gate.open();
    let response = served.recv_response(1).await;
    assert_eq!(response.id, 1);

    drop(gate);
    finish(&state, served).await;
}

/// Scenario 6 — an event-only connection (never sends a request) receives
/// events: the `mxr events` / bridge `bridge_events` pattern.
async fn scenario_06_event_only_connection<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    // No request is ever sent on this connection.
    emit(&state);

    let frame = served.recv().await;
    assert_eq!(frame.id, 0);
    assert!(matches!(
        frame.payload,
        IpcPayload::Event(DaemonEvent::SyncCompleted { .. })
    ));

    finish(&state, served).await;
}

/// Scenario 7 — `EventsLagged { skipped }` is delivered point-to-point to a slow
/// consumer after the 256-slot broadcast channel overflows, and the connection
/// survives (still serves a subsequent request).
async fn scenario_07_events_lagged_resync_and_survive<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();

    // Overflow the channel (cap 256) after the receiver subscribes but before
    // the serve loop drains it, so the first recv() lags. Synthetic filler is
    // fine here — the content is irrelevant; scenario 4 pins emitter output.
    let mut served = serve_with_prep::<C>(state.clone(), hot, bulk, |s| {
        for _ in 0..400u32 {
            let _ = s.event_tx.send(daemon_event(sample_event()));
        }
    })
    .await;

    // The first frame is the resync signal, not a silently-truncated stream.
    let frame = served.recv().await;
    match frame.payload {
        IpcPayload::Event(DaemonEvent::EventsLagged { skipped }) => {
            assert!(skipped > 0, "skipped count should be positive");
        }
        other => panic!("expected EventsLagged, got {other:?}"),
    }

    // The connection survives: a request issued after the lag still round-trips
    // (skipping any remaining buffered events).
    served.send(9, Request::Ping).await;
    let response = served.recv_response(9).await;
    assert!(pong(&response), "connection should survive an events lag");

    finish(&state, served).await;
}

/// Scenario 8 — framing size edges. A frame near the 16 MiB cap round-trips
/// (request decoded, response returned); an oversized frame errors and tears
/// down only its own connection, leaving the daemon serving other connections.
async fn scenario_08_frame_size_edges<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();

    // -- near the limit round-trips ----------------------------------------
    // A DeleteLabel with a ~15.5 MiB name: both the request frame and the
    // (NotFound) response frame sit just under the 16 MiB cap and round-trip.
    let big_name = "A".repeat(15 * 1024 * 1024 + 512 * 1024);
    let msg = request(
        1,
        Request::DeleteLabel {
            name: big_name,
            account_id: None,
        },
    );
    let encoded_len = serde_json::to_vec(&msg).unwrap().len();
    assert!(
        encoded_len < MAX_FRAME_LEN && encoded_len > 15 * 1024 * 1024,
        "request frame should be near (just under) the 16 MiB cap: {encoded_len} bytes"
    );

    let mut served = serve::<C>(state.clone(), hot.clone(), bulk.clone()).await;
    served.client.send(msg).await.unwrap();
    let response = served.recv_response_within(1, BIG_FRAME_RECV_TIMEOUT).await;
    assert_eq!(response.id, 1, "a near-limit frame round-trips its id");
    close(served).await;

    // -- oversized frame errors without killing the daemon ------------------
    let (mut raw_client, server) =
        spawn_server::<C>(state.clone(), hot.clone(), bulk.clone(), |_| {}).await;
    let oversized = u32::try_from(MAX_FRAME_LEN).unwrap() + 1; // one byte over the cap
    raw_client
        .write_all(&oversized.to_be_bytes())
        .await
        .unwrap();
    raw_client.flush().await.unwrap();
    // The daemon rejects the length prefix and closes this connection.
    expect_raw_closed(&mut raw_client).await;
    drop(raw_client);
    join_server(server).await;

    // A separate connection still works: the oversized frame killed one
    // connection, not the daemon.
    assert_daemon_alive::<C>(&state, hot, bulk).await;
    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Scenario 9 — malformed JSON inside a valid length-prefixed frame. PINNED
/// BEHAVIOR: the decode fails (`InvalidData`), and the daemon closes the
/// connection WITHOUT sending any error frame back — the client just sees EOF.
/// A separate connection is unaffected.
async fn scenario_09_malformed_json_closes_connection<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();

    let (mut raw_client, server) =
        spawn_server::<C>(state.clone(), hot.clone(), bulk.clone(), |_| {}).await;
    write_raw_frame(&mut raw_client, b"{ this is not valid json ]").await;

    // No response frame is returned; the connection is simply closed.
    expect_raw_closed(&mut raw_client).await;
    drop(raw_client);
    join_server(server).await;

    assert_daemon_alive::<C>(&state, hot, bulk).await;
    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Scenario 10 — a truncated / mid-frame disconnect. The client announces a
/// frame length, sends a partial body, then disconnects. The server task cleans
/// up (completes) rather than leaking or hanging.
async fn scenario_10_truncated_frame_cleanup<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();

    let (mut raw_client, server) = spawn_server::<C>(state.clone(), hot, bulk, |_| {}).await;
    // Header claims 1000 bytes; send only 10, then disconnect mid-frame.
    raw_client.write_all(&1000u32.to_be_bytes()).await.unwrap();
    raw_client.write_all(&[0u8; 10]).await.unwrap();
    raw_client.flush().await.unwrap();
    drop(raw_client);

    // The connection task terminates cleanly (no leak / no hang).
    tokio::time::timeout(JOIN_TIMEOUT, server)
        .await
        .expect("serve task should terminate after a mid-frame disconnect")
        .expect("serve task should not panic");

    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Scenario 11 — client disconnect with a request in flight. The handler runs
/// to completion (or the send fails harmlessly) and, critically, the lane
/// permit is released — it is not wedged.
async fn scenario_11_disconnect_with_inflight_request_frees_permit<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let gate = gate::install().await;
    let mut served = serve::<C>(state.clone(), hot, bulk.clone()).await;

    served.send(1, Request::RebuildAnalytics).await;
    gate.wait_until_entered(1).await;
    assert_eq!(
        bulk.available_permits(),
        BULK_CONCURRENCY_LIMIT - 1,
        "the in-flight bulk request should hold one permit"
    );

    // Disconnect while the request is still gated in flight.
    drop(served.client);

    // Release the gated handler; its task completes and drops its permit.
    gate.open();
    tokio::time::timeout(JOIN_TIMEOUT, served.server)
        .await
        .expect("serve task should terminate")
        .expect("serve task should not panic");

    assert_eq!(
        bulk.available_permits(),
        BULK_CONCURRENCY_LIMIT,
        "the lane permit must be released, not wedged"
    );

    drop(gate);
    state.request_shutdown();
    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Scenario 12 — a handler panic becomes a kinded `Error` response (via
/// `guard_ipc_response`) and the connection stays usable for the next request.
async fn scenario_12_handler_panic_recovers<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account_id = state.default_account_id();
    state.add_sync_provider_for_test(Arc::new(PanicOnCreateLabel { account_id }));

    let (hot, bulk) = lanes();
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    // The panicking handler yields an Error response, correlated to its id.
    served
        .send(
            1,
            Request::CreateLabel {
                name: "boom".to_string(),
                color: None,
                account_id: None,
            },
        )
        .await;
    let response = served.recv_response(1).await;
    match response.payload {
        IpcPayload::Response(Response::Error { message, .. }) => {
            assert!(
                message.contains("panicked"),
                "error should report the panic, got {message:?}"
            );
        }
        other => panic!("expected a kinded Error response, got {other:?}"),
    }

    // The connection is still usable.
    served.send(2, Request::Ping).await;
    let after = served.recv_response(2).await;
    assert!(pong(&after), "connection should survive a handler panic");

    finish(&state, served).await;
}

/// Scenario 13 — the daemon shutdown signal closes idle connections cleanly:
/// the shutdown watch arm ends the serve loop and the client sees EOF, no frame.
async fn scenario_13_shutdown_closes_connections<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    // Signal shutdown on an idle connection.
    state.request_shutdown();

    // The connection closes cleanly — a strict EOF (`None`), no dangling frame
    // and no I/O error masquerading as a graceful close.
    served.expect_eof().await;
    join_server(served.server).await;

    state.shutdown_runtime_tasks(JOIN_TIMEOUT).await;
}

/// Scenario 14 — UDS auth posture (placeholder for the phase-5 auth matrix).
/// Today any local connection is accepted with no handshake: a fresh connection
/// answers `Ping` immediately, no `Authenticate` step required.
async fn scenario_14_uds_accepts_any_local_connection<C: Carrier>() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (hot, bulk) = lanes();
    let mut served = serve::<C>(state.clone(), hot, bulk).await;

    // No auth handshake — straight to a request.
    served.send(1, Request::Ping).await;
    let response = served.recv_response(1).await;
    assert!(
        pong(&response),
        "UDS accepts any local connection without authentication"
    );

    finish(&state, served).await;
}

// ---------------------------------------------------------------------------
// Carrier matrix: run every scenario over both carriers.
// ---------------------------------------------------------------------------

/// Instantiate each scenario as two `#[tokio::test]`s — one per carrier — so the
/// corpus runs unchanged over the UDS socketpair and the in-memory duplex. The
/// generated tests live at `carriers::<scenario>::{uds, duplex}`, keeping the
/// per-carrier result visible in the test output. The scenario bodies — and
/// every assertion in them — are shared and carrier-agnostic; only the byte
/// stream underneath differs.
macro_rules! run_on_both_carriers {
    ($($scenario:ident),+ $(,)?) => {
        mod carriers {
            $(
                mod $scenario {
                    #[tokio::test]
                    async fn uds() {
                        super::super::$scenario::<super::super::UdsCarrier>().await;
                    }

                    #[tokio::test]
                    async fn duplex() {
                        super::super::$scenario::<super::super::DuplexCarrier>().await;
                    }
                }
            )+
        }
    };
}

run_on_both_carriers! {
    scenario_01_id_correlation_multiplexed,
    scenario_02_out_of_order_completion,
    scenario_03_lane_saturation_queues,
    scenario_04_broadcast_reaches_every_client,
    scenario_05_event_interleaves_with_inflight_request,
    scenario_06_event_only_connection,
    scenario_07_events_lagged_resync_and_survive,
    scenario_08_frame_size_edges,
    scenario_09_malformed_json_closes_connection,
    scenario_10_truncated_frame_cleanup,
    scenario_11_disconnect_with_inflight_request_frees_permit,
    scenario_12_handler_panic_recovers,
    scenario_13_shutdown_closes_connections,
    scenario_14_uds_accepts_any_local_connection,
}
