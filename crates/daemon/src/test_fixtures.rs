#![cfg(test)]

use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use crate::mxr_core::Account;

// -- Account ------------------------------------------------------------------

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

pub(crate) fn test_account_with_id(id: AccountId) -> Account {
    Account {
        id,
        name: "Fake Account".to_string(),
        email: "user@example.com".to_string(),
        sync_backend: Some(BackendRef {
            provider_kind: ProviderKind::Fake,
            config_key: "fake".to_string(),
        }),
        send_backend: None,
        enabled: true,
    }
}

// -- Envelope (builder) -------------------------------------------------------

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

    pub(crate) fn subject(mut self, s: impl Into<String>) -> Self {
        self.subject = s.into();
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
            name: name.map(|n| n.to_string()),
            email: email.to_string(),
        }];
        self
    }

    pub(crate) fn to(mut self, to: Vec<Address>) -> Self {
        self.to = to;
        self
    }

    pub(crate) fn flags(mut self, f: MessageFlags) -> Self {
        self.flags = f;
        self
    }

    pub(crate) fn account_id(mut self, id: AccountId) -> Self {
        self.account_id = id;
        self
    }

    pub(crate) fn thread_id(mut self, id: ThreadId) -> Self {
        self.thread_id = id;
        self
    }

    pub(crate) fn provider_id(mut self, id: impl Into<String>) -> Self {
        self.provider_id = id.into();
        self
    }

    pub(crate) fn label_provider_ids(mut self, ids: Vec<String>) -> Self {
        self.label_provider_ids = ids;
        self
    }

    pub(crate) fn date(mut self, d: chrono::DateTime<chrono::Utc>) -> Self {
        self.date = d;
        self
    }

    pub(crate) fn has_attachments(mut self, v: bool) -> Self {
        self.has_attachments = v;
        self
    }

    pub(crate) fn snippet(mut self, s: impl Into<String>) -> Self {
        self.snippet = s.into();
        self
    }

    pub(crate) fn size_bytes(mut self, n: u64) -> Self {
        self.size_bytes = n;
        self
    }

    pub(crate) fn message_id_header(mut self, h: Option<String>) -> Self {
        self.message_id_header = h;
        self
    }

    pub(crate) fn in_reply_to(mut self, r: Option<String>) -> Self {
        self.in_reply_to = r;
        self
    }

    pub(crate) fn references(mut self, r: Vec<String>) -> Self {
        self.references = r;
        self
    }

    pub(crate) fn unsubscribe(mut self, u: UnsubscribeMethod) -> Self {
        self.unsubscribe = u;
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

/// Shorthand: returns a default test envelope (same defaults as TestEnvelopeBuilder::new).
pub(crate) fn test_envelope() -> Envelope {
    TestEnvelopeBuilder::new().build()
}

// -- Label --------------------------------------------------------------------

pub(crate) fn test_label(account_id: &AccountId, name: &str, provider_id: &str) -> Label {
    Label {
        id: LabelId::new(),
        account_id: account_id.clone(),
        name: name.to_string(),
        kind: LabelKind::System,
        color: None,
        provider_id: provider_id.to_string(),
        unread_count: 0,
        total_count: 0,
    }
}

/// Standard set of system labels used by sidebar/label tests.
pub(crate) fn test_system_labels(account_id: &AccountId) -> Vec<Label> {
    let sys = |name: &str, pid: &str, unread: u32, total: u32| Label {
        id: LabelId::from_provider_id("test", pid),
        account_id: account_id.clone(),
        name: name.to_string(),
        kind: LabelKind::System,
        color: None,
        provider_id: pid.to_string(),
        unread_count: unread,
        total_count: total,
    };
    vec![
        sys("INBOX", "INBOX", 3, 10),
        sys("STARRED", "STARRED", 0, 2),
        sys("SENT", "SENT", 0, 5),
        sys("DRAFT", "DRAFT", 0, 0),
        sys("ARCHIVE", "ARCHIVE", 0, 0),
        sys("SPAM", "SPAM", 0, 0),
        sys("TRASH", "TRASH", 0, 0),
        sys("CHAT", "CHAT", 0, 0),
        sys("IMPORTANT", "IMPORTANT", 0, 5),
        Label {
            id: LabelId::from_provider_id("test", "Work"),
            account_id: account_id.clone(),
            name: "Work".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "Label_1".to_string(),
            unread_count: 2,
            total_count: 10,
        },
        Label {
            id: LabelId::from_provider_id("test", "Personal"),
            account_id: account_id.clone(),
            name: "Personal".to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: "Label_2".to_string(),
            unread_count: 0,
            total_count: 3,
        },
        Label {
            id: LabelId::from_provider_id("test", "CATEGORY_UPDATES"),
            account_id: account_id.clone(),
            name: "CATEGORY_UPDATES".to_string(),
            kind: LabelKind::System,
            color: None,
            provider_id: "CATEGORY_UPDATES".to_string(),
            unread_count: 0,
            total_count: 50,
        },
    ]
}

// -- MessageBody --------------------------------------------------------------

pub(crate) fn make_empty_body(message_id: &MessageId) -> MessageBody {
    MessageBody {
        message_id: message_id.clone(),
        text_plain: Some("test body".to_string()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: MessageMetadata::default(),
    }
}
