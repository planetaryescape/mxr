//! Reusable transport-contract conformance checks (behind the `conformance`
//! feature).
//!
//! The provider precedent (`crates/provider-fake/src/conformance.rs`,
//! `run_sync_conformance<P>` / `run_send_conformance<P>`): the reference crate
//! exports generic functions that any adapter calls from a normal
//! `#[tokio::test]` to prove its implementation upholds the contract. This
//! module is the transport analog — it lives in `mxr-transport` so an
//! **out-of-tree** transport crate can add `mxr-transport` (with the
//! `conformance` feature) as a dev-dependency, wire its own [`ServerTransport`] /
//! [`Connector`] pair, and verify conformance **without depending on the daemon**.
//!
//! ## What this suite covers, and what it deliberately does not
//!
//! This is the honest split of the phase-2..5 corpus:
//!
//! - **Transport contract (here):** the [`ServerTransport`] / [`TransportListener`]
//!   / [`Connector`] lifecycle — bind, accept yields a connected byte stream plus
//!   [`PeerInfo`], the stream is genuinely bidirectional, `accept` is cancel-safe,
//!   `stop_accepting` refuses further connections without hanging, and `cleanup`
//!   is idempotent. It is **protocol-free**: it never frames an [`crate::AsyncReadWrite`]
//!   through `IpcCodec`, never sends a `Request`, and never touches an `AppState`.
//! - **Protocol behavior (NOT here):** id correlation, out-of-order completion,
//!   lane back-pressure, event fan-out, framing edges, the `Authenticate` gate.
//!   Those exercise the daemon's serve core (`serve_client_connection`,
//!   `AppState`, the lane semaphores) and stay in the daemon's in-tree corpus
//!   (`crates/daemon/src/serve/ipc_conformance.rs`), which already runs every
//!   scenario over the real UDS / in-memory / TCP transports. That behavior is
//!   transport-independent by construction, so an adapter author does not need
//!   to re-prove it — they only need to prove their byte stream and its
//!   lifecycle behave, which is exactly this suite.
//!
//! For a token transport ([`crate::PeerAuth::TokenRequired`]) also call
//! [`run_token_auth_conformance`], which pins the transport half of the auth
//! contract (capabilities advertise a token, accept surfaces `TokenRequired`);
//! the protocol handshake itself is the daemon's serve-core corpus.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{BoxedIo, Connector, PeerAuth, ServerTransport, TransportCapabilities};

/// Upper bound on any single "this must not hang" wait. Generous: the checks are
/// rendezvous-driven, so this only trips on a genuine wedge, never on latency.
const WAIT: Duration = Duration::from_secs(5);

/// Longer bound for the multi-MiB frame round-trip — transferring ~16 MiB can be
/// slow on a loaded CI box (the daemon corpus uses the same 30s allowance for
/// its near-limit frame scenario).
const LARGE_WAIT: Duration = Duration::from_secs(30);

/// The maximum frame the wire protocol can put on a transport, in bytes.
///
/// This MUST match `IpcCodec`'s frame cap (`crates/protocol/src/codec.rs`: 16
/// MiB). It is duplicated as a documented constant here — rather than depending
/// on `mxr-protocol` — precisely to keep this crate a pure byte-stream leaf: a
/// transport carries opaque bytes and never parses a frame, so it needs the
/// *size* of the protocol's byte envelope, not the protocol types. A transport
/// that cannot round-trip a payload this large would silently fail the daemon's
/// near-limit-frame scenario in production, so the transport suite proves the
/// envelope here (see [`run_transport_conformance`]).
const MAX_PROTOCOL_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Chunk size for streaming the large-frame round-trip. A single 16 MiB write
/// would deadlock a socket transport (the kernel buffer is far smaller than the
/// payload), so writer and reader run concurrently and move the payload in
/// bounded chunks.
const LARGE_CHUNK: usize = 64 * 1024;

/// Run the transport-contract conformance suite against a `transport` and a
/// `connector` pre-wired to the same endpoint.
///
/// Works for every transport, token or implicit-trust. Call it from a
/// `#[tokio::test]`:
///
/// ```ignore
/// #[tokio::test]
/// async fn my_transport_conforms() {
///     let transport = MyServerTransport::new(/* … */);
///     let connector = MyConnector::new(/* same endpoint */);
///     mxr_transport::conformance::run_transport_conformance(&transport, &connector).await;
/// }
/// ```
///
/// # Panics
///
/// Panics (fails the test) on any contract violation.
pub async fn run_transport_conformance<T, C>(transport: &T, connector: &C)
where
    T: ServerTransport + ?Sized,
    C: Connector + ?Sized,
{
    // --- static contract: name + capability coherence ----------------------
    assert!(
        !transport.name().trim().is_empty(),
        "a transport must report a stable, non-empty name"
    );
    let caps = transport.capabilities();
    if caps.auth.token {
        assert!(
            !caps.auth.implicit_peer_identity,
            "a token transport carries no implicit OS peer identity: \
             auth.token and auth.implicit_peer_identity must not both be set"
        );
    }

    // --- bind ---------------------------------------------------------------
    let mut listener = transport.bind().await.expect("bind should succeed");
    assert!(
        !listener.endpoint().trim().is_empty(),
        "a bound listener must describe a non-empty endpoint"
    );
    assert!(
        !connector.describe().trim().is_empty(),
        "a connector must describe a non-empty endpoint"
    );

    // --- reachable while accepting: connect, accept, and prove the stream is
    //     a real bidirectional pipe; the accepted peer must match the
    //     advertised capabilities ---------------------------------------------
    let mut client = connector
        .connect()
        .await
        .expect("connect should succeed while the listener is accepting");
    let (mut server, peer) = within(listener.accept())
        .await
        .expect("accept should succeed while accepting");
    assert_peer_matches_caps(&peer.auth, caps, connector.auth_token().is_some());
    round_trip(&mut client, &mut server).await;
    // The transport must carry the protocol's full byte envelope — a payload at
    // the 16 MiB frame cap round-trips intact each way. Without this, a transport
    // capped below the cap would pass a tiny round-trip yet fail the daemon's
    // near-limit-frame scenario in production; proving it here is what lets the
    // protocol corpus stay out-of-tree (D057).
    round_trip_large(&mut client, &mut server).await;
    drop(client);
    drop(server);

    // --- cancel-safety: an `accept` future that registered interest (polled to
    //     Pending) and is then dropped must NOT lose a connection that arrives in
    //     that window — the daemon's accept loop `select_all`s several listeners
    //     and drops the losing branch each round. Ordering is load-bearing: poll
    //     to Pending FIRST (empty backlog), THEN connect, THEN drop the parked
    //     future WITHOUT polling it again, THEN a fresh accept must recover the
    //     connection. (A connection already peeled off by a completed poll is
    //     allowed to be lost per the trait doc; this tests the parked case.) -----
    let mut client: BoxedIo;
    {
        let accept = listener.accept();
        tokio::pin!(accept);
        assert!(
            futures::poll!(accept.as_mut()).is_pending(),
            "accept must park (Pending) when no client is waiting yet"
        );
        // A client arrives while the accept is parked...
        client = connector
            .connect()
            .await
            .expect("connect should succeed while an accept is parked");
        // ...and the parked accept is dropped here (end of scope) without being
        // polled again — exactly how `select!` abandons the losing branch.
    }
    // The connection must survive: a fresh accept still yields it (no loss).
    let (mut server, _peer) = within(listener.accept())
        .await
        .expect("a dropped parked accept must not lose the queued connection");
    round_trip(&mut client, &mut server).await;
    drop(client);
    drop(server);

    // --- stop_accepting: new connections are refused promptly (never hang),
    //     and accept fails rather than blocking forever ----------------------
    within(listener.stop_accepting()).await;

    let accept_after_stop = within(listener.accept()).await;
    assert!(
        accept_after_stop.is_err(),
        "accept must fail (not hang, not succeed) after stop_accepting"
    );

    match within(connector.connect()).await {
        // Refused outright — the strong, common case (UDS / TCP / memory).
        Err(_) => {}
        // A community transport may hand back a stream that immediately EOFs;
        // that is acceptable, a live connection is not.
        Ok(mut stream) => {
            let mut buf = [0u8; 1];
            let read = within(stream.read(&mut buf)).await.unwrap_or(0);
            assert_eq!(
                read, 0,
                "a connection opened after stop_accepting must not be live (expected EOF)"
            );
        }
    }

    // --- cleanup is idempotent ---------------------------------------------
    listener.cleanup().await.expect("cleanup should succeed");
    listener
        .cleanup()
        .await
        .expect("cleanup should be idempotent");
}

/// Pin the transport half of the auth contract for a token transport
/// ([`PeerAuth::TokenRequired`]).
///
/// Call this **in addition** to [`run_transport_conformance`] when the transport
/// requires a bearer token. It verifies the transport-level obligations —
/// capabilities advertise a token, the connector carries one, and accept
/// surfaces `TokenRequired`. The `Authenticate` protocol handshake it gates is
/// the daemon serve core's responsibility, covered by the in-tree corpus.
///
/// # Panics
///
/// Panics (fails the test) on any contract violation, including being called for
/// a non-token transport.
pub async fn run_token_auth_conformance<T, C>(transport: &T, connector: &C)
where
    T: ServerTransport + ?Sized,
    C: Connector + ?Sized,
{
    let caps = transport.capabilities();
    assert!(
        caps.auth.token,
        "run_token_auth_conformance is for token transports (auth.token must be set)"
    );
    assert!(
        !caps.auth.implicit_peer_identity,
        "a token transport must not also advertise an implicit peer identity"
    );
    assert!(
        connector.auth_token().is_some(),
        "a token transport's connector must advertise a bearer token"
    );

    let mut listener = transport.bind().await.expect("bind should succeed");
    let _client = connector
        .connect()
        .await
        .expect("connect should succeed while accepting");
    let (_server, peer) = within(listener.accept())
        .await
        .expect("accept should succeed");
    assert_eq!(
        peer.auth,
        PeerAuth::TokenRequired,
        "a token transport must surface PeerAuth::TokenRequired on accept"
    );

    listener.stop_accepting().await;
    let _ = listener.cleanup().await;
}

/// The accepted peer's evidence, the advertised capabilities, and the
/// connector's token must be **tri-coherent**:
///
/// > `connector.auth_token().is_some()`  ⟺  `capabilities.auth.token == true`
/// > ⟺  the accepted `PeerAuth` is `TokenRequired`.
///
/// A mismatch is a real bug: e.g. a connector that advertises a token while the
/// transport is not `TokenRequired` would make the shared IPC client send an
/// unwanted `Authenticate` handshake a trusting daemon never asked for. A real
/// `UnixPeer` additionally requires the implicit-identity capability;
/// `LocalProcess` (in-process) is trusted by construction.
fn assert_peer_matches_caps(
    auth: &PeerAuth,
    caps: TransportCapabilities,
    connector_has_token: bool,
) {
    let peer_is_token_required = matches!(auth, PeerAuth::TokenRequired);
    assert_eq!(
        caps.auth.token, peer_is_token_required,
        "auth.token capability must match whether the accepted peer is TokenRequired"
    );
    assert_eq!(
        connector_has_token, peer_is_token_required,
        "connector.auth_token().is_some() must match whether the accepted peer is TokenRequired"
    );
    match auth {
        PeerAuth::TokenRequired => {
            assert!(
                !caps.auth.implicit_peer_identity,
                "a TokenRequired peer must not also claim an implicit identity"
            );
        }
        PeerAuth::UnixPeer { .. } => {
            assert!(
                caps.auth.implicit_peer_identity,
                "a UnixPeer transport must advertise auth.implicit_peer_identity"
            );
        }
        PeerAuth::LocalProcess => {}
    }
}

/// Write a small payload each way and assert it arrives intact — proof the
/// accepted stream is a genuine bidirectional pipe, not a one-way or dead one.
async fn round_trip(client: &mut BoxedIo, server: &mut BoxedIo) {
    client
        .write_all(b"ping")
        .await
        .expect("client should write to the stream");
    client.flush().await.expect("client should flush");
    let mut to_server = [0u8; 4];
    within(server.read_exact(&mut to_server))
        .await
        .expect("the server end should read the client's bytes");
    assert_eq!(
        &to_server, b"ping",
        "bytes must arrive intact client -> server"
    );

    server
        .write_all(b"pong")
        .await
        .expect("server should write to the stream");
    server.flush().await.expect("server should flush");
    let mut to_client = [0u8; 4];
    within(client.read_exact(&mut to_client))
        .await
        .expect("the client end should read the server's bytes");
    assert_eq!(
        &to_client, b"pong",
        "bytes must arrive intact server -> client"
    );
}

/// Round-trip a payload at the protocol's maximum frame size
/// ([`MAX_PROTOCOL_FRAME_BYTES`]) each way, proving the transport carries the
/// full byte envelope the wire protocol can put on it. Writer and reader run
/// concurrently — a 16 MiB payload dwarfs any socket buffer, so a sequential
/// write-then-read would deadlock a socket transport.
async fn round_trip_large(client: &mut BoxedIo, server: &mut BoxedIo) {
    let len = MAX_PROTOCOL_FRAME_BYTES;
    let both = async {
        tokio::join!(write_filled(client, len), read_filled(server, len));
    };
    tokio::time::timeout(LARGE_WAIT, both)
        .await
        .expect("a max-size frame must round-trip client -> server without hanging");

    let both = async {
        tokio::join!(write_filled(server, len), read_filled(client, len));
    };
    tokio::time::timeout(LARGE_WAIT, both)
        .await
        .expect("a max-size frame must round-trip server -> client without hanging");
}

/// Stream `len` bytes of a fixed sentinel pattern onto `io` in bounded chunks.
async fn write_filled(io: &mut BoxedIo, len: usize) {
    let chunk = vec![0xA5u8; LARGE_CHUNK];
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(LARGE_CHUNK);
        io.write_all(&chunk[..n])
            .await
            .expect("large-frame write should succeed");
        remaining -= n;
    }
    io.flush().await.expect("large-frame flush should succeed");
}

/// Read `len` bytes from `io` in bounded chunks, asserting every byte is the
/// sentinel pattern — proof the whole payload arrived intact, not truncated.
async fn read_filled(io: &mut BoxedIo, len: usize) {
    let mut buf = vec![0u8; LARGE_CHUNK];
    let mut remaining = len;
    while remaining > 0 {
        let n = remaining.min(LARGE_CHUNK);
        io.read_exact(&mut buf[..n])
            .await
            .expect("large-frame read should succeed");
        assert!(
            buf[..n].iter().all(|&b| b == 0xA5),
            "large-frame bytes must arrive intact"
        );
        remaining -= n;
    }
}

/// Await `future` under [`WAIT`], failing the test loudly (via `expect`) if it
/// does not resolve — a hung transport operation is a conformance failure, never
/// a silent stall.
async fn within<F: std::future::Future>(future: F) -> F::Output {
    tokio::time::timeout(WAIT, future)
        .await
        .expect("transport operation must not hang")
}

#[cfg(test)]
mod tests {
    //! Self-tests: the in-tree transports must pass their own conformance suite,
    //! one per `PeerAuth` variant.
    //!
    //! - `UdsServerTransport` covers `UnixPeer` (real OS peer creds).
    //! - `MemoryTransport` covers `LocalProcess` (in-process).
    //! - `TokenFixture` (below) covers `TokenRequired` — a deterministic,
    //!   duplex-backed token transport, so the token path of the suite (and
    //!   `run_token_auth_conformance`) is exercised without an ephemeral-port
    //!   bind race. The real `TcpServerTransport`'s own behavior is covered by
    //!   `tcp::tests` and the daemon's TCP corpus harness.

    use super::{run_token_auth_conformance, run_transport_conformance};
    use crate::{UdsServerTransport, UnixConnector};

    #[tokio::test]
    async fn uds_transport_conforms() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conformance.sock");
        let transport = UdsServerTransport::new(path.clone());
        let connector = UnixConnector::new(path);
        run_transport_conformance(&transport, &connector).await;
    }

    #[cfg(feature = "test-util")]
    #[tokio::test]
    async fn memory_transport_conforms() {
        use crate::MemoryTransport;
        let transport = MemoryTransport::new();
        let connector = transport.connector();
        run_transport_conformance(&transport, &connector).await;
    }

    #[tokio::test]
    async fn token_transport_conforms() {
        // Fresh instance per suite call: like `MemoryTransport`, this duplex
        // fixture shares one queue across binds and `stop_accepting` closes it
        // for good, so each conformance run (which does one bind lifecycle) gets
        // its own transport. Re-bindable transports (TCP/UDS) can reuse one.
        let transport = token_fixture::TokenFixture::new();
        let connector = transport.connector();
        run_transport_conformance(&transport, &connector).await;

        let transport = token_fixture::TokenFixture::new();
        let connector = transport.connector();
        run_token_auth_conformance(&transport, &connector).await;
    }

    /// A minimal duplex-backed transport that reports `TokenRequired` — the
    /// `TokenRequired` reference for the suite's own tests. No sockets, no ports,
    /// fully deterministic. (A trimmed sibling of `MemoryTransport` with token
    /// capabilities instead of `LocalProcess`.)
    mod token_fixture {
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex};

        use async_trait::async_trait;
        use tokio::io::DuplexStream;
        use tokio::sync::Notify;

        use crate::{
            AuthCaps, BoxedIo, Connector, LifecycleCaps, LocalityCaps, PeerAuth, PeerInfo, Result,
            ServerTransport, TransportCapabilities, TransportError, TransportListener,
        };

        const TOKEN: &str = "fixture-token";

        struct Shared {
            pending: Mutex<Option<VecDeque<DuplexStream>>>,
            ready: Notify,
        }

        #[derive(Clone)]
        pub(super) struct TokenFixture {
            inner: Arc<Shared>,
        }

        impl TokenFixture {
            pub(super) fn new() -> Self {
                Self {
                    inner: Arc::new(Shared {
                        pending: Mutex::new(Some(VecDeque::new())),
                        ready: Notify::new(),
                    }),
                }
            }

            pub(super) fn connector(&self) -> TokenConnector {
                TokenConnector {
                    inner: self.inner.clone(),
                }
            }
        }

        #[async_trait]
        impl ServerTransport for TokenFixture {
            fn name(&self) -> &str {
                "token-fixture"
            }

            fn capabilities(&self) -> TransportCapabilities {
                TransportCapabilities {
                    locality: LocalityCaps { same_machine: true },
                    auth: AuthCaps {
                        implicit_peer_identity: false,
                        token: true,
                    },
                    lifecycle: LifecycleCaps {
                        client_autostart: false,
                    },
                }
            }

            async fn bind(&self) -> Result<Box<dyn TransportListener>> {
                Ok(Box::new(TokenListener {
                    inner: self.inner.clone(),
                }))
            }
        }

        struct TokenListener {
            inner: Arc<Shared>,
        }

        #[async_trait]
        impl TransportListener for TokenListener {
            async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)> {
                loop {
                    let taken = {
                        let mut guard = self.inner.pending.lock().expect("lock");
                        match guard.as_mut() {
                            None => {
                                return Err(TransportError::Accept(std::io::Error::new(
                                    std::io::ErrorKind::NotConnected,
                                    "listener stopped accepting",
                                )))
                            }
                            Some(queue) => queue.pop_front(),
                        }
                    };
                    match taken {
                        Some(server_end) => {
                            return Ok((
                                Box::new(server_end),
                                PeerInfo {
                                    auth: PeerAuth::TokenRequired,
                                },
                            ))
                        }
                        None => self.inner.ready.notified().await,
                    }
                }
            }

            async fn stop_accepting(&mut self) {
                *self.inner.pending.lock().expect("lock") = None;
                self.inner.ready.notify_waiters();
            }

            async fn cleanup(&mut self) -> Result<()> {
                Ok(())
            }

            fn endpoint(&self) -> String {
                "token-fixture:".to_string()
            }
        }

        #[derive(Clone)]
        pub(super) struct TokenConnector {
            inner: Arc<Shared>,
        }

        #[async_trait]
        impl Connector for TokenConnector {
            async fn connect(&self) -> Result<BoxedIo> {
                // A modest buffer: the large-frame check streams concurrently,
                // so any size works; this keeps 16 MiB brisk.
                let (server_end, client_end) = tokio::io::duplex(64 * 1024);
                let mut guard = self.inner.pending.lock().expect("lock");
                match guard.as_mut() {
                    Some(queue) => queue.push_back(server_end),
                    None => {
                        return Err(TransportError::Connect {
                            endpoint: "token-fixture:".to_string(),
                            source: std::io::Error::new(
                                std::io::ErrorKind::ConnectionRefused,
                                "listener stopped accepting",
                            ),
                        })
                    }
                }
                drop(guard);
                self.inner.ready.notify_one();
                Ok(Box::new(client_end))
            }

            fn describe(&self) -> String {
                "token-fixture:".to_string()
            }

            fn auth_token(&self) -> Option<&str> {
                Some(TOKEN)
            }
        }
    }
}
