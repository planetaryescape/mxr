#![cfg(test)]
#![allow(clippy::wrong_self_convention)]

use mxr_core::id::*;
use mxr_core::types::*;

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

    pub(crate) fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    pub(crate) fn from_address(mut self, name: &str, email: &str) -> Self {
        self.from = Address {
            name: Some(name.to_string()),
            email: email.to_string(),
        };
        self
    }

    pub(crate) fn to_address(mut self, name: Option<&str>, email: &str) -> Self {
        self.to = vec![Address {
            name: name.map(|value| value.to_string()),
            email: email.to_string(),
        }];
        self
    }

    pub(crate) fn provider_id(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = provider_id.into();
        self
    }

    pub(crate) fn message_id_header(mut self, message_id_header: Option<String>) -> Self {
        self.message_id_header = message_id_header;
        self
    }

    pub(crate) fn flags(mut self, flags: MessageFlags) -> Self {
        self.flags = flags;
        self
    }

    pub(crate) fn has_attachments(mut self, has_attachments: bool) -> Self {
        self.has_attachments = has_attachments;
        self
    }

    pub(crate) fn snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = snippet.into();
        self
    }

    pub(crate) fn size_bytes(mut self, size_bytes: u64) -> Self {
        self.size_bytes = size_bytes;
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
