//! Transport seam for the mxr daemon (Phase 4, transport-adapter initiative).
//!
//! The daemon speaks one frozen wire protocol (`mxr-protocol`) over *some* byte
//! stream. This crate abstracts only "where the bytes come from": a
//! [`ServerTransport`] binds a [`TransportListener`] that yields connected byte
//! streams plus [`PeerInfo`] auth evidence, and a [`Connector`] dials one from
//! the client side. Everything above the stream — framing, dispatch, event
//! fan-out — stays in the daemon and is transport-neutral.
//!
//! The shape mirrors the provider adapter system (`docs/blueprint/03-providers.md`):
//! object-safe `#[async_trait]` traits over concrete shared types, no associated
//! types in the trait objects, consumed as `Box<dyn _>`; namespaced additive
//! [`TransportCapabilities`] with all-false defaults that the daemon trusts.
//!
//! Adapters this phase:
//! - [`UdsServerTransport`] / [`UnixConnector`] — the Unix domain socket, the
//!   default and only production transport (absorbs the socket lifecycle that
//!   used to live inline in the daemon's `server.rs`).
//! - `MemoryTransport` (behind the `test-util` feature) — an in-memory duplex
//!   pair, the "fake provider" analog for the conformance corpus. (Plain code
//!   span, not an intra-doc link: the type is compiled out of the default
//!   feature set, so a link would dangle when docs build without `test-util`.)
//!
//! See `docs/transport-adapters/04-transport-traits.md` for the design.

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

mod addr;
mod caps;
mod error;
mod peer;
mod uds;

#[cfg(feature = "test-util")]
mod memory;

pub use addr::{TransportAddr, DAEMON_ADDR_ENV};
pub use caps::{AuthCaps, LifecycleCaps, LocalityCaps, TransportCapabilities};
pub use error::{Result, TransportError};
pub use peer::{PeerAuth, PeerInfo};
pub use uds::{UdsServerTransport, UnixConnector};

#[cfg(feature = "test-util")]
pub use memory::{MemoryConnector, MemoryTransport};

/// A connected, bidirectional byte stream — the unit every transport produces.
///
/// Blanket-implemented for any `AsyncRead + AsyncWrite`, so a trait object of it
/// is what a [`TransportListener`] hands the serve core and what a [`Connector`]
/// returns. A trait object implements its supertraits, so
/// `Box<dyn AsyncReadWrite + Send + Unpin>` is itself `AsyncRead + AsyncWrite`
/// and drops straight into `Framed<_, IpcCodec>`.
pub trait AsyncReadWrite: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> AsyncReadWrite for T {}

/// One boxed connection. The single boxed indirection per connection is noise
/// next to JSON serialization; provider crates made the same `dyn` trade.
pub type BoxedIo = Box<dyn AsyncReadWrite + Send + Unpin>;

/// A server-side transport: binds once at daemon startup and produces
/// connections. The daemon names concrete implementors only in its transport
/// factory (provider pattern); everything downstream is `dyn`.
#[async_trait]
pub trait ServerTransport: Send + Sync {
    /// Stable adapter name for logs/status (`"uds"`, `"memory"`, …).
    fn name(&self) -> &str;

    /// Namespaced capabilities. The daemon trusts these (provider rule); the
    /// phase-6 skeleton ships all-false.
    fn capabilities(&self) -> TransportCapabilities;

    /// Bind and return a listener. Called once per configured transport at
    /// daemon startup.
    async fn bind(&self) -> Result<Box<dyn TransportListener>>;
}

/// A bound listener: accepts connections and owns any transport-level resource
/// (a socket file, an in-memory queue) that must be released on shutdown.
///
/// ## Shutdown ordering
///
/// Graceful shutdown is three ordered steps: [`stop_accepting`](Self::stop_accepting)
/// (refuse new clients), drain in-flight connections, then [`cleanup`](Self::cleanup)
/// (release the resource). Splitting the first and last steps is deliberate —
/// see each method.
#[async_trait]
pub trait TransportListener: Send {
    /// Accept the next connection: a byte stream plus the peer's auth evidence.
    ///
    /// **Cancel-safety (required):** the returned future may be dropped before
    /// completion — the daemon's accept loop polls several listeners with
    /// `select!`/`select_all` and drops the losers each round. An
    /// implementation MUST NOT lose or leak a connection when its `accept`
    /// future is dropped before it resolves (a connection already peeled off
    /// the OS backlog is fine; one that was never returned must remain
    /// acceptable on the next call). `tokio`'s `UnixListener::accept` satisfies
    /// this; custom transports must uphold it.
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)>;

    /// Stop accepting new connections: close the listening endpoint so new
    /// clients are refused promptly (they must NOT hang), WITHOUT releasing the
    /// transport resource. Called before the connection drain; the resource
    /// release is deferred to [`cleanup`](Self::cleanup) so a successor that
    /// re-bound the endpoint during the drain window is not clobbered.
    /// Idempotent; after it returns, `accept` must fail rather than block.
    async fn stop_accepting(&mut self);

    /// Release transport-owned resources (socket file, …). Idempotent; runs
    /// LAST, after in-flight connections drain. The pid file and daemon
    /// singleton guard are daemon lifecycle, not transport lifecycle, and stay
    /// in the daemon.
    async fn cleanup(&mut self) -> Result<()>;

    /// Human-readable endpoint for logs/status (`"unix:/path"`).
    fn endpoint(&self) -> String;
}

/// A client-side dialer: opens one connection to a daemon endpoint. The shared
/// [`mxr_client::IpcConnection`](../mxr_client/struct.IpcConnection.html) is
/// generic over this.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Open one connection to the daemon.
    async fn connect(&self) -> Result<BoxedIo>;

    /// Human-readable description of what this connector dials (`"unix:/path"`).
    fn describe(&self) -> String;
}
