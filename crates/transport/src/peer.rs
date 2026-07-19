//! Peer auth evidence surfaced on accept.
//!
//! The Tailscale lesson (discovery §5): identity evidence is per-transport, so
//! the transport contract must surface it — not just bytes. UDS carries OS peer
//! credentials; a token-bearing transport carries none until the peer
//! authenticates. This phase only *plumbs* the evidence into the dispatch
//! context; no policy reads it yet (phase 5's token gate does).

/// How a peer's identity is (or is not yet) established.
///
/// Additive: future transports add variants (`TlsClientCert { .. }`,
/// `PipeIdentity { .. }`) without disturbing the ones that exist today.
///
/// The credential-bearing variant carries only *real* evidence: `UnixPeer`
/// always means the OS reported these credentials for this connection. A
/// transport that cannot establish an identity uses a distinct variant
/// ([`Self::LocalProcess`] for in-process, [`Self::TokenRequired`] for a
/// transport that must authenticate) — it never fabricates a `UnixPeer`. Phase
/// 5's token gate can therefore match `UnixPeer` and KNOW the creds are real.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerAuth {
    /// OS-level Unix peer credentials (`SO_PEERCRED` / `getpeereid`, surfaced by
    /// tokio's `UnixStream::peer_cred`). Always real — an accept that cannot
    /// read them fails closed rather than fabricating this variant. `pid` is
    /// `None` on platforms that do not report it.
    UnixPeer {
        /// Peer effective user id.
        uid: u32,
        /// Peer effective group id.
        gid: u32,
        /// Peer process id, when the platform reports it.
        pid: Option<i32>,
    },
    /// The peer is this process: an in-memory transport or a daemon-internal
    /// re-dispatch. Implicitly trusted; no OS-level peer to interrogate.
    LocalProcess,
    /// The transport provides no implicit identity; dispatch must demand a token
    /// before trusting the peer (phase 5).
    TokenRequired,
}

/// Auth evidence for one accepted connection. A struct (not a bare [`PeerAuth`])
/// so it can grow additively without churning every accept site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// The peer's auth evidence.
    pub auth: PeerAuth,
}

impl PeerInfo {
    /// An in-process peer: the in-memory transport, and daemon-internal
    /// re-dispatch (where a handler issues another request in-process) or unit
    /// tests that drive the serve core without a real transport. Carries
    /// [`PeerAuth::LocalProcess`] — an explicit "this process," never a
    /// fabricated `UnixPeer`.
    #[must_use]
    pub fn local() -> Self {
        Self {
            auth: PeerAuth::LocalProcess,
        }
    }
}
