
use std::collections::{HashMap, HashSet};

use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use mxr_core::Envelope;

use super::MutationEffect;
use super::MutationId;

#[derive(Debug, Default)]
pub struct PendingOptimisticState {
    removed: HashMap<MutationId, Vec<MessageId>>,
    flag_overrides: HashMap<MutationId, Vec<(MessageId, MessageFlags)>>,
    removed_lookup: HashSet<MessageId>,
    flag_lookup: HashMap<MessageId, MessageFlags>,
}

impl PendingOptimisticState {
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
            _ => {}
        }
    }

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
            keywords: std::collections::BTreeSet::new(),
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
