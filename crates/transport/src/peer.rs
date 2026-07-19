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
/// `PipeIdentity { .. }`) without disturbing the two that exist today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerAuth {
    /// OS-level Unix peer credentials (`SO_PEERCRED` / `getpeereid`, surfaced by
    /// tokio's `UnixStream::peer_cred`). `pid` is `None` on platforms that do
    /// not report it.
    UnixPeer {
        /// Peer effective user id.
        uid: u32,
        /// Peer effective group id.
        gid: u32,
        /// Peer process id, when the platform reports it.
        pid: Option<i32>,
    },
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
    /// An in-process / internally-dispatched peer: no OS-level peer to
    /// interrogate. Used by daemon-internal re-dispatch (where a handler issues
    /// another request in-process) and by unit tests that drive the serve core
    /// without a real transport. Represented as an *unresolved* `UnixPeer`
    /// (`uid`/`gid` == `u32::MAX`, i.e. "not resolved"; `pid` is this process).
    /// Never consulted for policy this phase.
    #[must_use]
    pub fn local() -> Self {
        Self {
            auth: PeerAuth::UnixPeer {
                uid: u32::MAX,
                gid: u32::MAX,
                pid: Some(std::process::id() as i32),
            },
        }
    }
}
