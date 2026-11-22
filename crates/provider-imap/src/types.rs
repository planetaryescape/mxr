//! Intermediate IMAP types used between raw IMAP responses and mxr core types.
/// Mailbox status returned by IMAP SELECT.
#[derive(Debug, Clone, Default)]
pub struct MailboxInfo {
    pub uid_validity: u32,
    pub uid_next: u32,
    pub exists: u32,
    pub highest_modseq: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct ImapCapabilities {
    pub move_ext: bool,
    pub uidplus: bool,
    pub idle: bool,
    pub condstore: bool,
    pub qresync: bool,
    pub namespace: bool,
    pub list_status: bool,
    pub utf8_accept: bool,
    pub imap4rev2: bool,
    pub x_gm_ext_1: bool,
}

#[derive(Debug, Clone)]
pub struct FetchedMessage {
    pub uid: u32,
    pub flags: Vec<String>,
    pub envelope: Option<ImapEnvelope>,
    pub body: Option<Vec<u8>>,
    pub header: Option<Vec<u8>>,
    pub size: Option<u32>,
    /// IMAP INTERNALDATE (server-assigned, caller-selectable on APPEND). Used as the date
    /// fallback when a message has no parseable `Date:` header, so such
    /// messages get a stable time instead of the sync wall-clock.
    pub internal_date: Option<chrono::DateTime<chrono::Utc>>,
    pub gmail_labels: Vec<String>,
    pub gmail_msg_id: Option<u64>,
    pub gmail_thread_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ImapEnvelope {
    pub date: Option<String>,
    pub subject: Option<String>,
    pub from: Vec<ImapAddress>,
    pub to: Vec<ImapAddress>,
    pub cc: Vec<ImapAddress>,
    pub bcc: Vec<ImapAddress>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImapAddress {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Default)]
pub struct FolderInfo {
    pub name: String,
    pub special_use: Option<String>,
    pub delimiter: Option<String>,
    pub unread_count: Option<u32>,
    pub total_count: Option<u32>,
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
    pub highest_modseq: Option<u64>,
    pub namespace_prefix: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NamespaceInfo {
    pub personal_prefix: Option<String>,
    pub delimiter: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QresyncInfo {
    pub mailbox: MailboxInfo,
    pub vanished: Vec<u32>,
    pub changed: Vec<u32>,
}
