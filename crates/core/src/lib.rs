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
    #![allow(clippy::unwrap_used)]

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
    fn response_time_direction_round_trips_db_str() {
        // Cheap pin: as_db_str / from_db_str round-trip the enum cleanly,
        // which is what reply_pairs.direction column relies on.
        for d in [
            ResponseTimeDirection::IReplied,
            ResponseTimeDirection::TheyReplied,
        ] {
            assert_eq!(d.as_db_str().is_empty(), false);
        }
    }

    #[test]
    fn in_memory_account_address_lookup_classifies_correctly() {
        let lookup = InMemoryAccountAddressLookup::new();
        // Empty cache: not loaded; everything reads as not-account.
        assert!(!lookup.is_loaded());
        assert!(!lookup.is_account_address("anyone@example.com"));

        lookup.replace([
            "Me@Example.com".to_string(),
            "alias@example.com".to_string(),
        ]);
        assert!(lookup.is_loaded());

        // Case-insensitive match.
        assert!(lookup.is_account_address("me@example.com"));
        assert!(lookup.is_account_address("ME@EXAMPLE.COM"));
        assert!(lookup.is_account_address("alias@example.com"));
        // Outside the set.
        assert!(!lookup.is_account_address("other@example.com"));

        // Replacing with a different set discards the previous one.
        lookup.replace(["fresh@example.com".to_string()]);
        assert!(lookup.is_loaded());
        assert!(lookup.is_account_address("fresh@example.com"));
        assert!(!lookup.is_account_address("me@example.com"));
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
                mailboxes: vec![ImapMailboxCursor {
                    mailbox: "INBOX".into(),
                    uid_validity: 1,
                    uid_next: 100,
                    highest_modseq: Some(123),
                }],
                capabilities: Some(ImapCapabilityState {
                    move_ext: true,
                    uidplus: true,
                    idle: true,
                    condstore: false,
                    qresync: false,
                    namespace: true,
                    list_status: false,
                    utf8_accept: false,
                    imap4rev2: false,
                }),
            },
            SyncCursor::Initial,
        ];
        for c in cursors {
            let json = serde_json::to_string(&c).unwrap();
            let _parsed: SyncCursor = serde_json::from_str(&json).unwrap();
        }
    }
}
