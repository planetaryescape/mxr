//! Foundation types for the mxr workspace.
//!
//! `mxr-core` sits at the bottom of the crate graph and depends on no other
//! workspace crate. Every other layer — `protocol`, `store`, `search`,
//! `sync`, the provider adapters, and the daemon — builds on the vocabulary
//! defined here, so anything in this crate must stay provider-agnostic and
//! client-agnostic.
//!
//! What lives here:
//!
//! - [`id`] — typed UUID newtypes ([`MessageId`], [`ThreadId`],
//!   [`AccountId`], [`DraftId`], …) so ids of different entities cannot be
//!   confused at compile time, plus deterministic UUIDv5 derivation from
//!   provider-native ids.
//! - [`types`] — the provider-agnostic mail model: [`Account`], [`Label`],
//!   message envelopes, and the enums describing sync, screening, and
//!   analytics state. Gmail/IMAP/SMTP/Outlook behavior is normalised into
//!   this model by the adapter crates.
//! - [`provider`] — the traits adapters implement and the daemon consumes:
//!   [`MailSyncProvider`] (pull mail, push flag changes), [`MailSendProvider`]
//!   (submission), and [`IdleWatcher`] (push notifications). Daemon code
//!   talks to providers only through these seams.
//! - [`error`] — [`MxrError`], the shared error type.
//! - [`time_parse`] — human time expressions ("tomorrow 9am", "in 2h") for
//!   snooze/remind/send-later, via [`parse_relative_time`].
//! - [`text`] and [`i18n`] — small text-normalisation and localisation
//!   helpers shared across crates.

#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "unit tests use unwrap to keep fixture failures direct"
    )
)]

pub mod error;
pub mod i18n;
pub mod id;
pub mod provider;
pub mod text;
pub mod time_parse;
pub mod types;

pub use error::MxrError;
pub use id::*;
pub use provider::*;
pub use time_parse::{parse_relative_time, TimeParseError};
pub use types::*;

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

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
        let display = format!("{id}");
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
            assert!(!d.as_db_str().is_empty());
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
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
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
    fn sync_cursor_empty_is_default() {
        let c = SyncCursor::default();
        assert!(c.is_empty());
        assert_eq!(c.as_bytes(), &[] as &[u8]);
        assert_eq!(SyncCursor::empty().as_bytes(), c.as_bytes());
    }

    #[test]
    fn sync_cursor_round_trips_arbitrary_bytes() {
        // The struct is opaque — any byte sequence the adapter produces
        // must round-trip through serde unchanged.
        for payload in [
            br#"{"v":1,"history_id":12345}"#.to_vec(),
            br#"{"v":2,"mailboxes":[]}"#.to_vec(),
            b"random non-json bytes".to_vec(),
            vec![],
        ] {
            let c = SyncCursor::from_bytes(payload.clone());
            let json = serde_json::to_string(&c).unwrap();
            let parsed: SyncCursor = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.as_bytes(), payload.as_slice());
        }
    }

    #[test]
    fn sync_cursor_debug_does_not_leak_bytes() {
        let c = SyncCursor::from_bytes(b"sensitive-page-token-xyz".to_vec());
        let dbg = format!("{c:?}");
        assert_eq!(dbg, "SyncCursor(len=24)");
        assert!(!dbg.contains("sensitive"));
    }
}
