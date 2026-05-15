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

    /// Apply provider-native placement/label state.
    ///
    /// For providers with `capabilities().labels == true`, callers may treat this as
    /// stable multi-assign label semantics.
    ///
    /// For folder-based providers (`labels == false`), callers must not assume Gmail-style
    /// label behavior. The same request may map to move or copy semantics instead.
    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<()>;

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

    async fn trash(&self, provider_message_id: &str) -> Result<()>;
    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<()>;
    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<()>;

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
