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
    assert_peer_matches_caps(&peer.auth, caps);
    round_trip(&mut client, &mut server).await;
    drop(client);
    drop(server);

    // --- cancel-safety: an `accept` future that is polled (registers interest)
    //     then dropped before a client arrives must NOT wedge the listener --
    {
        let accept = listener.accept();
        tokio::pin!(accept);
        let polled = futures::poll!(accept.as_mut());
        assert!(
            polled.is_pending(),
            "accept must be pending when no client is waiting (nothing to consume yet)"
        );
        // `accept` is dropped here: the required cancel-safe drop.
    }
    // The listener must still accept a real connection after the cancelled poll.
    let mut client = connector
        .connect()
        .await
        .expect("connect should succeed after a cancelled accept");
    let (mut server, _peer) = within(listener.accept())
        .await
        .expect("a cancelled accept must not lose the next connection");
    round_trip(&mut client, &mut server).await;
    drop(client);
    drop(server);

    // --- stop_accepting: new connections are refused promptly (never hang),
    //     and accept fails rather than blocking forever ----------------------
    listener.stop_accepting().await;

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

/// The accepted peer's evidence must be coherent with what the transport
/// advertises: a `TokenRequired` peer requires the token capability and no
/// implicit identity; a real `UnixPeer` requires the implicit-identity
/// capability. `LocalProcess` (in-process) is trusted by construction and puts
/// no requirement on the capability flags.
fn assert_peer_matches_caps(auth: &PeerAuth, caps: TransportCapabilities) {
    match auth {
        PeerAuth::TokenRequired => {
            assert!(
                caps.auth.token,
                "a TokenRequired peer requires the auth.token capability"
            );
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
    //! Self-tests: the in-tree transports must pass their own conformance suite.

    use super::{run_token_auth_conformance, run_transport_conformance};
    use crate::{
        ServerTransport, TcpConnector, TcpServerTransport, UdsServerTransport, UnixConnector,
    };

    #[tokio::test]
    async fn uds_transport_conforms() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("conformance.sock");
        let transport = UdsServerTransport::new(path.clone());
        let connector = UnixConnector::new(path);
        run_transport_conformance(&transport, &connector).await;
    }

    #[tokio::test]
    async fn tcp_transport_conforms() {
        // Discover a free loopback port (bind `:0`, read it, release it), then
        // build the transport + connector on that concrete port so the suite's
        // own bind and the connector agree on the address. tokio sets
        // SO_REUSEADDR, so the just-freed port re-binds without a TIME_WAIT stall.
        let probe = TcpServerTransport::new("127.0.0.1:0".parse().expect("addr"))
            .bind()
            .await
            .expect("probe bind");
        let addr = probe
            .endpoint()
            .strip_prefix("tcp://")
            .and_then(|authority| authority.parse().ok())
            .expect("tcp endpoint should be tcp://host:port");
        drop(probe);

        let transport = TcpServerTransport::new(addr);
        let connector = TcpConnector::new(addr, Some("conformance-token".to_string()));
        run_transport_conformance(&transport, &connector).await;
        run_token_auth_conformance(&transport, &connector).await;
    }

    #[cfg(feature = "test-util")]
    #[tokio::test]
    async fn memory_transport_conforms() {
        use crate::MemoryTransport;
        let transport = MemoryTransport::new();
        let connector = transport.connector();
        run_transport_conformance(&transport, &connector).await;
    }
}
