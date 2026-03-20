//! Example mxr adapter skeleton.
//!
//! Copy this crate into a standalone repository, replace the stubbed provider
//! methods with your real implementation, then run the conformance helpers from
//! `mxr-provider-fake`.

use async_trait::async_trait;
use mxr_core::{
    AccountId, Address, Draft, Label, MailSendProvider, MailSyncProvider, MxrError, SendReceipt,
    SyncBatch, SyncCapabilities, SyncCursor,
};

pub struct ExampleSyncProvider {
    account_id: AccountId,
}

impl ExampleSyncProvider {
    pub fn new(account_id: AccountId) -> Self {
        Self { account_id }
    }
}

#[async_trait]
impl MailSyncProvider for ExampleSyncProvider {
    fn name(&self) -> &str {
        "example"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: false,
            server_search: false,
            delta_sync: false,
            push: false,
            batch_operations: false,
            native_thread_ids: false,
        }
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Err(MxrError::Provider("authenticate not implemented".to_string()))
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Err(MxrError::Provider("refresh_auth not implemented".to_string()))
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Err(MxrError::Provider("sync_labels not implemented".to_string()))
    }

    async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        Err(MxrError::Provider("sync_messages not implemented".to_string()))
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, MxrError> {
        Err(MxrError::Provider("fetch_attachment not implemented".to_string()))
    }

    async fn modify_labels(
        &self,
        _provider_message_id: &str,
        _add: &[String],
        _remove: &[String],
    ) -> Result<(), MxrError> {
        Err(MxrError::Provider("modify_labels not implemented".to_string()))
    }

    async fn trash(&self, _provider_message_id: &str) -> Result<(), MxrError> {
        Err(MxrError::Provider("trash not implemented".to_string()))
    }

    async fn set_read(&self, _provider_message_id: &str, _read: bool) -> Result<(), MxrError> {
        Err(MxrError::Provider("set_read not implemented".to_string()))
    }

    async fn set_starred(
        &self,
        _provider_message_id: &str,
        _starred: bool,
    ) -> Result<(), MxrError> {
        Err(MxrError::Provider("set_starred not implemented".to_string()))
    }
}

pub struct ExampleSendProvider;

#[async_trait]
impl MailSendProvider for ExampleSendProvider {
    fn name(&self) -> &str {
        "example"
    }

    async fn send(&self, _draft: &Draft, _from: &Address) -> Result<SendReceipt, MxrError> {
        Err(MxrError::Provider("send not implemented".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_constructs_provider() {
        let provider = ExampleSyncProvider::new(AccountId::new());
        assert_eq!(provider.name(), "example");
    }
}
