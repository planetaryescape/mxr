use thiserror::Error;

#[derive(Debug, Error)]
pub enum MxrError {
    #[error("Store error: {0}")]
    Store(String),

    #[error("Provider error: {0}")]
    Provider(String),

    /// The provider rejected this account's credentials: a revoked or expired
    /// OAuth grant, or an IMAP/SMTP login refusal. Typed so the daemon answers
    /// with `IpcErrorKind::Auth` and the CLI exits 4. The text matches
    /// `Provider` so status and logs read the same.
    #[error("Provider error: {0}")]
    ProviderAuth(String),

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
    /// backoff without parsing strings. `detail` names who refused and why
    /// (e.g. `Gmail messages.get (rateLimitExceeded: ...)`), for status and logs.
    #[error("{}", rate_limit_message(*retry_after_secs, .detail.as_deref()))]
    RateLimited {
        retry_after_secs: u64,
        detail: Option<String>,
    },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

fn rate_limit_message(retry_after_secs: u64, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("Rate limited by {detail} — retry after {retry_after_secs}s"),
        None => format!("Rate limited — retry after {retry_after_secs}s"),
    }
}
