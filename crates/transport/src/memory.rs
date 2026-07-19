//! In-memory duplex transport (behind the `test-util` feature).
//!
//! The "fake provider" analog: a real [`ServerTransport`] / [`Connector`] pair
//! backed by `tokio::io::duplex` — no socket file, no fd, no kernel round-trip.
//! The conformance corpus runs its scenarios over this through the same
//! `bind`/`accept`/`connect` path the UDS transport uses, so a scenario that
//! passes here and on UDS is proven carrier-independent.
//!
//! A [`MemoryConnector::connect`] builds a duplex pair, hands the server end to
//! the listener's pending queue, and returns the client end; the matching
//! [`MemoryListener::accept`] pops the server end.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use tokio::io::DuplexStream;
use tokio::sync::Notify;

use crate::error::{Result, TransportError};
use crate::peer::PeerInfo;
use crate::{
    AuthCaps, BoxedIo, Connector, LifecycleCaps, LocalityCaps, ServerTransport,
    TransportCapabilities, TransportListener,
};

/// Duplex buffer size — past the 16 MiB `IpcCodec` frame cap plus slack, so a
/// near-limit frame transfers without backpressure churn (mirrors the corpus's
/// duplex carrier).
const MEMORY_BUFFER: usize = 16 * 1024 * 1024 + 1024;

/// Shared rendezvous between a memory transport's connectors and its listener.
struct Rendezvous {
    pending: Mutex<VecDeque<DuplexStream>>,
    ready: Notify,
}

impl Rendezvous {
    fn new() -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            ready: Notify::new(),
        }
    }

    fn push(&self, server_end: DuplexStream) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(server_end);
        self.ready.notify_one();
    }

    fn try_pop(&self) -> Option<DuplexStream> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }
}

/// An in-memory duplex transport. Clone-cheap; the listener and every connector
/// share one rendezvous.
#[derive(Clone)]
pub struct MemoryTransport {
    inner: Arc<Rendezvous>,
}

impl Default for MemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryTransport {
    /// A fresh in-memory transport with an empty pending queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Rendezvous::new()),
        }
    }

    /// A connector that dials this transport's listener.
    #[must_use]
    pub fn connector(&self) -> MemoryConnector {
        MemoryConnector {
            inner: self.inner.clone(),
        }
    }
}

#[async_trait]
impl ServerTransport for MemoryTransport {
    fn name(&self) -> &str {
        "memory"
    }

    fn capabilities(&self) -> TransportCapabilities {
        // In-process: same machine, and the peer is literally this process, so
        // there is an implicit (trivial) identity. No autostart — you cannot
        // spawn a daemon over an in-memory pipe.
        TransportCapabilities {
            locality: LocalityCaps { same_machine: true },
            auth: AuthCaps {
                implicit_peer_identity: true,
                token: false,
            },
            lifecycle: LifecycleCaps {
                client_autostart: false,
            },
        }
    }

    async fn bind(&self) -> Result<Box<dyn TransportListener>> {
        Ok(Box::new(MemoryListener {
            inner: self.inner.clone(),
            accepting: true,
        }))
    }
}

/// The bound in-memory listener.
struct MemoryListener {
    inner: Arc<Rendezvous>,
    accepting: bool,
}

#[async_trait]
impl TransportListener for MemoryListener {
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)> {
        loop {
            if !self.accepting {
                return Err(TransportError::Accept(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "listener has stopped accepting",
                )));
            }
            if let Some(server_end) = self.inner.try_pop() {
                // In-memory: the peer is this process — an explicit
                // `LocalProcess`, never a fabricated `UnixPeer`.
                return Ok((Box::new(server_end), PeerInfo::local()));
            }
            self.inner.ready.notified().await;
        }
    }

    async fn stop_accepting(&mut self) {
        self.accepting = false;
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Nothing on disk to remove; dropping the rendezvous is enough.
        Ok(())
    }

    fn endpoint(&self) -> String {
        "memory:".to_string()
    }
}

/// A connector that dials an in-memory transport.
#[derive(Clone)]
pub struct MemoryConnector {
    inner: Arc<Rendezvous>,
}

#[async_trait]
impl Connector for MemoryConnector {
    async fn connect(&self) -> Result<BoxedIo> {
        let (server_end, client_end) = tokio::io::duplex(MEMORY_BUFFER);
        self.inner.push(server_end);
        Ok(Box::new(client_end))
    }

    fn describe(&self) -> String {
        "memory:".to_string()
    }
}
