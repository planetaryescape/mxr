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

    async fn fetch_body(&self, provider_message_id: &str) -> Result<MessageBody>;
    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<Vec<u8>>;

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<()>;

    async fn trash(&self, provider_message_id: &str) -> Result<()>;
    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<()>;
    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<()>;

    async fn search_remote(&self, _query: &str) -> Result<Vec<String>> {
        Err(MxrError::Provider(
            "Server-side search not supported".into(),
        ))
    }
}

#[async_trait]
pub trait MailSendProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn send(&self, draft: &Draft, from: &Address) -> Result<SendReceipt>;
}
