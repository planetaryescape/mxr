//! Intermediate IMAP types used between raw IMAP responses and mxr core types.
/// Mailbox status returned by IMAP SELECT.
#[derive(Debug, Clone)]
pub struct MailboxInfo {
    pub uid_validity: u32,
    pub uid_next: u32,
    pub exists: u32,
}

/// A single fetched message from IMAP FETCH.
#[derive(Debug, Clone)]
pub struct FetchedMessage {
    pub uid: u32,
    pub flags: Vec<String>,
    pub envelope: Option<ImapEnvelope>,
    pub body: Option<Vec<u8>>,
    pub header: Option<Vec<u8>>,
    pub size: Option<u32>,
}

/// Parsed IMAP ENVELOPE structure.
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

/// An address from IMAP ENVELOPE.
#[derive(Debug, Clone)]
pub struct ImapAddress {
    pub name: Option<String>,
    pub email: String,
}

/// IMAP folder info from LIST response.
#[derive(Debug, Clone)]
pub struct FolderInfo {
    pub name: String,
    pub special_use: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_info_debug() {
        let info = MailboxInfo {
            uid_validity: 1,
            uid_next: 100,
            exists: 50,
        };
        assert_eq!(info.uid_validity, 1);
        assert_eq!(info.uid_next, 100);
        assert_eq!(info.exists, 50);
    }

    #[test]
    fn fetched_message_with_envelope() {
        let msg = FetchedMessage {
            uid: 42,
            flags: vec!["\\Seen".to_string(), "\\Flagged".to_string()],
            envelope: Some(ImapEnvelope {
                date: Some("Mon, 1 Jan 2024 12:00:00 +0000".to_string()),
                subject: Some("Test subject".to_string()),
                from: vec![ImapAddress {
                    name: Some("Alice".to_string()),
                    email: "alice@example.com".to_string(),
                }],
                to: vec![ImapAddress {
                    name: None,
                    email: "bob@example.com".to_string(),
                }],
                cc: vec![],
                bcc: vec![],
                message_id: Some("<msg1@example.com>".to_string()),
                in_reply_to: None,
            }),
            body: None,
            header: None,
            size: Some(1024),
        };
        assert_eq!(msg.uid, 42);
        assert_eq!(msg.flags.len(), 2);
        assert!(msg.envelope.is_some());
    }

    #[test]
    fn folder_info_with_special_use() {
        let folder = FolderInfo {
            name: "INBOX".to_string(),
            special_use: Some("\\Inbox".to_string()),
        };
        assert_eq!(folder.name, "INBOX");
        assert_eq!(folder.special_use, Some("\\Inbox".to_string()));
    }

    #[test]
    fn folder_info_without_special_use() {
        let folder = FolderInfo {
            name: "Projects/Work".to_string(),
            special_use: None,
        };
        assert!(folder.special_use.is_none());
    }
}
