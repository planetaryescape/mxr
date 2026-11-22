use crate::state::{llm_answer_locality, relationship_data_allowed, AppState};
use mxr_llm::{LlmFeature, PinnedLlm};
use mxr_protocol::{
    DraftProvenanceData, DraftRewriteOutcomeData, DraftRewriteProvenanceData, DraftSourceData,
};

pub(crate) struct DraftPolicy {
    pub llm: PinnedLlm,
    pub share_history: bool,
}

impl DraftPolicy {
    pub fn pin(state: &AppState, feature: LlmFeature) -> Self {
        let config = state.config_snapshot();
        match crate::state::compose_model_for(&config.llm, feature) {
            Some(model) => Self::pin_model(state, &config, feature, model),
            None => Self::pin_feature(state, &config, feature),
        }
    }

    pub fn pin_compose(
        state: &AppState,
        config: &mxr_config::MxrConfig,
        feature: LlmFeature,
        model: Option<&str>,
    ) -> Self {
        match model {
            Some(model) => Self::pin_model(state, config, feature, model),
            None => Self::pin_feature(state, config, feature),
        }
    }

    fn pin_feature(state: &AppState, config: &mxr_config::MxrConfig, feature: LlmFeature) -> Self {
        let llm = state.llm.for_feature(feature).pin();
        let share_history = relationship_data_allowed(&config.llm, &llm);
        Self { llm, share_history }
    }

    fn pin_model(
        state: &AppState,
        config: &mxr_config::MxrConfig,
        feature: LlmFeature,
        model: &str,
    ) -> Self {
        let mut effective = crate::state::base_llm_config(&config.llm);
        effective.model = model.trim().to_string();
        let health = state.llm.route_health().clone();
        let fallback = crate::state::llm_fallback(&config.llm).ok().flatten();
        let chain = crate::state::llm_fallback_chain(
            &config.llm,
            fallback.as_ref(),
            Some(feature),
            &effective,
        );
        let provider = crate::state::build_llm_provider(&effective, chain.as_ref(), &health);
        let llm = state.llm.for_feature(feature).pin_with(provider);
        let share_history = relationship_data_allowed(&config.llm, &llm);
        Self { llm, share_history }
    }

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
            locality: llm_answer_locality(&self.llm, model),
            history_used,
            voice_examples,
            conversation,
            rewrite: None,
        }
    }

    pub fn rewrite_provenance(
        &self,
        model: &str,
        history_used: bool,
        outcome: DraftRewriteOutcomeData,
    ) -> DraftRewriteProvenanceData {
        DraftRewriteProvenanceData {
            model: model.to_string(),
            locality: llm_answer_locality(&self.llm, model),
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
}
