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
//! - [`natural_time`] — the one natural-language time parser ("fri 3",
//!   "tomorrow 9am", "in 2h") for snooze, send later and reminders, with
//!   spans, assumptions and choices.
//! - [`text`] and [`i18n`] — small text-normalisation and localisation
//!   helpers shared across crates.

#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "unit tests use unwrap to keep fixture failures direct"
    )
)]

pub mod account_identity;
pub mod arrival;
pub mod asks;
pub mod assessment;
pub mod error;
pub mod i18n;
pub mod id;
pub mod mail_evidence;
pub mod natural_time;
pub mod provider;
pub mod recovery;
pub mod remote;
pub mod text;
pub mod types;

pub use account_identity::*;
pub use arrival::{ArrivalEvidence, SenderCheck, UnverifiedReason};
pub use error::MxrError;
pub use id::*;
pub use provider::*;
pub use recovery::*;
pub use remote::*;
pub use types::*;

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;

    fn owned(primary: &str, aliases: &[&str]) -> OwnedAddresses {
        OwnedAddresses::new(Some(primary), aliases.iter().copied())
    }

    #[test]
    fn owned_addresses_match_ignoring_case_and_whitespace_and_keep_primary_first() {
        let owned = OwnedAddresses::new(
            Some(" Me@Example.com "),
            ["alias@example.com", "me@example.com", ""],
        );
        assert_eq!(owned.primary(), Some("Me@Example.com"));
        assert!(owned.contains("me@example.com"));
        assert!(owned.contains("  ME@EXAMPLE.COM "));
        assert!(owned.contains("Alias@Example.com"));
        assert!(!owned.contains("someone@example.com"));
        assert!(!owned.contains(""));
        assert_eq!(
            owned.iter().collect::<Vec<_>>(),
            vec!["Me@Example.com", "alias@example.com"]
        );
        assert_eq!(
            owned.canonical("ALIAS@example.com"),
            Some("alias@example.com")
        );
    }

    #[test]
    fn owned_addresses_fold_ascii_only_so_unicode_spellings_stay_distinct() {
        let owned = owned(
            "user@example.com",
            &["user@BÜCHER.example", "user@bücher.example"],
        );
        assert_eq!(
            owned.iter().collect::<Vec<_>>(),
            vec![
                "user@example.com",
                "user@BÜCHER.example",
                "user@bücher.example"
            ]
        );
        assert_eq!(
            owned.canonical("user@bücher.example"),
            Some("user@bücher.example")
        );
        assert_eq!(
            owned.canonical("USER@BÜCHER.EXAMPLE"),
            Some("user@BÜCHER.example")
        );
        assert!(owned.contains("user@BÜCHER.EXAMPLE"));
        assert!(!owned.contains("user@bucher.example"));
    }

    #[test]
    fn owned_addresses_always_include_a_primary_passed_in() {
        let owned = OwnedAddresses::new(Some("me@example.com"), []);
        assert!(owned.contains("me@example.com"));
        assert_eq!(owned.primary(), Some("me@example.com"));
        assert!(OwnedAddresses::new(None, []).is_empty());
    }

    #[test]
    fn registry_owned_set_follows_replace_for_each_account() {
        let account_a = AccountId::new();
        let lookup = InMemoryAccountAddressLookup::new();
        lookup.replace([(account_a.clone(), owned("me@example.com", &[]))]);
        assert!(!lookup.owned(&account_a).contains("alias@example.com"));

        lookup.replace([(
            account_a.clone(),
            owned("me@example.com", &["alias@example.com"]),
        )]);
        assert!(lookup.owned(&account_a).contains("alias@example.com"));
        assert_eq!(lookup.owned(&account_a).primary(), Some("me@example.com"));
        assert!(lookup.owned(&AccountId::new()).is_empty());
    }

    #[test]
    fn in_memory_account_address_lookup_classifies_correctly() {
        let account_a = AccountId::new();
        let account_b = AccountId::new();
        let lookup = InMemoryAccountAddressLookup::new();
        assert!(!lookup.is_loaded());
        assert!(!lookup.is_account_address(&account_a, "anyone@example.com"));

        lookup.replace([
            (
                account_a.clone(),
                owned(" A@Example.com ", &["alias@example.com"]),
            ),
            (account_b.clone(), owned("b@other.com", &[])),
        ]);
        assert!(lookup.is_loaded());

        assert!(lookup.is_account_address(&account_a, "a@example.com"));
        assert!(lookup.is_account_address(&account_a, "A@EXAMPLE.COM"));
        assert!(lookup.is_account_address(&account_a, "alias@example.com"));
        assert!(lookup.is_account_address(&account_b, "b@other.com"));

        assert!(!lookup.is_account_address(&account_a, "b@other.com"));
        assert!(!lookup.is_account_address(&account_b, "a@example.com"));
        assert!(!lookup.is_account_address(&account_a, "other@example.com"));
        assert!(!lookup.is_account_address(&AccountId::new(), "a@example.com"));

        lookup.replace([(account_a.clone(), owned("fresh@example.com", &[]))]);
        assert!(lookup.is_loaded());
        assert!(lookup.is_account_address(&account_a, "fresh@example.com"));
        assert!(!lookup.is_account_address(&account_a, "a@example.com"));
    }

    #[test]
    fn stale_account_address_lookup_reads_as_not_loaded_until_replaced() {
        let account = AccountId::new();
        let lookup = InMemoryAccountAddressLookup::new();
        lookup.replace([(account.clone(), owned("a@example.com", &[]))]);
        assert!(lookup.is_account_address(&account, "a@example.com"));

        lookup.mark_stale();
        assert!(lookup.is_stale());
        assert!(!lookup.is_loaded());
        assert!(
            lookup.take_repair_owed(),
            "a stale set owes a direction repair"
        );
        assert!(!lookup.take_repair_owed(), "the debt is taken once");

        lookup.replace([(account.clone(), owned("b@example.com", &[]))]);
        assert!(!lookup.is_stale());
        assert!(lookup.is_loaded());
        assert!(lookup.is_account_address(&account, "b@example.com"));
    }

    #[test]
    fn stale_lookup_still_answers_from_the_held_set_for_a_pass_already_running() {
        let account = AccountId::new();
        let lookup = InMemoryAccountAddressLookup::new();
        lookup.replace([(account.clone(), owned("a@example.com", &[]))]);

        lookup.mark_stale();
        assert!(!lookup.is_loaded());
        assert!(
            lookup.is_account_address(&account, "a@example.com"),
            "an owned sender must not read as inbound after a failed rebuild"
        );
    }

    #[test]
    fn sync_cursor_round_trips_arbitrary_bytes() {
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
