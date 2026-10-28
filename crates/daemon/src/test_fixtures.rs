#![cfg(test)]
#![allow(dead_code)]

use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::Account;

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

#[allow(dead_code)]
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
    link_count: u32,
    body_word_count: u32,
    label_provider_ids: Vec<String>,
}

#[allow(dead_code)]
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
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
        }
    }

    pub(crate) fn subject(mut self, s: impl Into<String>) -> Self {
        self.subject = s.into();
        self
    }

    pub(crate) fn sender_address(mut self, name: &str, email: &str) -> Self {
        self.from = Address {
            name: Some(name.to_string()),
            email: email.to_string(),
        };
        self
    }

    pub(crate) fn recipient_address(mut self, name: Option<&str>, email: &str) -> Self {
        self.to = vec![Address {
            name: name.map(std::string::ToString::to_string),
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
            link_count: self.link_count,
            body_word_count: self.body_word_count,
            label_provider_ids: self.label_provider_ids,
            keywords: std::collections::BTreeSet::new(),
        }
    }
}

/// Shorthand: returns a default test envelope (same defaults as TestEnvelopeBuilder::new).
#[allow(dead_code)]
pub(crate) fn test_envelope() -> Envelope {
    TestEnvelopeBuilder::new().build()
}

// -- Label --------------------------------------------------------------------

#[allow(dead_code)]
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
        role: None,
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
        role: None,
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
            role: None,
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
            role: None,
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
            role: None,
        },
    ]
}

// -- MessageBody --------------------------------------------------------------

#[allow(dead_code)]
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

// -- Fake daemon for bulk selection -------------------------------------------

/// A daemon that answers only what a `--search` bulk command sends: the
/// search, the envelope lookup, the unsubscribe plan and the mutation. It holds
/// `matching` messages newest first and returns the first `limit` of them,
/// with `has_more` set when more remain, the way the real daemon pages a
/// search. Tests drive the production
/// selection and mutation code through a real `IpcClient` against it.
pub(crate) struct FakeBulkDaemon {
    _dir: tempfile::TempDir,
    socket: std::path::PathBuf,
    /// The messages the search matches, newest first.
    pub(crate) message_ids: Vec<MessageId>,
    /// Every request that arrived, in order.
    pub(crate) requests: std::sync::Arc<std::sync::Mutex<Vec<mxr_protocol::Request>>>,
    /// A label per request as it arrives, shared with `NoticeSink` so a test can
    /// see whether a notice was written before a mutation request arrived.
    pub(crate) events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl FakeBulkDaemon {
    pub(crate) fn start(matching: usize) -> Self {
        Self::start_with_total(matching, matching)
    }

    /// Like [`Self::start`], but the search reports `total` as its match count
    /// whatever `matching` is. The real total is an estimate; `has_more` is the
    /// exact signal and always follows `matching`.
    pub(crate) fn start_with_total(matching: usize, total: usize) -> Self {
        use futures::{SinkExt, StreamExt};
        use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload, Request, Response};

        let dir = tempfile::tempdir().expect("temporary socket directory");
        let socket = dir.path().join("d.sock");
        let listener = tokio::net::UnixListener::bind(&socket).expect("bind fake daemon");
        let message_ids: Vec<MessageId> = (0..matching).map(|_| MessageId::new()).collect();
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let served_ids = message_ids.clone();
        let served_requests = requests.clone();
        let served_events = events.clone();
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let mut framed = tokio_util::codec::Framed::new(stream, IpcCodec::new());
            while let Some(Ok(message)) = framed.next().await {
                let IpcPayload::Request(request) = message.payload else {
                    continue;
                };
                served_requests
                    .lock()
                    .expect("request log")
                    .push(request.clone());
                let (label, response) = match request {
                    Request::Search { limit, .. } => (
                        "search",
                        Response::Ok {
                            data: mxr_protocol::ResponseData::SearchResults {
                                results: served_ids
                                    .iter()
                                    .take(limit as usize)
                                    .map(|id| mxr_protocol::SearchResultItem {
                                        message_id: id.clone(),
                                        account_id: AccountId::new(),
                                        thread_id: ThreadId::new(),
                                        score: 1.0,
                                        mode: SearchMode::Lexical,
                                    })
                                    .collect(),
                                total: total as u32,
                                has_more: served_ids.len() > limit as usize,
                                next_offset: None,
                                explain: None,
                                limit: Some(limit),
                                degraded: None,
                            },
                        },
                    ),
                    Request::ListEnvelopesByIds { message_ids } => (
                        "envelopes",
                        Response::Ok {
                            data: mxr_protocol::ResponseData::Envelopes {
                                envelopes: message_ids
                                    .into_iter()
                                    .map(|id| {
                                        let mut envelope = TestEnvelopeBuilder::new().build();
                                        envelope.id = id;
                                        envelope
                                    })
                                    .collect(),
                            },
                        },
                    ),
                    Request::PlanUnsubscribe { message_ids } => (
                        "plan",
                        Response::Ok {
                            data: mxr_protocol::ResponseData::UnsubscribePlan {
                                plan: mxr_protocol::UnsubscribePlanData {
                                    matched_messages: message_ids.len() as u32,
                                    request: message_ids.first().map(|id| {
                                        mxr_protocol::UnsubscribePlanRequestData {
                                            message_id: id.clone(),
                                            method: UnsubscribeMethod::HttpLink {
                                                url: "https://example.com/unsubscribe".into(),
                                            },
                                        }
                                    }),
                                    already_unsubscribed: false,
                                },
                            },
                        },
                    ),
                    Request::Unsubscribe { .. } => (
                        "unsubscribe",
                        Response::Ok {
                            data: mxr_protocol::ResponseData::Ack,
                        },
                    ),
                    Request::Mutation { .. } => (
                        "mutation",
                        Response::Ok {
                            data: mxr_protocol::ResponseData::Ack,
                        },
                    ),
                    Request::StartMutationJob { .. } => (
                        "mutation",
                        Response::Ok {
                            data: mxr_protocol::ResponseData::JobStarted {
                                job: serde_json::from_value(serde_json::json!({
                                    "job_id": "job-1",
                                    "kind": "mutation",
                                    "status": "running",
                                    "progress": {
                                        "total": 0, "completed": 0, "succeeded": 0,
                                        "skipped": 0, "failed": 0
                                    },
                                    "started_at": 0
                                }))
                                .expect("job fixture"),
                            },
                        },
                    ),
                    other => unreachable!("the fake bulk daemon was sent {other:?}"),
                };
                served_events
                    .lock()
                    .expect("event log")
                    .push(label.to_owned());
                let reply = IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(response),
                };
                if framed.send(reply).await.is_err() {
                    return;
                }
            }
        });

        Self {
            _dir: dir,
            socket,
            message_ids,
            requests,
            events,
        }
    }

    pub(crate) async fn client(&self) -> crate::ipc_client::IpcClient {
        crate::ipc_client::IpcClient::connect_to(&self.socket)
            .await
            .expect("connect to fake bulk daemon")
    }

    pub(crate) fn search_requests(&self) -> Vec<mxr_protocol::Request> {
        self.requests
            .lock()
            .expect("request log")
            .iter()
            .filter(|request| matches!(request, mxr_protocol::Request::Search { .. }))
            .cloned()
            .collect()
    }

    /// A notice sink that records into the same log as the requests.
    pub(crate) fn notice_sink(&self) -> NoticeSink {
        NoticeSink {
            events: self.events.clone(),
            line: Vec::new(),
        }
    }
}

/// Stands in for stderr: each finished line lands in the daemon's event log as
/// `notice: <line>`.
pub(crate) struct NoticeSink {
    events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    line: Vec<u8>,
}

impl std::io::Write for NoticeSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for byte in buf {
            if *byte == b'\n' {
                let line = String::from_utf8_lossy(&self.line).into_owned();
                self.events
                    .lock()
                    .expect("event log")
                    .push(format!("notice: {line}"));
                self.line.clear();
            } else {
                self.line.push(*byte);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
