#![cfg(test)]

use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use crate::mxr_core::{MailSyncProvider, MxrError, SyncCapabilities};

/// A provider that always returns errors from sync_messages, for testing error handling.
pub(crate) struct ErrorProvider {
    pub account_id: AccountId,
}

#[async_trait::async_trait]
impl MailSyncProvider for ErrorProvider {
    fn name(&self) -> &str {
        "error"
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
            native_thread_ids: true,
        }
    }
    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }
    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }
    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(vec![])
    }
    async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        Err(MxrError::Provider("simulated sync error".into()))
    }
    async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
        Err(MxrError::Provider("simulated attachment error".into()))
    }
    async fn modify_labels(
        &self,
        _id: &str,
        _add: &[String],
        _rm: &[String],
    ) -> Result<(), MxrError> {
        Err(MxrError::Provider("simulated error".into()))
    }
    async fn trash(&self, _id: &str) -> Result<(), MxrError> {
        Err(MxrError::Provider("simulated error".into()))
    }
    async fn set_read(&self, _id: &str, _read: bool) -> Result<(), MxrError> {
        Err(MxrError::Provider("simulated error".into()))
    }
    async fn set_starred(&self, _id: &str, _starred: bool) -> Result<(), MxrError> {
        Err(MxrError::Provider("simulated error".into()))
    }
}

/// Provider that returns label_changes on delta sync for testing the label change code path.
pub(crate) struct DeltaLabelProvider {
    pub account_id: AccountId,
    pub messages: Vec<SyncedMessage>,
    pub labels: Vec<Label>,
    pub label_changes: Vec<LabelChange>,
}

impl DeltaLabelProvider {
    pub fn new(
        account_id: AccountId,
        messages: Vec<Envelope>,
        labels: Vec<Label>,
        label_changes: Vec<LabelChange>,
    ) -> Self {
        let messages = messages
            .into_iter()
            .map(|env| SyncedMessage {
                body: make_empty_body(&env.id),
                envelope: env,
            })
            .collect();
        Self {
            account_id,
            messages,
            labels,
            label_changes,
        }
    }
}

#[async_trait::async_trait]
impl MailSyncProvider for DeltaLabelProvider {
    fn name(&self) -> &str {
        "delta-label"
    }
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }
    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: true,
            server_search: false,
            delta_sync: true,
            push: false,
            batch_operations: false,
            native_thread_ids: true,
        }
    }
    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }
    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }
    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(self.labels.clone())
    }
    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        match cursor {
            SyncCursor::Initial => Ok(SyncBatch {
                upserted: self.messages.clone(),
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::Gmail { history_id: 100 },
            }),
            _ => Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: self.label_changes.clone(),
                next_cursor: SyncCursor::Gmail { history_id: 200 },
            }),
        }
    }
    async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
        Err(MxrError::NotFound("no attachment".into()))
    }
    async fn modify_labels(
        &self,
        _id: &str,
        _add: &[String],
        _rm: &[String],
    ) -> Result<(), MxrError> {
        Ok(())
    }
    async fn trash(&self, _id: &str) -> Result<(), MxrError> {
        Ok(())
    }
    async fn set_read(&self, _id: &str, _read: bool) -> Result<(), MxrError> {
        Ok(())
    }
    async fn set_starred(&self, _id: &str, _starred: bool) -> Result<(), MxrError> {
        Ok(())
    }
}

pub(crate) struct ThreadingProvider {
    pub account_id: AccountId,
    pub messages: Vec<SyncedMessage>,
}

#[async_trait::async_trait]
impl MailSyncProvider for ThreadingProvider {
    fn name(&self) -> &str {
        "threading"
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
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(vec![])
    }

    async fn sync_messages(&self, _cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        Ok(SyncBatch {
            upserted: self.messages.clone(),
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor: SyncCursor::Initial,
        })
    }

    async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
        Err(MxrError::NotFound("no attachment".into()))
    }

    async fn modify_labels(
        &self,
        _id: &str,
        _add: &[String],
        _rm: &[String],
    ) -> Result<(), MxrError> {
        Ok(())
    }

    async fn trash(&self, _id: &str) -> Result<(), MxrError> {
        Ok(())
    }

    async fn set_read(&self, _id: &str, _read: bool) -> Result<(), MxrError> {
        Ok(())
    }

    async fn set_starred(&self, _id: &str, _starred: bool) -> Result<(), MxrError> {
        Ok(())
    }
}

pub(crate) struct RecoveringNotFoundProvider {
    pub account_id: AccountId,
    pub message: SyncedMessage,
    pub calls: std::sync::Mutex<Vec<SyncCursor>>,
}

#[async_trait::async_trait]
impl MailSyncProvider for RecoveringNotFoundProvider {
    fn name(&self) -> &str {
        "recovering-not-found"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: false,
            server_search: false,
            delta_sync: true,
            push: false,
            batch_operations: false,
            native_thread_ids: true,
        }
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(vec![])
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        self.calls.lock().unwrap().push(cursor.clone());
        match cursor {
            SyncCursor::Gmail { .. } => {
                Err(MxrError::NotFound("Requested entity was not found.".into()))
            }
            SyncCursor::Initial => Ok(SyncBatch {
                upserted: vec![self.message.clone()],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::Gmail { history_id: 22 },
            }),
            other => panic!("unexpected cursor in test: {other:?}"),
        }
    }

    async fn fetch_attachment(&self, _mid: &str, _aid: &str) -> Result<Vec<u8>, MxrError> {
        Err(MxrError::NotFound("no attachment".into()))
    }

    async fn modify_labels(
        &self,
        _id: &str,
        _add: &[String],
        _rm: &[String],
    ) -> Result<(), MxrError> {
        Ok(())
    }

    async fn trash(&self, _id: &str) -> Result<(), MxrError> {
        Ok(())
    }

    async fn set_read(&self, _id: &str, _read: bool) -> Result<(), MxrError> {
        Ok(())
    }

    async fn set_starred(&self, _id: &str, _starred: bool) -> Result<(), MxrError> {
        Ok(())
    }
}

pub(crate) use crate::test_fixtures::{
    make_empty_body, test_account_with_id as test_account, test_label as make_test_label,
    TestEnvelopeBuilder,
};

pub(crate) fn make_test_envelope(
    account_id: &AccountId,
    provider_id: &str,
    label_provider_ids: Vec<String>,
) -> Envelope {
    TestEnvelopeBuilder::new()
        .account_id(account_id.clone())
        .provider_id(provider_id)
        .from_address("Test", "test@example.com")
        .to(vec![])
        .message_id_header(None)
        .subject("Test message")
        .snippet("Test snippet")
        .size_bytes(1000)
        .label_provider_ids(label_provider_ids)
        .build()
}
