use crate::error::MxrError;
use crate::id::AccountId;
use crate::types::*;
use async_trait::async_trait;

pub type Result<T> = std::result::Result<T, MxrError>;

#[async_trait]
pub trait MailSyncProvider: Send + Sync {
    fn name(&self) -> &str;
    fn account_id(&self) -> &AccountId;
    fn capabilities(&self) -> SyncCapabilities;

    /// Render an opaque cursor as a one-line human-readable string for
    /// daemon logs, `mxr doctor` output, and the status surface clients
    /// see. Adapters that own a structured private cursor (Gmail
    /// history_id, IMAP UIDVALIDITY+mailboxes, …) should decode and
    /// summarise it; the default impl reports the byte length.
    ///
    /// Contract: one short line, ideally < 80 chars.
    fn describe_cursor(&self, cursor: &SyncCursor) -> String {
        if cursor.is_empty() {
            "initial".to_string()
        } else {
            format!("opaque len={}", cursor.as_bytes().len())
        }
    }

    /// True iff this cursor represents the middle of a multi-page
    /// initial backfill — the daemon uses this to defer relationship
    /// indexing, skip label re-sync, and shorten its requeue interval
    /// while a provider walks the archive. Default `false` is correct
    /// for any adapter that completes initial sync in one shot.
    fn is_backfill_cursor(&self, _cursor: &SyncCursor) -> bool {
        false
    }

    async fn authenticate(&mut self) -> Result<()>;
    async fn refresh_auth(&mut self) -> Result<()>;

    async fn sync_labels(&self) -> Result<Vec<Label>>;
    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch>;

    async fn fetch_message(&self, _provider_message_id: &str) -> Result<Option<SyncedMessage>> {
        Ok(None)
    }

    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<Vec<u8>>;

    /// Apply a mutation against the provider.
    ///
    /// Callers supply a stable `mutation_id` (UUIDv7) so the daemon's
    /// `mutation_dedup_log` (24h window) can skip a duplicate apply
    /// on retry. Adapters are free to ignore `mutation_id` — the
    /// authoritative dedup happens in the daemon's store.
    ///
    /// For providers with `capabilities().mutate.labels == true`,
    /// `Mutation::ModifyLabels` is stable multi-assign label semantics.
    /// For folder-based providers (`mutate.labels == false`), the same
    /// request may map to move or copy semantics instead.
    async fn apply_mutation(&self, mutation_id: &str, mutation: &Mutation) -> Result<()>;

    async fn create_label(&self, _name: &str, _color: Option<&str>) -> Result<Label> {
        Err(MxrError::Provider(
            "Label creation not supported".to_string(),
        ))
    }

    async fn rename_label(&self, _provider_label_id: &str, _new_name: &str) -> Result<Label> {
        Err(MxrError::Provider("Label rename not supported".to_string()))
    }

    async fn delete_label(&self, _provider_label_id: &str) -> Result<()> {
        Err(MxrError::Provider(
            "Label deletion not supported".to_string(),
        ))
    }

    async fn search_remote(&self, _query: &str) -> Result<Vec<String>> {
        Err(MxrError::Provider(
            "Server-side search not supported".into(),
        ))
    }

    /// Phase 3.1: open an IDLE-style watcher that emits a notification
    /// whenever the server pushes an EXISTS / EXPUNGE / equivalent
    /// "something changed" event on the watched folder (default: INBOX).
    ///
    /// Default impl returns `Ok(None)`, signalling the daemon should
    /// fall back to its periodic poll loop. Providers that want push
    /// freshness override this. The watcher owns its own connection;
    /// the regular sync session is not interrupted.
    async fn idle_watch(&self) -> Result<Option<Box<dyn IdleWatcher>>> {
        Ok(None)
    }
}

/// Phase 3.1: a long-lived watcher returned by
/// `MailSyncProvider::idle_watch`. The daemon polls `next_event`
/// in a loop; each successful return triggers a delta sync for the
/// owning account. Errors signal a dropped connection — the daemon
/// reconnects with backoff.
#[async_trait]
pub trait IdleWatcher: Send + Sync {
    /// Wait for the next server-side change notification on the
    /// watched folder. Implementations should re-issue the protocol
    /// IDLE command before any vendor-specific timeout (~28 minutes
    /// for IMAP per RFC 2177) and return `Ok(())` on the next real
    /// event.
    async fn next_event(&mut self) -> Result<()>;
}

#[async_trait]
pub trait MailSendProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt>;

    async fn send_calendar_reply(
        &self,
        _reply: &CalendarReplyMessage,
        _from: &Address,
        _rfc2822_message_id: &str,
    ) -> Result<SendReceipt> {
        Err(MxrError::Provider(
            "Calendar RSVP sending not supported by this provider".to_string(),
        ))
    }

    /// Save a draft to the mail server. Returns the server-side draft ID if supported.
    /// Default: returns Ok(None) (provider doesn't support server drafts).
    async fn save_draft(&self, _draft: &Draft, _from: &Address) -> Result<Option<String>> {
        Ok(None)
    }
}
