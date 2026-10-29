//! A conversation as a list row names it.
//!
//! A legacy Gmail thread id can be held by two accounts, so a thread id alone
//! no longer says which conversation a row is. Rows carry their account;
//! this key carries both through the pending/in-flight state and requests.
//! Older paths that only know a thread id build a key with no account and keep
//! the bare-id behaviour.

use mxr_core::types::{Envelope, Thread};
use mxr_core::{AccountId, ThreadId};
use mxr_protocol::ThreadAccountRef;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThreadKey {
    pub account_id: Option<AccountId>,
    pub thread_id: ThreadId,
}

impl ThreadKey {
    pub fn new(account_id: AccountId, thread_id: ThreadId) -> Self {
        Self {
            account_id: Some(account_id),
            thread_id,
        }
    }

    /// A thread known only by id: matches any account, as before.
    pub fn bare(thread_id: ThreadId) -> Self {
        Self {
            account_id: None,
            thread_id,
        }
    }

    pub fn of_envelope(envelope: &Envelope) -> Self {
        Self::new(envelope.account_id.clone(), envelope.thread_id.clone())
    }

    pub fn of_thread(thread: &Thread) -> Self {
        Self::new(thread.account_id.clone(), thread.id.clone())
    }

    /// True when `envelope` belongs to this conversation. A key without an
    /// account only compares the thread id.
    pub fn matches_envelope(&self, envelope: &Envelope) -> bool {
        envelope.thread_id == self.thread_id
            && self
                .account_id
                .as_ref()
                .is_none_or(|account| account == &envelope.account_id)
    }

    /// True when the daemon's answer (`account_id` may be absent on older
    /// payloads) is for this conversation.
    pub fn matches_ids(&self, thread_id: &ThreadId, account_id: Option<&AccountId>) -> bool {
        &self.thread_id == thread_id
            && match (self.account_id.as_ref(), account_id) {
                (Some(ours), Some(theirs)) => ours == theirs,
                _ => true,
            }
    }

    pub fn thread_ref(&self) -> Option<ThreadAccountRef> {
        self.account_id.as_ref().map(|account_id| ThreadAccountRef {
            thread_id: self.thread_id.clone(),
            account_id: account_id.clone(),
        })
    }
}

impl From<ThreadId> for ThreadKey {
    fn from(thread_id: ThreadId) -> Self {
        Self::bare(thread_id)
    }
}

/// `thread_accounts` for a request: one pair per key that knows its account.
pub fn thread_accounts(keys: &[ThreadKey]) -> Vec<ThreadAccountRef> {
    keys.iter().filter_map(ThreadKey::thread_ref).collect()
}

impl super::MailListRow {
    /// The conversation this row stands for, in the row's own account.
    pub fn thread_key(&self) -> ThreadKey {
        ThreadKey::of_envelope(&self.representative)
    }
}
