#![cfg(test)]

use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::Account;

pub(crate) fn test_account() -> Account {
    Account {
        id: AccountId::new(),
        name: "Test".to_string(),
        email: "test@example.com".to_string(),
        sync_backend: Some(BackendRef {
            provider_kind: ProviderKind::Fake,
            config_key: "fake".to_string(),
        }),
        send_backend: None,
        enabled: true,
    }
}

pub(crate) struct TestEnvelopeBuilder {
    id: MessageId,
    account_id: AccountId,
    provider_id: String,
    thread_id: ThreadId,
    message_id_header: Option<String>,
    in_reply_to: Option<String>,
    references: Vec<String>,
    from: Address,
    to: Vec<Address>,
    cc: Vec<Address>,
    bcc: Vec<Address>,
    subject: String,
    date: chrono::DateTime<chrono::Utc>,
    flags: MessageFlags,
    snippet: String,
    has_attachments: bool,
    size_bytes: u64,
    unsubscribe: UnsubscribeMethod,
    label_provider_ids: Vec<String>,
}

impl TestEnvelopeBuilder {
    pub(crate) fn new() -> Self {
        Self {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "fake-1".to_string(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<test@example.com>".to_string()),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".to_string()),
                email: "alice@example.com".to_string(),
            },
            to: vec![Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test subject".to_string(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "Preview text".to_string(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        }
    }

    pub(crate) fn account_id(mut self, id: AccountId) -> Self {
        self.account_id = id;
        self
    }

    pub(crate) fn flags(mut self, flags: MessageFlags) -> Self {
        self.flags = flags;
        self
    }

    pub(crate) fn build(self) -> Envelope {
        Envelope {
            id: self.id,
            account_id: self.account_id,
            provider_id: self.provider_id,
            thread_id: self.thread_id,
            message_id_header: self.message_id_header,
            in_reply_to: self.in_reply_to,
            references: self.references,
            from: self.from,
            to: self.to,
            cc: self.cc,
            bcc: self.bcc,
            subject: self.subject,
            date: self.date,
            flags: self.flags,
            snippet: self.snippet,
            has_attachments: self.has_attachments,
            size_bytes: self.size_bytes,
            unsubscribe: self.unsubscribe,
            label_provider_ids: self.label_provider_ids,
        }
    }
}
