use mxr_core::id::{AccountId, DraftId};
use mxr_protocol::ComposeProposalData;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(30 * 60);
const MAX_PROPOSALS: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct StoredProposal {
    pub account_id: AccountId,
    pub draft_id: Option<DraftId>,
    pub grounded_figures: Vec<String>,
    pub proposal: ComposeProposalData,
}

struct Entry {
    created: Instant,
    stored: StoredProposal,
}

pub(crate) struct ComposeProposals {
    ttl: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for ComposeProposals {
    fn default() -> Self {
        Self::with_ttl(TTL)
    }
}

impl ComposeProposals {
    pub(crate) fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::default(),
        }
    }

    pub(crate) fn insert(&self, stored: StoredProposal) {
        let mut entries = self.entries.lock();
        entries.retain(|_, entry| entry.created.elapsed() < self.ttl);
        while entries.len() >= MAX_PROPOSALS {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.created)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            entries.remove(&oldest);
        }
        entries.insert(
            stored.proposal.proposal_id.clone(),
            Entry {
                created: Instant::now(),
                stored,
            },
        );
    }

    pub(crate) fn get(&self, proposal_id: &str) -> Option<StoredProposal> {
        let mut entries = self.entries.lock();
        let expired = entries
            .get(proposal_id)
            .is_some_and(|entry| entry.created.elapsed() >= self.ttl);
        if expired {
            entries.remove(proposal_id);
        }
        entries.get(proposal_id).map(|entry| entry.stored.clone())
    }

    pub(crate) fn remove(&self, proposal_id: &str) {
        self.entries.lock().remove(proposal_id);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{AiLocalityData, AssistActionData, DraftProvenanceData};

    fn stored(id: &str) -> StoredProposal {
        StoredProposal {
            account_id: AccountId::new(),
            draft_id: None,
            grounded_figures: Vec::new(),
            proposal: ComposeProposalData {
                proposal_id: id.into(),
                action: AssistActionData::Write,
                base: String::new(),
                text: "Hi.".into(),
                changes: Vec::new(),
                gaps: Vec::new(),
                provenance: DraftProvenanceData {
                    model: "m".into(),
                    locality: AiLocalityData::Local,
                    history_used: false,
                    voice_examples: Vec::new(),
                    conversation: Vec::new(),
                    rewrite: None,
                },
                yours_changed: Vec::new(),
                composer_version: String::new(),
                style_marks: Vec::new(),
                style_score: 100,
                style_version: String::new(),
            },
        }
    }

    #[test]
    fn a_proposal_expires_after_its_time_to_live() {
        let proposals = ComposeProposals::with_ttl(Duration::from_millis(20));
        proposals.insert(stored("a"));
        assert!(proposals.get("a").is_some());
        std::thread::sleep(Duration::from_millis(40));
        assert!(proposals.get("a").is_none());
        assert_eq!(proposals.len(), 0, "an expired proposal is dropped");
    }
}
