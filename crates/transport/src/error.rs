//! Transport error vocabulary.

/// One error class per way a transport operation can fail. Each byte-stream
/// failure carries the underlying [`std::io::Error`] so callers can inspect its
/// [`std::io::ErrorKind`] — the client's autostart/stale-socket logic keys off
/// the connect error kind.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Binding the listener failed (socket already bound, permissions, …).
    #[error("transport bind failed at {endpoint}: {source}")]
    Bind {
        /// The endpoint that could not be bound.
        endpoint: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Accepting a connection failed.
    #[error("transport accept failed: {0}")]
    Accept(#[source] std::io::Error),

    /// Dialing the daemon failed. The [`std::io::ErrorKind`] of `source` drives
    /// autostart decisions in the client's policy code.
    #[error("cannot connect to daemon at {endpoint}: {source}")]
    Connect {
        /// The endpoint that could not be reached.
        endpoint: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// Releasing a transport resource failed.
    #[error("transport cleanup failed: {0}")]
    Cleanup(#[source] std::io::Error),

    /// A transport address string could not be parsed.
    #[error("invalid transport address: {0}")]
    Addr(String),
}

/// Convenience alias for transport results.
pub type Result<T> = std::result::Result<T, TransportError>;
