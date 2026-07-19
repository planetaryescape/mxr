//! Namespaced, additive transport capabilities.
//!
//! Grouped into `locality`, `auth`, and `lifecycle` namespaces, every field a
//! `bool` defaulting to `false` (via `#[derive(Default)]`). The provider-
//! capabilities lesson (`crates/core/src/types.rs` `SyncCapabilities`): adding a
//! field never means "unsupported," only "the older shape doesn't carry this
//! signal yet." A missing/false signal is the safe default. The daemon trusts
//! whatever an adapter advertises.

/// The full capability set an adapter advertises. All-false by default: a
/// transport opts in to each property it genuinely provides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportCapabilities {
    /// Where the peer runs relative to the daemon.
    pub locality: LocalityCaps,
    /// What auth evidence the transport carries.
    pub auth: AuthCaps,
    /// Lifecycle affordances the transport supports.
    pub lifecycle: LifecycleCaps,
}

/// Locality-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalityCaps {
    /// The peer is on the same machine as the daemon. Guards same-machine
    /// assumptions: `$EDITOR`-compose, local attachment paths, the build-id
    /// handshake. `false` means the daemon must not assume shared filesystem.
    pub same_machine: bool,
}

/// Auth-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AuthCaps {
    /// The transport surfaces an OS-level peer identity on accept (UDS peer
    /// creds). `false` means the daemon has no implicit identity to trust.
    pub implicit_peer_identity: bool,
    /// The transport requires a bearer token to authenticate a peer (phase 5).
    pub token: bool,
}

/// Lifecycle-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LifecycleCaps {
    /// A client can start the daemon over this transport (autostart re-execs
    /// `current_exe()`; only meaningful when the client can spawn the daemon
    /// locally).
    pub client_autostart: bool,
}
