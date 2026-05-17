use thiserror::Error;

#[derive(Debug, Error)]
pub enum MxrError {
    #[error("Store error: {0}")]
    Store(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Search error: {0}")]
    Search(String),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Not found: {0}")]
    NotFound(String),

    /// Adapter's sync cursor is no longer usable (Gmail historyId past
    /// the server's retention window, IMAP UIDVALIDITY changed, JMAP
    /// `cannotCalculateChanges`). The daemon catches this, clears the
    /// stored cursor, and retries with a full sync — the same recovery
    /// path MSP §5 prescribes via `msp.sync.cannot_calculate_changes`.
    /// Adapters MUST return this instead of `NotFound` so the daemon's
    /// recovery code stays provider-agnostic.
    #[error("Sync cursor expired: {reason}")]
    SyncCursorExpired { reason: String },

    /// Provider asked us to back off. `retry_after_secs` is the wait the
    /// provider suggested (Retry-After header for Gmail, server hint for IMAP).
    /// Surfaced as a typed variant so the daemon's sync loop can size its
    /// backoff without parsing strings.
    #[error("Rate limited — retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
