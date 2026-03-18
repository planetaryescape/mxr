pub mod error;
pub mod id;
pub mod provider;
pub mod types;

pub use error::MxrError;
pub use id::*;
pub use provider::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_id_roundtrip() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: MessageId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn typed_id_display() {
        let id = AccountId::new();
        let display = format!("{}", id);
        assert!(!display.is_empty());
        assert_eq!(display, id.as_str());
    }

    #[test]
    fn message_flags_bitwise() {
        let flags = MessageFlags::READ | MessageFlags::STARRED;
        assert!(flags.contains(MessageFlags::READ));
        assert!(flags.contains(MessageFlags::STARRED));
        assert!(!flags.contains(MessageFlags::DRAFT));
        assert_eq!(flags.bits(), 0b0000_0011);
    }

    #[test]
    fn message_flags_serde() {
        let flags = MessageFlags::READ | MessageFlags::STARRED;
        let json = serde_json::to_string(&flags).unwrap();
        let parsed: MessageFlags = serde_json::from_str(&json).unwrap();
        assert_eq!(flags, parsed);
    }

    #[test]
    fn envelope_serde_roundtrip() {
        let env = Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "test-1".to_string(),
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
            flags: MessageFlags::READ | MessageFlags::STARRED,
            snippet: "Preview text".to_string(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![],
        };
        let json = serde_json::to_string(&env).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env.id, parsed.id);
        assert_eq!(env.subject, parsed.subject);
        assert_eq!(env.flags, parsed.flags);
        assert_eq!(env.from, parsed.from);
    }

    #[test]
    fn unsubscribe_method_variants() {
        let variants = vec![
            UnsubscribeMethod::OneClick {
                url: "https://unsub.example.com".to_string(),
            },
            UnsubscribeMethod::HttpLink {
                url: "https://unsub.example.com/link".to_string(),
            },
            UnsubscribeMethod::Mailto {
                address: "unsub@example.com".to_string(),
                subject: Some("unsubscribe".to_string()),
            },
            UnsubscribeMethod::BodyLink {
                url: "https://body.example.com".to_string(),
            },
            UnsubscribeMethod::None,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: UnsubscribeMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn sync_cursor_variants() {
        let cursors = vec![
            SyncCursor::Gmail { history_id: 12345 },
            SyncCursor::Imap {
                uid_validity: 1,
                uid_next: 100,
            },
            SyncCursor::Initial,
        ];
        for c in cursors {
            let json = serde_json::to_string(&c).unwrap();
            let _parsed: SyncCursor = serde_json::from_str(&json).unwrap();
        }
    }
}
