//! Text that AI wrote from the user's history (past emails, habits, the
//! relationship summary), remembered so a later refine or humanize can't
//! quietly hand it to a cloud model the user hasn't opted in to. A draft
//! suggestion lands in the compose editor and comes back later as the body
//! of a refine; the daemon only sees that body, so it recognises its own
//! earlier sentences by fingerprint.
//!
//! Only fingerprints are stored (SQLite, migration 055), never text, for
//! [`TTL`], so the gate holds across daemon restarts.

use chrono::{Duration, Utc};
use mxr_core::id::AccountId;
use mxr_store::Store;
use sha2::{Digest, Sha256};

const TTL_DAYS: i64 = 7;
/// Sentences shorter than this ("Thanks so much.") are too common to
/// attribute.
const MIN_WORDS: usize = 4;
/// Per account; the oldest go first past it.
const MAX_SENTENCES: u32 = 5_000;

/// Remember `text` as written from `account`'s history. A failure is
/// logged: the privacy gates on newly gathered history still hold.
pub(crate) async fn record(store: &Store, account: &AccountId, text: &str) {
    let fingerprints = fingerprints(text);
    let now = Utc::now();
    if let Err(error) = store
        .record_history_fingerprints(
            account,
            &fingerprints,
            now,
            now - Duration::days(TTL_DAYS),
            MAX_SENTENCES,
        )
        .await
    {
        tracing::warn!(%error, "couldn't remember history-derived draft text");
    }
}

/// The accounts whose history `text` repeats a sentence of: just
/// `account` when given, else any (a request that names none). Empty when
/// the text carries nothing written from history.
pub(crate) async fn source_accounts(
    store: &Store,
    account: Option<&AccountId>,
    text: &str,
) -> Result<Vec<AccountId>, sqlx::Error> {
    store
        .history_fingerprint_accounts(
            account,
            &fingerprints(text),
            Utc::now() - Duration::days(TTL_DAYS),
        )
        .await
}

/// One fingerprint per sentence of at least [`MIN_WORDS`] words, ignoring
/// case, punctuation, apostrophes and spacing, so light edits around it
/// still match. A stable hash (the first 8 bytes of SHA-256), so rows
/// written before a restart still match after it.
fn fingerprints(text: &str) -> Vec<i64> {
    text.split(['.', '!', '?', '\n'])
        .map(|sentence| {
            sentence
                .replace(['\'', '\u{2019}'], "")
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .filter(|words| words.len() >= MIN_WORDS)
        .map(|words| {
            let digest = Sha256::digest(words.join(" ").as_bytes());
            let mut head = [0u8; 8];
            head.copy_from_slice(&digest[..8]);
            i64::from_be_bytes(head)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn found(store: &Store, account: Option<&AccountId>, text: &str) -> Vec<AccountId> {
        source_accounts(store, account, text).await.unwrap()
    }

    #[tokio::test]
    async fn a_remembered_sentence_is_found_through_light_edits_and_only_for_its_account() {
        let store = Store::in_memory().await.unwrap();
        let mine = crate::test_fixtures::test_account_with_id(AccountId::new());
        let other = crate::test_fixtures::test_account_with_id(AccountId::new());
        store.insert_account(&mine).await.unwrap();
        store.insert_account(&other).await.unwrap();
        record(
            &store,
            &mine.id,
            "Thanks so much! Yep, resent just now. Shout if it's missing from your inbox!",
        )
        .await;

        assert_eq!(
            found(
                &store,
                Some(&mine.id),
                "Hi Sam,\n\nSHOUT if its missing   from your inbox.\n\nThanks"
            )
            .await,
            vec![mine.id.clone()]
        );
        assert_eq!(
            found(&store, None, "shout if it's missing from your inbox").await,
            vec![mine.id.clone()]
        );
        assert!(found(
            &store,
            Some(&other.id),
            "shout if it's missing from your inbox"
        )
        .await
        .is_empty());
        // Short, common lines never attribute a body to history.
        assert!(found(&store, Some(&mine.id), "Thanks so much.")
            .await
            .is_empty());
        assert!(found(
            &store,
            Some(&mine.id),
            "I wrote this myself, every word of it."
        )
        .await
        .is_empty());
    }

    // The gate survives a daemon restart: what one process records, a
    // fresh store on the same database finds.
    #[tokio::test]
    async fn history_text_is_still_recognised_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mxr.db");
        let account = crate::test_fixtures::test_account_with_id(AccountId::new());
        {
            let store = Store::new(&path).await.unwrap();
            store.insert_account(&account).await.unwrap();
            record(
                &store,
                &account.id,
                "Shout if it's missing from your inbox.",
            )
            .await;
        }
        let reopened = Store::new(&path).await.unwrap();
        assert_eq!(
            found(
                &reopened,
                None,
                "Hi,\n\nshout if it's missing from your inbox\n"
            )
            .await,
            vec![account.id]
        );
    }

    #[test]
    fn fingerprints_are_stable_across_processes() {
        // A fixed value: a per-process hasher would break the gate after a
        // restart.
        assert_eq!(
            fingerprints("Shout if it's missing from your inbox"),
            fingerprints("shout if its missing from your inbox!")
        );
        assert_eq!(fingerprints("one two three four"), vec![FOUR_WORDS]);
    }

    /// The first 8 bytes of SHA-256("one two three four"), big-endian.
    const FOUR_WORDS: i64 = 1_242_828_351_107_727_526;
}
