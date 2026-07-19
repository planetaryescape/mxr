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

/// Queue state guarded by one lock so `closed` and `pending` mutate atomically.
struct Queue {
    pending: VecDeque<DuplexStream>,
    closed: bool,
}

/// Shared rendezvous between a memory transport's connectors and its listener.
struct Rendezvous {
    queue: Mutex<Queue>,
    ready: Notify,
}

impl Rendezvous {
    fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                pending: VecDeque::new(),
                closed: false,
            }),
            ready: Notify::new(),
        }
    }

    /// Enqueue a fresh connection's server end, but only if the listener is
    /// still accepting. The closed-check and the push happen under the SAME
    /// lock `stop_accepting` takes, so a connect can never slip an endpoint in
    /// after closure (which `accept` would then refuse, hanging the client).
    /// Returns `false` (rejected) if already closed.
    fn enqueue(&self, server_end: DuplexStream) -> bool {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.closed {
            return false;
        }
        queue.pending.push_back(server_end);
        drop(queue);
        self.ready.notify_one();
        true
    }

    /// Pop the next pending server end, or report that the listener has closed.
    /// `Ok(None)` means "empty, keep waiting".
    fn take(&self) -> std::result::Result<Option<DuplexStream>, ()> {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.closed {
            return Err(());
        }
        Ok(queue.pending.pop_front())
    }

    /// Stop accepting: mark closed AND drop every pending server end, so any
    /// connector that raced in just before closure observes a closed channel
    /// (EOF/broken pipe) rather than a client that hangs forever unaccepted.
    /// Then wake any parked `accept` so it observes the closure and returns.
    fn close(&self) {
        {
            let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
            queue.closed = true;
            queue.pending.clear();
        }
        self.ready.notify_waiters();
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
        }))
    }
}

/// The bound in-memory listener. `stop_accepting`/`accept` and the connectors
/// share one `closed` flag through the [`Rendezvous`].
struct MemoryListener {
    inner: Arc<Rendezvous>,
}

#[async_trait]
impl TransportListener for MemoryListener {
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)> {
        loop {
            match self.inner.take() {
                Err(()) => {
                    return Err(TransportError::Accept(std::io::Error::new(
                        std::io::ErrorKind::NotConnected,
                        "listener has stopped accepting",
                    )));
                }
                // In-memory: the peer is this process — an explicit
                // `LocalProcess`, never a fabricated `UnixPeer`.
                Ok(Some(server_end)) => return Ok((Box::new(server_end), PeerInfo::local())),
                Ok(None) => self.inner.ready.notified().await,
            }
        }
    }

    async fn stop_accepting(&mut self) {
        self.inner.close();
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
        // Once the listener has stopped accepting, a connect must fail (like a
        // UDS connection-refused) rather than hand back a client whose server
        // end will never be accepted. The closed-check and enqueue are atomic
        // inside `enqueue`, so this can't race a concurrent `stop_accepting`.
        let (server_end, client_end) = tokio::io::duplex(MEMORY_BUFFER);
        if !self.inner.enqueue(server_end) {
            return Err(TransportError::Connect {
                endpoint: "memory:".to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "memory listener has stopped accepting",
                ),
            });
        }
        Ok(Box::new(client_end))
    }

    fn describe(&self) -> String {
        "memory:".to_string()
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests assert directly on fixtures"
    )]

    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    /// Mirror of the UDS `stop_accepting` contract test: after `stop_accepting`,
    /// a connect fails (connection-refused-equivalent) rather than handing back
    /// a client that would hang unaccepted, and `accept` returns an error.
    #[tokio::test]
    async fn stop_accepting_refuses_new_connections() {
        let transport = MemoryTransport::new();
        let connector = transport.connector();
        let mut listener = transport.bind().await.unwrap();

        // Reachable while accepting: connect then accept the pair.
        assert!(
            connector.connect().await.is_ok(),
            "reachable while accepting"
        );
        listener
            .accept()
            .await
            .expect("accept the queued connection");

        // Stop accepting: further connects are refused, and accept errors.
        listener.stop_accepting().await;
        match connector.connect().await {
            Err(TransportError::Connect { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::ConnectionRefused);
            }
            Ok(_) => panic!("connect must be refused once accepting stopped"),
            Err(other) => panic!("expected a Connect error, got {other:?}"),
        }
        assert!(
            listener.accept().await.is_err(),
            "accept must fail after stop_accepting"
        );
    }

    /// The race the atomic `enqueue`/`close` closes: a connector that enqueued
    /// its server end BEFORE `stop_accepting` (but was never accepted) must not
    /// hang. `stop_accepting` drops the pending server end, so the raced
    /// client's channel reads EOF. Driven deterministically (no sleeps): the
    /// EOF is observable the instant the server end is dropped.
    #[tokio::test]
    async fn raced_pending_connection_sees_closed_channel_not_a_hang() {
        let transport = MemoryTransport::new();
        let connector = transport.connector();
        let mut listener = transport.bind().await.unwrap();

        // Connect (server end enqueued) but do NOT accept it.
        let mut raced = connector.connect().await.expect("connect before stop");

        // stop_accepting drains pending: the enqueued server end is dropped, so
        // the raced client end observes a closed channel.
        listener.stop_accepting().await;

        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(5), raced.read(&mut buf))
            .await
            .expect("read must not hang")
            .expect("read completes");
        assert_eq!(
            read, 0,
            "the raced connector's channel must be closed (EOF), never left hanging"
        );

        // And accept now reports closed rather than blocking forever.
        assert!(listener.accept().await.is_err(), "accept fails after close");
    }
}
