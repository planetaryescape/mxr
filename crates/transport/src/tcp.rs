//! TCP-loopback + token transport (phase 5a, transport-adapter initiative).
//!
//! The first transport with **no implicit peer identity**: a TCP connection
//! carries no OS peer credentials, and on loopback any local process — plus
//! browsers via DNS-rebinding / `0.0.0.0` tricks — can reach the port. So it
//! forces the token half of the trait: [`TcpListener::accept`] surfaces
//! [`PeerAuth::TokenRequired`], and the serve core gates every request behind a
//! successful `Authenticate` (see `crates/daemon/src/serve.rs`).
//!
//! ## Bind policy — loopback only
//!
//! [`TcpServerTransport::bind`] refuses a non-loopback bind outright, mirroring
//! the web bridge's posture (`enforce_non_loopback_safety`,
//! `crates/daemon/src/bridge.rs`). Remote access is deliberately out of scope
//! (decision gate Q2: no in-daemon remote); off-machine reach goes through
//! `mxr daemon dial-stdio` over SSH instead. Even bound to loopback the token
//! is mandatory — loopback is not an authenticator.

use std::net::{IpAddr, SocketAddr};

use async_trait::async_trait;
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};

use crate::error::{Result, TransportError};
use crate::peer::{PeerAuth, PeerInfo};
use crate::{
    AuthCaps, BoxedIo, Connector, LifecycleCaps, LocalityCaps, ServerTransport,
    TransportCapabilities, TransportListener,
};

/// True iff `ip` is an IPv4/IPv6 loopback address. The bind guard and any
/// caller wanting to pre-validate an address share this one definition.
#[must_use]
pub fn is_loopback_ip(ip: IpAddr) -> bool {
    ip.is_loopback()
}

/// The TCP server transport. Binds loopback-only; peers must authenticate with
/// a token before any request is served.
#[derive(Debug, Clone)]
pub struct TcpServerTransport {
    addr: SocketAddr,
}

impl TcpServerTransport {
    /// A TCP transport that binds at `addr`. Non-loopback addresses are refused
    /// at [`ServerTransport::bind`] time.
    #[must_use]
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }
}

#[async_trait]
impl ServerTransport for TcpServerTransport {
    fn name(&self) -> &str {
        "tcp"
    }

    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            // Loopback bind: the peer is on this machine. No autostart — a TCP
            // client cannot re-exec the daemon binary the way a UDS client can.
            locality: LocalityCaps { same_machine: true },
            auth: AuthCaps {
                // No OS-level peer identity; the token IS the identity.
                implicit_peer_identity: false,
                token: true,
            },
            lifecycle: LifecycleCaps {
                client_autostart: false,
            },
        }
    }

    async fn bind(&self) -> Result<Box<dyn TransportListener>> {
        let endpoint = tcp_endpoint(self.addr);
        if !is_loopback_ip(self.addr.ip()) {
            // Refuse outright: remote TCP without TLS is a canonical RCE vector
            // (Docker's unauthenticated 2375). Off-machine access is the
            // `dial-stdio`-over-SSH story, not an in-daemon bind.
            return Err(TransportError::Bind {
                endpoint,
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "refusing non-loopback TCP bind: remote access without TLS is out of scope \
                     (use `mxr daemon dial-stdio` over SSH); bind 127.0.0.1 or ::1",
                ),
            });
        }
        let listener =
            TokioTcpListener::bind(self.addr)
                .await
                .map_err(|source| TransportError::Bind {
                    endpoint: endpoint.clone(),
                    source,
                })?;
        // The kernel may have assigned an ephemeral port (`:0`); report the real
        // one in the endpoint string.
        let bound = listener.local_addr().unwrap_or(self.addr);
        Ok(Box::new(TcpListenerAdapter {
            listener: Some(listener),
            addr: bound,
        }))
    }
}

/// A bound TCP listener. The socket is closed on [`Self::stop_accepting`]; there
/// is no filesystem resource to unlink, so [`Self::cleanup`] is a no-op.
struct TcpListenerAdapter {
    listener: Option<TokioTcpListener>,
    addr: SocketAddr,
}

#[async_trait]
impl TransportListener for TcpListenerAdapter {
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)> {
        let listener = self.listener.as_ref().ok_or_else(|| {
            TransportError::Accept(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "listener has stopped accepting",
            ))
        })?;
        let (stream, _peer) = listener.accept().await.map_err(TransportError::Accept)?;
        // Framed IPC is request/response with small control frames; Nagle only
        // adds latency. Best-effort — a set failure never fails the accept.
        let _ = stream.set_nodelay(true);
        // No OS peer identity on TCP: the dispatch gate must demand a token.
        let peer = PeerInfo {
            auth: PeerAuth::TokenRequired,
        };
        Ok((Box::new(stream), peer))
    }

    async fn stop_accepting(&mut self) {
        // Drop the listening socket so new connects are refused promptly.
        self.listener = None;
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Nothing on disk; dropping the socket is enough.
        Ok(())
    }

    fn endpoint(&self) -> String {
        tcp_endpoint(self.addr)
    }
}

/// The TCP client connector — dials a loopback daemon and carries the bearer
/// token the shared client authenticates with.
#[derive(Debug, Clone)]
pub struct TcpConnector {
    addr: SocketAddr,
    token: Option<String>,
}

impl TcpConnector {
    /// A connector that dials `addr` with `token` (when `Some`, the shared IPC
    /// client sends `Authenticate` automatically on connect).
    #[must_use]
    pub fn new(addr: SocketAddr, token: Option<String>) -> Self {
        Self { addr, token }
    }
}

#[async_trait]
impl Connector for TcpConnector {
    async fn connect(&self) -> Result<BoxedIo> {
        // Refuse non-loopback BEFORE opening the socket: TCP carries the token
        // in plaintext, so `MXR_DAEMON_ADDR=tcp://<remote-ip>` would leak the
        // credential to an off-machine host. The target is a numeric address
        // (parsed as a `SocketAddr`), so this is a direct check — no DNS. The
        // server also refuses non-loopback binds; this closes the client half.
        if !is_loopback_ip(self.addr.ip()) {
            return Err(TransportError::Connect {
                endpoint: tcp_endpoint(self.addr),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "refusing non-loopback tcp:// connect: the daemon token would be sent in \
                     plaintext to a remote host (use `cmd://ssh … mxr daemon dial-stdio` for \
                     off-machine access); target 127.0.0.1 or ::1",
                ),
            });
        }
        let stream =
            TcpStream::connect(self.addr)
                .await
                .map_err(|source| TransportError::Connect {
                    endpoint: tcp_endpoint(self.addr),
                    source,
                })?;
        let _ = stream.set_nodelay(true);
        Ok(Box::new(stream))
    }

    fn describe(&self) -> String {
        tcp_endpoint(self.addr)
    }

    fn auth_token(&self) -> Option<&str> {
        // Defense in depth: never advertise the token for a non-loopback
        // target. `connect` already refuses first, so the handshake is never
        // reached — this guarantees the token isn't surfaced even to a caller
        // that inspects the connector directly.
        if is_loopback_ip(self.addr.ip()) {
            self.token.as_deref()
        } else {
            None
        }
    }
}

fn tcp_endpoint(addr: SocketAddr) -> String {
    format!("tcp://{addr}")
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests assert directly on fixtures"
    )]

    use super::*;

    #[tokio::test]
    async fn refuses_non_loopback_bind() {
        let transport = TcpServerTransport::new("0.0.0.0:0".parse().unwrap());
        match transport.bind().await {
            Err(TransportError::Bind { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            }
            Ok(_) => panic!("a non-loopback bind must be refused"),
            Err(other) => panic!("expected a Bind error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn loopback_bind_accepts_and_surfaces_token_required() {
        // Ephemeral loopback port; a connect+accept yields a TokenRequired peer.
        let transport = TcpServerTransport::new("127.0.0.1:0".parse().unwrap());
        let mut listener = transport.bind().await.unwrap();
        let endpoint = listener.endpoint();
        let addr: SocketAddr = endpoint.strip_prefix("tcp://").unwrap().parse().unwrap();

        let connector = TcpConnector::new(addr, Some("secret".to_string()));
        assert_eq!(connector.auth_token(), Some("secret"));

        let accept = tokio::spawn(async move { listener.accept().await.map(|(_, peer)| peer) });
        let _client = connector.connect().await.unwrap();
        let peer = accept.await.unwrap().unwrap();
        assert_eq!(peer.auth, PeerAuth::TokenRequired);
    }

    #[test]
    fn connector_without_token_advertises_none() {
        let connector = TcpConnector::new("127.0.0.1:9000".parse().unwrap(), None);
        assert_eq!(connector.auth_token(), None);
    }

    #[tokio::test]
    async fn connector_refuses_non_loopback_before_sending_token() {
        // A non-loopback target must be refused at connect time, and the token
        // must never be advertised for it — no plaintext credential leak.
        let connector = TcpConnector::new("93.184.216.34:9000".parse().unwrap(), Some("s".into()));
        assert_eq!(
            connector.auth_token(),
            None,
            "token must not be advertised for a non-loopback target"
        );
        match connector.connect().await {
            Err(TransportError::Connect { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            }
            Ok(_) => panic!("a non-loopback connect must be refused before any token is sent"),
            Err(other) => panic!("expected a Connect error, got {other:?}"),
        }
    }
}
