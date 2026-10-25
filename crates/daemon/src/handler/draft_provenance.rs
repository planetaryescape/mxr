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
        let config = state.config_snapshot();
        if let Some(model) = config
            .llm
            .compose
            .as_ref()
            .and_then(|compose| compose.model.as_deref())
        {
            return Self::pin_model(&config, feature, model);
        }
        let llm = state.llm.for_feature(feature).pin();
        let share_history = relationship_data_allowed(&state.config_snapshot().llm, &llm);
        let locality = if llm_endpoint_is_local(&llm) {
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

    pub fn pin_compose(
        state: &AppState,
        config: &mxr_config::MxrConfig,
        feature: LlmFeature,
        model: Option<&str>,
    ) -> Self {
        let Some(model) = model else {
            let llm = state.llm.for_feature(feature).pin();
            let share_history = relationship_data_allowed(&state.config_snapshot().llm, &llm);
            let locality = if llm_endpoint_is_local(&llm) {
                AiLocalityData::Local
            } else {
                AiLocalityData::Cloud
            };
            return Self {
                llm,
                locality,
                share_history,
            };
        };
        Self::pin_model(config, feature, model)
    }

    fn pin_model(config: &mxr_config::MxrConfig, feature: LlmFeature, model: &str) -> Self {
        let mut effective = crate::state::base_llm_config(&config.llm);
        effective.model = model.trim().to_string();
        let health = mxr_llm::RouteHealth::default();
        let provider = crate::state::build_llm_provider(&effective, &health);
        let runtime = std::sync::Arc::new(mxr_llm::LlmRuntime::with_route_health(provider, health));
        let llm = runtime.for_feature(feature).pin();
        let share_history = relationship_data_allowed(&config.llm, &llm);
        let locality = if llm_endpoint_is_local(&llm) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compose_default_pins_all_four_drafting_features_and_survives_reload() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.llm.model = "base-model".into();
        config.llm.compose = Some(mxr_config::LlmComposeConfig {
            model: Some("gpt-6-luna".into()),
        });
        state.set_config_for_test(config.clone()).await;

        for feature in [
            LlmFeature::DraftAssist,
            LlmFeature::DraftNew,
            LlmFeature::DraftRefine,
            LlmFeature::HumanizeRewrite,
        ] {
            let pinned = DraftPolicy::pin(&state, feature);
            assert_eq!(pinned.llm.model_name(), "gpt-6-luna", "{feature:?}");
        }

        let pinned = DraftPolicy::pin(&state, LlmFeature::DraftNew);
        config.llm.compose.as_mut().unwrap().model = Some("claude-haiku-5-5".into());
        state.set_config_for_test(config).await;
        assert_eq!(pinned.llm.model_name(), "gpt-6-luna");
    }

    #[tokio::test]
    async fn legacy_draft_override_remains_active_without_a_compose_default() {
        let state = AppState::in_memory().await.unwrap();
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.llm.compose = None;
        config.llm.overrides.draft_new = Some(mxr_config::LlmOverrideConfig {
            model: Some("legacy-draft-model".into()),
            ..Default::default()
        });
        state.set_config_for_test(config).await;

        assert_eq!(
            DraftPolicy::pin(&state, LlmFeature::DraftNew)
                .llm
                .model_name(),
            "legacy-draft-model"
        );
    }
}
