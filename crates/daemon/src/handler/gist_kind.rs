//! What kind of mail a conversation is, for gists: who may be asked about
//! (people's conversations, and Updates when a client says so) and which
//! prompt writes it (a conversation's, or a notification's).
//!
//! The kind comes from the shared `mail_kind` rules applied to the newest
//! message from someone other than the account, so a gist, Inbox and Updates
//! agree on what a message is.

use super::mail_kind::SenderKind;
use super::places::AccountKinds;
use super::HandlerError;
use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::Envelope;
use std::collections::HashMap;

/// Which prompt writes a conversation's gist, and so part of its cache key:
/// a sender moved between Messages and Updates gets a new gist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GistStyle {
    /// People writing to each other: the state of the exchange and what is
    /// asked of the owner.
    Conversation,
    /// A machine reporting a fact: the one number, state or date that matters.
    Notification,
}

impl GistStyle {
    pub(super) const fn of(kind: SenderKind) -> Self {
        match kind {
            SenderKind::Automated => Self::Notification,
            SenderKind::Person | SenderKind::List | SenderKind::Denied => Self::Conversation,
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Notification => "notification",
        }
    }
}

/// The kind of each conversation, by its newest message from someone else.
/// A conversation only the owner has written in is with the people they
/// wrote to. Absent from the map: the message is gone (trashed since).
pub(super) async fn thread_kinds(
    state: &AppState,
    threads: &[(&ThreadId, &[Envelope])],
) -> Result<HashMap<ThreadId, SenderKind>, HandlerError> {
    let mut by_account: HashMap<&AccountId, Vec<(&ThreadId, &[Envelope])>> = HashMap::new();
    for (thread_id, envelopes) in threads {
        if let Some(newest) = envelopes.last() {
            by_account
                .entry(&newest.account_id)
                .or_default()
                .push((thread_id, envelopes));
        }
    }
    let mut out = HashMap::new();
    for (account_id, threads) in by_account {
        let mut messages = Vec::new();
        for (_, envelopes) in &threads {
            for envelope in *envelopes {
                if let Some(message) = state.store.place_message(&envelope.id).await? {
                    messages.push(message);
                }
            }
        }
        let kinds = AccountKinds::load(state, account_id, &messages).await?;
        let messages: HashMap<_, _> = messages.into_iter().map(|m| (m.id.clone(), m)).collect();
        for (thread_id, envelopes) in threads {
            let theirs = envelopes
                .iter()
                .rev()
                .find(|envelope| !kinds.is_self(&envelope.from.email));
            let Some(theirs) = theirs else {
                out.insert((*thread_id).clone(), SenderKind::Person);
                continue;
            };
            if let Some(message) = messages.get(&theirs.id) {
                out.insert((*thread_id).clone(), kinds.classification(message).kind);
            }
        }
    }
    Ok(out)
}

/// The prompt style for one conversation; a conversation whose message is
/// gone reads as a conversation.
pub(super) async fn style_of(
    state: &AppState,
    thread_id: &ThreadId,
    envelopes: &[Envelope],
) -> Result<GistStyle, HandlerError> {
    let kinds = thread_kinds(state, &[(thread_id, envelopes)]).await?;
    Ok(kinds
        .get(thread_id)
        .copied()
        .map_or(GistStyle::Conversation, GistStyle::of))
}
