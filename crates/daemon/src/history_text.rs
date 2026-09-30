//! Text that AI wrote from the user's history (past emails, habits, the
//! relationship summary), remembered so a later refine or humanize can't
//! quietly hand it to a cloud model the user hasn't opted in to. A draft
//! suggestion lands in the compose editor and comes back later as the body
//! of a refine; the daemon only sees that body, so it recognises its own
//! earlier sentences by fingerprint.
//!
//! Kept in memory for [`TTL`]: a daemon restart forgets it (the privacy
//! gates on newly gathered history still hold), which keeps it free of
//! schema and of any copy of mail text on disk.

use mxr_core::id::AccountId;
use parking_lot::Mutex;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Sentences shorter than this ("Thanks,") are too common to attribute.
const MIN_WORDS: usize = 4;
/// Per account; the oldest go first past it.
const MAX_SENTENCES: usize = 5_000;

#[derive(Default)]
pub struct HistoryTextLedger {
    seen: Mutex<HashMap<AccountId, HashMap<u64, Instant>>>,
}

impl HistoryTextLedger {
    /// Remember `text` as written from `account`'s history.
    pub fn record(&self, account: &AccountId, text: &str) {
        let now = Instant::now();
        let mut seen = self.seen.lock();
        let sentences = seen.entry(account.clone()).or_default();
        sentences.retain(|_, at| now.duration_since(*at) < TTL);
        for fingerprint in fingerprints(text) {
            sentences.insert(fingerprint, now);
        }
        if sentences.len() > MAX_SENTENCES {
            let mut ages: Vec<(u64, Instant)> = sentences.iter().map(|(k, v)| (*k, *v)).collect();
            ages.sort_by_key(|(_, at)| *at);
            for (fingerprint, _) in ages.iter().take(sentences.len() - MAX_SENTENCES) {
                sentences.remove(fingerprint);
            }
        }
    }

    /// Whether `text` repeats a sentence AI wrote from the history of
    /// `account`, or of any account when `None` (a request that names none).
    pub fn contains(&self, account: Option<&AccountId>, text: &str) -> bool {
        let now = Instant::now();
        let seen = self.seen.lock();
        let fresh = |sentences: &HashMap<u64, Instant>, fingerprint: &u64| {
            sentences
                .get(fingerprint)
                .is_some_and(|at| now.duration_since(*at) < TTL)
        };
        fingerprints(text).any(|fingerprint| match account {
            Some(account) => seen
                .get(account)
                .is_some_and(|sentences| fresh(sentences, &fingerprint)),
            None => seen
                .values()
                .any(|sentences| fresh(sentences, &fingerprint)),
        })
    }
}

/// One fingerprint per sentence of at least [`MIN_WORDS`] words, ignoring
/// case, punctuation and spacing, so light edits around it still match.
fn fingerprints(text: &str) -> impl Iterator<Item = u64> + '_ {
    text.split(['.', '!', '?', '\n'])
        .map(|sentence| {
            // "it's" and "its" are one word here: apostrophes drop out.
            sentence
                .replace(['\'', '\u{2019}'], "")
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .filter(|words| words.len() >= MIN_WORDS)
        .map(|words| {
            let mut hasher = DefaultHasher::new();
            words.hash(&mut hasher);
            hasher.finish()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remembered_sentence_is_found_through_light_edits_and_only_for_its_account() {
        let ledger = HistoryTextLedger::default();
        let mine = AccountId::new();
        let other = AccountId::new();
        ledger.record(
            &mine,
            "Thanks so much! Yep, resent just now. Shout if it's missing from your inbox!",
        );

        assert!(ledger.contains(
            Some(&mine),
            "Hi Sam,\n\nSHOUT if its missing   from your inbox.\n\nThanks"
        ));
        assert!(ledger.contains(None, "shout if it's missing from your inbox"));
        assert!(!ledger.contains(Some(&other), "shout if it's missing from your inbox"));
        // Short, common lines never attribute a body to history.
        assert!(!ledger.contains(Some(&mine), "Thanks so much."));
        assert!(!ledger.contains(Some(&mine), "I wrote this myself, every word of it."));
    }
}
