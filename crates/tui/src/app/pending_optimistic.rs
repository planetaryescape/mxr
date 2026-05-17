//! Tracks the message-id state of in-flight optimistic mutations so a
//! stale envelope refresh from the daemon can't undo the local change
//! before the mutation acks.
//!
//! Concrete bug this fixes: user archives a message in INBOX, the row
//! disappears optimistically, then a `LabelEnvelopes` response from a
//! sync- or mutation-triggered refresh lands with the still-present
//! envelope (because the daemon hasn't processed the archive yet) and
//! overwrites the local list — the archived message "pops back in"
//! until the mutation finally acks. The fix is to filter daemon-sourced
//! envelope responses against the pending optimistic state for as long
//! as the corresponding mutation is in flight.
//!
//! Mirrors the Linear-style mutation gate: server responses can't
//! clobber a pending optimistic change.

use std::collections::{HashMap, HashSet};

use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::Envelope;

use super::MutationEffect;
use super::MutationId;

/// In-flight optimistic state keyed by the mutation that produced it.
/// On mutation ack / rollback the entries for that id are dropped.
#[derive(Debug, Default)]
pub struct PendingOptimisticState {
    /// Per-mutation list of message IDs removed from visible lists.
    removed: HashMap<MutationId, Vec<MessageId>>,
    /// Per-mutation list of optimistic flag overrides.
    flag_overrides: HashMap<MutationId, Vec<(MessageId, MessageFlags)>>,
    /// Cached union of `removed` values — fast O(1) membership check.
    removed_lookup: HashSet<MessageId>,
    /// Cached latest-write flags per message — masking value for refresh.
    flag_lookup: HashMap<MessageId, MessageFlags>,
}

impl PendingOptimisticState {
    /// Record the optimistic effect tied to a mutation. Idempotent if
    /// called twice for the same id (last write wins).
    pub fn record(&mut self, id: MutationId, effect: &MutationEffect) {
        match effect {
            MutationEffect::RemoveFromList(mid) => {
                self.removed.entry(id).or_default().push(mid.clone());
                self.removed_lookup.insert(mid.clone());
            }
            MutationEffect::RemoveFromListMany(ids) => {
                self.removed
                    .entry(id)
                    .or_default()
                    .extend(ids.iter().cloned());
                for mid in ids {
                    self.removed_lookup.insert(mid.clone());
                }
            }
            MutationEffect::UpdateFlags { message_id, flags } => {
                self.flag_overrides
                    .entry(id)
                    .or_default()
                    .push((message_id.clone(), *flags));
                self.flag_lookup.insert(message_id.clone(), *flags);
            }
            MutationEffect::UpdateFlagsMany { updates } => {
                self.flag_overrides
                    .entry(id)
                    .or_default()
                    .extend(updates.iter().cloned());
                for (mid, flags) in updates {
                    self.flag_lookup.insert(mid.clone(), *flags);
                }
            }
            // ModifyLabels, ReplyLater, RefreshList, StatusOnly,
            // SentSuccess: label and reply-later overrides reconcile
            // cleanly on next refresh because the daemon writes them
            // before responding, and the rollback path covers the
            // failure case. No need to mask refresh responses.
            _ => {}
        }
    }

    /// Drop everything recorded for this mutation id. Call from both
    /// the ack path (mutation succeeded, optimistic state is now
    /// authoritative locally and on the daemon) and the failure path
    /// (snapshot rollback already restored the pre-mutation state).
    pub fn clear(&mut self, id: MutationId) {
        if let Some(ids) = self.removed.remove(&id) {
            for mid in &ids {
                if !self
                    .removed
                    .values()
                    .any(|other| other.iter().any(|m| m == mid))
                {
                    self.removed_lookup.remove(mid);
                }
            }
        }
        if let Some(entries) = self.flag_overrides.remove(&id) {
            for (mid, _) in &entries {
                let latest = self
                    .flag_overrides
                    .values()
                    .flat_map(|entries| entries.iter().rev())
                    .find(|(m, _)| m == mid)
                    .map(|(_, flags)| *flags);
                match latest {
                    Some(flags) => {
                        self.flag_lookup.insert(mid.clone(), flags);
                    }
                    None => {
                        self.flag_lookup.remove(mid);
                    }
                }
            }
        }
    }

    pub fn is_removed(&self, message_id: &MessageId) -> bool {
        self.removed_lookup.contains(message_id)
    }

    pub fn flag_override(&self, message_id: &MessageId) -> Option<MessageFlags> {
        self.flag_lookup.get(message_id).copied()
    }

    /// Apply the pending state to a daemon-sourced envelope list:
    /// drop optimistically-removed ids, then mask flag fields for any
    /// remaining envelope with a pending flag override. Use this at
    /// every list-replacement site that consumes a refresh response.
    pub fn apply(&self, envelopes: &mut Vec<Envelope>) {
        if self.removed_lookup.is_empty() && self.flag_lookup.is_empty() {
            return;
        }
        envelopes.retain(|env| !self.removed_lookup.contains(&env.id));
        if !self.flag_lookup.is_empty() {
            for env in envelopes {
                if let Some(flags) = self.flag_lookup.get(&env.id) {
                    env.flags = *flags;
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.flag_overrides.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};

    fn fixture_envelope(id: MessageId) -> Envelope {
        Envelope {
            id,
            account_id: AccountId::new(),
            provider_id: "p".into(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: "a@example.com".into(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: String::new(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: String::new(),
            has_attachments: false,
            size_bytes: 0,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
        }
    }

    #[test]
    fn apply_drops_optimistically_removed_envelopes() {
        let mut state = PendingOptimisticState::default();
        let mid = MessageId::new();
        let other = MessageId::new();
        let id = MutationId::from_raw(1);
        state.record(id, &MutationEffect::RemoveFromList(mid.clone()));

        let mut envelopes = vec![
            fixture_envelope(mid.clone()),
            fixture_envelope(other.clone()),
        ];
        state.apply(&mut envelopes);
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].id, other);

        state.clear(id);
        let mut envelopes = vec![fixture_envelope(mid), fixture_envelope(other)];
        state.apply(&mut envelopes);
        assert_eq!(envelopes.len(), 2, "clear() releases the removal mask");
    }

    #[test]
    fn apply_overrides_flags_for_pending_mutations() {
        let mut state = PendingOptimisticState::default();
        let mid = MessageId::new();
        let id = MutationId::from_raw(2);
        state.record(
            id,
            &MutationEffect::UpdateFlags {
                message_id: mid.clone(),
                flags: MessageFlags::READ,
            },
        );

        let mut env = fixture_envelope(mid);
        env.flags = MessageFlags::empty();
        let mut envelopes = vec![env];
        state.apply(&mut envelopes);
        assert!(envelopes[0].flags.contains(MessageFlags::READ));
    }

    #[test]
    fn clear_only_releases_when_no_other_mutation_pins_the_id() {
        let mut state = PendingOptimisticState::default();
        let mid = MessageId::new();
        let first = MutationId::from_raw(3);
        let second = MutationId::from_raw(4);
        state.record(first, &MutationEffect::RemoveFromList(mid.clone()));
        state.record(second, &MutationEffect::RemoveFromList(mid.clone()));
        state.clear(first);
        assert!(state.is_removed(&mid));
        state.clear(second);
        assert!(!state.is_removed(&mid));
    }

    #[test]
    fn clear_recomputes_flag_lookup_to_remaining_latest_entry() {
        let mut state = PendingOptimisticState::default();
        let mid = MessageId::new();
        let first = MutationId::from_raw(5);
        let second = MutationId::from_raw(6);
        state.record(
            first,
            &MutationEffect::UpdateFlags {
                message_id: mid.clone(),
                flags: MessageFlags::READ,
            },
        );
        state.record(
            second,
            &MutationEffect::UpdateFlags {
                message_id: mid.clone(),
                flags: MessageFlags::READ | MessageFlags::STARRED,
            },
        );
        assert_eq!(
            state.flag_override(&mid),
            Some(MessageFlags::READ | MessageFlags::STARRED)
        );
        state.clear(second);
        assert_eq!(state.flag_override(&mid), Some(MessageFlags::READ));
        state.clear(first);
        assert_eq!(state.flag_override(&mid), None);
    }
}
