//! What a draft request says about itself. The provider is pinned once per
//! request: whether the user's other mail may go into the prompt, the call
//! itself and the provenance the client shows all follow that one endpoint,
//! even if a config reload swaps providers mid-request.

use crate::state::{llm_endpoint_is_local, relationship_data_allowed, AppState};
use mxr_llm::{LlmFeature, PinnedLlm};
use mxr_protocol::{
    AiLocalityData, DraftProvenanceData, DraftRewriteOutcomeData, DraftRewriteProvenanceData,
    DraftSourceData,
};

pub(crate) struct DraftPolicy {
    pub llm: PinnedLlm,
    pub locality: AiLocalityData,
    /// The user's other mail may reach this model (see
    /// [`relationship_data_allowed`]).
    pub share_history: bool,
}

impl DraftPolicy {
    pub fn pin(state: &AppState, feature: LlmFeature) -> Self {
        let llm = state.llm.for_feature(feature).pin();
        let share_history = relationship_data_allowed(&state.config_snapshot().llm, &llm);
        let locality = if llm_endpoint_is_local(llm.base_url()) {
            AiLocalityData::Local
        } else {
            AiLocalityData::Cloud
        };
        Self {
            llm,
            locality,
            share_history,
        }
    }

    /// The provenance of text this policy's model wrote. `model` is the name
    /// the endpoint answered with; the pinned name stands in when it gave
    /// none. `history_used` reports what the prompt carried, never the gate,
    /// so a gate bug would show rather than hide.
    pub fn provenance(
        &self,
        model: &str,
        history_used: bool,
        voice_examples: Vec<DraftSourceData>,
        conversation: Vec<DraftSourceData>,
    ) -> DraftProvenanceData {
        let model = if model.trim().is_empty() {
            self.llm.model_name()
        } else {
            model
        };
        DraftProvenanceData {
            model: model.to_string(),
            locality: self.locality,
            history_used,
            voice_examples,
            conversation,
            rewrite: None,
        }
    }

    /// A rewrite pass by this policy's model. `model` is the name the
    /// endpoint answered with; `history_used` when the rewrite saw the
    /// user's habits or past emails.
    pub fn rewrite_provenance(
        &self,
        model: &str,
        history_used: bool,
        outcome: DraftRewriteOutcomeData,
    ) -> DraftRewriteProvenanceData {
        DraftRewriteProvenanceData {
            model: model.to_string(),
            locality: self.locality,
            history_used,
            outcome,
            rejected_by: None,
        }
    }
}
