//! Example mxr transport-adapter skeleton.
//!
//! Copy this crate into a standalone repository, replace the stubbed methods
//! with a real implementation over your byte carrier (a named pipe, a vsock, a
//! WebSocket, …), then prove it conforms with the reusable checks from
//! `mxr-transport`'s `conformance` feature (see the commented test at the
//! bottom).
//!
//! Before writing an in-tree trait impl at all, check whether your transport can
//! instead be a byte pipe behind `mxr daemon dial-stdio` / a `cmd://` connector
//! — that is the *recommended* community path (SSH, containers, tunnels) and
//! needs zero new daemon trust surface. Implement `ServerTransport` /
//! `Connector` directly only for a transport that genuinely cannot be
//! byte-piped. See `docs/blueprint/20-transports.md`.
//!
//! Study `UdsServerTransport` (the production reference) and `MemoryTransport`
//! (the in-memory reference, behind `test-util`) in `crates/transport/` for two
//! worked implementations.

use std::io;

use async_trait::async_trait;
use mxr_transport::{
    BoxedIo, Connector, PeerInfo, Result, ServerTransport, TransportCapabilities, TransportError,
    TransportListener,
};

/// A stub server transport. Replace the bodies with a real listener over your
/// carrier.
pub struct ExampleServerTransport;

#[async_trait]
impl ServerTransport for ExampleServerTransport {
    fn name(&self) -> &str {
        "example"
    }

    /// Start with EVERY capability off and flip a flag on only once the
    /// corresponding property is genuinely provided — the daemon TRUSTS this and
    /// routes/authenticates based on it. `TransportCapabilities::default()` is
    /// all-false, the safe baseline. (For example: set `auth.token = true` only
    /// when your accept really surfaces `PeerAuth::TokenRequired` and the daemon
    /// must gate the connection behind `Authenticate`.)
    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities::default()
    }

    async fn bind(&self) -> Result<Box<dyn TransportListener>> {
        Err(TransportError::Bind {
            endpoint: "example:".to_string(),
            source: unimplemented("bind"),
        })
    }
}

/// A stub bound listener. `bind` would return this once implemented; its methods
/// form the accept/shutdown contract the conformance suite exercises.
pub struct ExampleListener;

#[async_trait]
impl TransportListener for ExampleListener {
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)> {
        // Return a connected byte stream plus REAL auth evidence. Never fabricate
        // a `PeerAuth::UnixPeer` — use `PeerInfo::local()` for an in-process peer,
        // or `PeerAuth::TokenRequired` for a transport that must authenticate.
        Err(TransportError::Accept(unimplemented("accept")))
    }

    async fn stop_accepting(&mut self) {
        // Close the listening endpoint so new connects are refused promptly
        // (they must NOT hang), WITHOUT releasing the transport resource — that
        // is `cleanup`'s job, deferred until in-flight connections drain.
        // Idempotent.
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Release transport-owned resources (a socket file, …). Runs LAST, after
        // the connection drain. Idempotent.
        Ok(())
    }

    fn endpoint(&self) -> String {
        "example:".to_string()
    }
}

/// A stub client connector. Replace the body with a real dial to your carrier.
pub struct ExampleConnector;

#[async_trait]
impl Connector for ExampleConnector {
    async fn connect(&self) -> Result<BoxedIo> {
        Err(TransportError::Connect {
            endpoint: "example:".to_string(),
            source: unimplemented("connect"),
        })
    }

    fn describe(&self) -> String {
        "example:".to_string()
    }

    // If your transport requires a bearer token (like TCP), override `auth_token`
    // to advertise it; the shared IPC client performs the `Authenticate`
    // handshake after the stream is up. The default is `None` (no handshake) —
    // correct for every implicit-trust transport.
}

/// A placeholder "not implemented" I/O error for the stubs above.
fn unimplemented(method: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{method} not implemented"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_constructs_transport() {
        let transport = ExampleServerTransport;
        assert_eq!(transport.name(), "example");
    }

    // Once your transport really binds/accepts/connects, uncomment this to prove
    // it upholds the transport contract. Wire the transport and a connector to
    // the SAME endpoint, then run the suite — the provider kit's
    // `run_sync_conformance` analog. For a token transport, also call
    // `mxr_transport::conformance::run_token_auth_conformance`.
    //
    // #[tokio::test]
    // async fn example_transport_conforms() {
    //     let transport = ExampleServerTransport;
    //     let connector = ExampleConnector;
    //     mxr_transport::conformance::run_transport_conformance(&transport, &connector).await;
    // }
}
