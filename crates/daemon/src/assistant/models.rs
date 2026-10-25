//! Which model the assistant runs on: the choices the picker lists, whether
//! each can be used now, and setting the one in use.
//!
//! The model is the `assistant` feature's `[llm.overrides.assistant]` model,
//! one for every conversation. Where a model runs follows from its id (see
//! `mxr_llm::routes_for_model`), so a choice is just an id.

use crate::llm_keys::LlmKeys;
use crate::state::{effective_llm_config, llm_model_routes, AppState};
use mxr_config::{EffectiveLlmConfig, LlmConfig, LlmOverrideConfig};
use mxr_llm::{ApiKeySource, LlmFeature};
use mxr_protocol::{
    AssistantModelChangeData, AssistantModelChoiceData, AssistantModelsData, ResponseData,
};

/// Names for the ids mxr ships. Any other id is shown as it is written.
const LABELS: [(&str, &str); 5] = [
    ("gemma4:latest", "Gemma 4"),
    ("gpt-6-luna", "GPT-6 Luna"),
    ("gpt-5.6-terra", "GPT-5.6 Terra"),
    ("claude-haiku-5-5", "Claude Haiku 5.5"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
];

fn label(id: &str) -> String {
    LABELS
        .iter()
        .find(|(known, _)| *known == id)
        .map_or_else(|| id.to_string(), |(_, label)| (*label).to_string())
}

/// Ollama serves `gemma4` and `gemma4:latest` as the same model, and a
/// config written by hand uses either.
fn same_model(a: &str, b: &str) -> bool {
    let bare = |id: &str| {
        id.trim()
            .strip_suffix(":latest")
            .unwrap_or_else(|| id.trim())
            .to_string()
    };
    bare(a) == bare(b)
}

/// The assistant's settings with `model` in place of the one configured.
fn effective_with(config: &LlmConfig, model: &str) -> EffectiveLlmConfig {
    let override_config = LlmOverrideConfig {
        model: Some(model.to_string()),
        ..config.overrides.assistant.clone().unwrap_or_default()
    };
    config.effective_override(&override_config)
}

fn judge(config: &LlmConfig, keys: &dyn ApiKeySource, id: &str) -> AssistantModelChoiceData {
    let effective = effective_with(config, id);
    let routes = llm_model_routes(&effective);
    let cloud = routes.is_some() || !mxr_llm::is_loopback_endpoint(&effective.base_url);
    let unavailable_reason = if mxr_config::is_demo_instance() {
        Some("The demo profile runs a built-in script, not a model.".to_string())
    } else if !effective.enabled {
        Some("AI features are off. Turn them on: mxr config set llm.enabled true".to_string())
    } else if let Some(missing) = routes.as_deref().and_then(|routes| {
        let first = routes.first()?;
        routes
            .iter()
            .all(|route| keys.api_key(route.provider).is_none())
            .then_some(first.provider)
    }) {
        Some(format!(
            "Add an {} key: mxr llm key set {}",
            missing.label(),
            missing.key()
        ))
    } else if cloud && !config.allow_cloud_relationship_data {
        Some(
            "The assistant reads across your mailbox, so mxr keeps it on this machine until you \
             allow this: mxr config set llm.allow_cloud_relationship_data true"
                .to_string(),
        )
    } else {
        None
    };
    AssistantModelChoiceData {
        id: id.to_string(),
        label: label(id),
        cloud,
        sees: if !cloud || config.allow_cloud_relationship_data {
            "this conversation and your past mail".to_string()
        } else {
            "this conversation only".to_string()
        },
        available: unavailable_reason.is_none(),
        unavailable_reason,
    }
}

fn models_for(config: &LlmConfig, keys: &dyn ApiKeySource) -> AssistantModelsData {
    let ids = config.assistant_model_ids();
    let configured = effective_llm_config(config, LlmFeature::Assistant)
        .model
        .trim()
        .to_string();
    // Name the listed id when the configured one is the same model, so the
    // picker can mark it.
    let current = ids
        .iter()
        .find(|id| same_model(id, &configured))
        .cloned()
        .or_else(|| (!configured.is_empty()).then_some(configured));
    AssistantModelsData {
        current,
        choices: ids.iter().map(|id| judge(config, keys, id)).collect(),
    }
}

/// The choices with their standing now. Keys are read from the key store
/// and the environment on each call, so a key added a moment ago counts.
pub(crate) fn models(state: &AppState) -> AssistantModelsData {
    models_for(&state.config_snapshot().llm, &LlmKeys::at_default_path())
}

/// Choices judged for the compose writer: the base LLM route and its
/// credentials, with cloud history sharing reported separately. Writing a
/// reply only needs the current conversation, so the history opt-in does not
/// make a cloud model unavailable.
pub(crate) fn compose_models(state: &AppState) -> AssistantModelsData {
    let config = state.config_snapshot().llm;
    let keys = LlmKeys::at_default_path();
    let current = config
        .compose
        .as_ref()
        .and_then(|compose| compose.model.clone())
        .or_else(|| Some(config.model.clone()));
    let choices = config
        .assistant_model_ids()
        .iter()
        .map(|id| judge_compose(&config, &keys, id))
        .collect();
    AssistantModelsData { current, choices }
}

fn judge_compose(
    config: &LlmConfig,
    keys: &dyn ApiKeySource,
    id: &str,
) -> AssistantModelChoiceData {
    let effective = config.effective_override(&LlmOverrideConfig {
        model: Some(id.to_string()),
        ..LlmOverrideConfig::default()
    });
    let routes = llm_model_routes(&effective);
    let cloud = routes.is_some() || !mxr_llm::is_loopback_endpoint(&effective.base_url);
    let unavailable_reason = if mxr_config::is_demo_instance() {
        Some("The demo profile runs a built-in script, not a model.".to_string())
    } else if !effective.enabled {
        Some("AI features are off. Turn them on: mxr config set llm.enabled true".to_string())
    } else {
        routes
            .as_deref()
            .and_then(|routes| {
                let first = routes.first()?;
                routes
                    .iter()
                    .all(|route| keys.api_key(route.provider).is_none())
                    .then_some(first.provider)
            })
            .map(|missing| {
                format!(
                    "Add an {} key: mxr llm key set {}",
                    missing.label(),
                    missing.key()
                )
            })
    };
    AssistantModelChoiceData {
        id: id.to_string(),
        label: label(id),
        cloud,
        sees: if cloud && !config.allow_cloud_relationship_data {
            "this conversation only".to_string()
        } else {
            "this conversation and your past mail".to_string()
        },
        available: unavailable_reason.is_none(),
        unavailable_reason,
    }
}

/// The refusal for `model` against `models`, or `None` when it can be set.
fn refusal(models: &AssistantModelsData, model: &str) -> Option<String> {
    let Some(choice) = models.choices.iter().find(|choice| choice.id == model) else {
        let listed: Vec<&str> = models.choices.iter().map(|c| c.id.as_str()).collect();
        return Some(format!(
            "`{model}` isn't one of the assistant's models. Choose one of: {}",
            listed.join(", ")
        ));
    };
    choice
        .unavailable_reason
        .as_ref()
        .map(|reason| format!("{} can't be used yet. {reason}", choice.label))
}

/// Set the assistant's model, or with `dry_run` say what would change.
/// Refuses a model outside the list, and one that can't run now, with what
/// to do about it. The write re-checks against the config it changes, under
/// the lock every config writer takes, so a cloud model can't slip in after
/// the cloud setting was turned off.
pub(crate) async fn set_model(
    state: &AppState,
    model: &str,
    dry_run: bool,
) -> Result<ResponseData, String> {
    let model = model.trim();
    let keys = LlmKeys::at_default_path();
    let before = models_for(&state.config_snapshot().llm, &keys);
    if let Some(refusal) = refusal(&before, model) {
        return Err(refusal);
    }
    let changed = before.current.as_deref() != Some(model);
    let models = if dry_run || !changed {
        AssistantModelsData {
            current: Some(model.to_string()),
            choices: before.choices.clone(),
        }
    } else {
        let id = model.to_string();
        let restore_pending = state.is_restore_pending();
        let config = state
            .try_mutate_config(|config| {
                // The file as it is now. A restored workspace keeps AI off
                // in the daemon whatever the file says, so judge it that way.
                let mut judged = config.llm.clone();
                if restore_pending {
                    judged.enabled = false;
                    judged.overrides = Default::default();
                }
                if let Some(refusal) = refusal(&models_for(&judged, &keys), &id) {
                    return Err(refusal);
                }
                config
                    .llm
                    .overrides
                    .assistant
                    .get_or_insert_with(LlmOverrideConfig::default)
                    .model = Some(id);
                Ok(())
            })
            .await?;
        tracing::info!(model, "assistant model set");
        models_for(&config.llm, &keys)
    };
    Ok(ResponseData::AssistantModelChange {
        change: AssistantModelChangeData {
            dry_run,
            changed,
            previous: before.current,
            models,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_llm::{ApiKey, RouteProvider};
    use std::collections::HashSet;

    struct Keys(HashSet<RouteProvider>);

    impl ApiKeySource for Keys {
        fn api_key(&self, provider: RouteProvider) -> Option<ApiKey> {
            self.0.contains(&provider).then(|| ApiKey::new("test-key"))
        }
    }

    fn keys(providers: &[RouteProvider]) -> Keys {
        Keys(providers.iter().copied().collect())
    }

    fn config(model: &str) -> LlmConfig {
        LlmConfig {
            enabled: true,
            model: model.into(),
            allow_cloud_relationship_data: true,
            ..LlmConfig::default()
        }
    }

    fn choice<'a>(models: &'a AssistantModelsData, id: &str) -> &'a AssistantModelChoiceData {
        models
            .choices
            .iter()
            .find(|choice| choice.id == id)
            .unwrap()
    }

    #[test]
    fn a_config_with_no_list_offers_the_five_defaults_in_order() {
        let config: LlmConfig = toml::from_str("enabled = true").unwrap();
        let models = models_for(&config, &keys(&[]));
        let ids: Vec<&str> = models.choices.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemma4:latest",
                "gpt-6-luna",
                "gpt-5.6-terra",
                "claude-haiku-5-5",
                "claude-sonnet-5-5"
            ]
        );
        let labels: Vec<&str> = models.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Gemma 4",
                "GPT-6 Luna",
                "GPT-5.6 Terra",
                "Claude Haiku 5.5",
                "Claude Sonnet 5.5"
            ]
        );
    }

    #[test]
    fn an_empty_or_blank_list_means_the_defaults_and_repeats_collapse() {
        let mut config = config("gemma4");
        config.models = vec![];
        assert_eq!(config.assistant_model_ids().len(), 5);
        config.models = vec!["  ".into(), "gemma4:latest".into(), "gemma4:latest".into()];
        assert_eq!(config.assistant_model_ids(), ["gemma4:latest"]);
    }

    #[test]
    fn the_default_list_is_not_written_back_to_config() {
        let toml = toml::to_string(&LlmConfig::default()).unwrap();
        assert!(!toml.contains("models"), "{toml}");
        let custom = LlmConfig {
            models: vec!["gemma4:latest".into()],
            ..LlmConfig::default()
        };
        assert!(toml::to_string(&custom).unwrap().contains("models"));
    }

    #[test]
    fn a_model_without_its_key_is_unavailable_with_the_command_to_add_one() {
        let models = models_for(&config("gemma4"), &keys(&[]));
        assert!(choice(&models, "gemma4:latest").available);
        assert_eq!(
            choice(&models, "gpt-6-luna").unavailable_reason.as_deref(),
            Some("Add an OpenAI key: mxr llm key set openai")
        );
        assert_eq!(
            choice(&models, "claude-haiku-5-5")
                .unavailable_reason
                .as_deref(),
            Some("Add an Anthropic key: mxr llm key set anthropic")
        );
        assert!(models
            .choices
            .iter()
            .all(|c| c.available == c.unavailable_reason.is_none()));
    }

    #[test]
    fn one_key_makes_only_its_models_available() {
        let models = models_for(&config("gemma4"), &keys(&[RouteProvider::OpenAI]));
        assert!(choice(&models, "gpt-6-luna").available);
        assert!(choice(&models, "gpt-5.6-terra").available);
        assert!(!choice(&models, "claude-sonnet-5-5").available);
    }

    #[test]
    fn an_openrouter_key_alone_serves_every_hosted_model() {
        let models = models_for(&config("gemma4"), &keys(&[RouteProvider::OpenRouter]));
        assert!(models.choices.iter().all(|c| c.available));
    }

    #[test]
    fn cloud_is_true_for_hosted_models_and_false_for_the_local_one() {
        let models = models_for(&config("gemma4"), &keys(&[]));
        assert!(!choice(&models, "gemma4:latest").cloud);
        for id in [
            "gpt-6-luna",
            "gpt-5.6-terra",
            "claude-haiku-5-5",
            "claude-sonnet-5-5",
        ] {
            assert!(choice(&models, id).cloud, "{id}");
        }
    }

    #[test]
    fn compose_catalogue_uses_compose_route_and_allows_conversation_only_cloud() {
        let mut config = config("gemma4:latest");
        config.allow_cloud_relationship_data = false;
        config.overrides.assistant = Some(LlmOverrideConfig {
            enabled: Some(false),
            base_url: Some("http://127.0.0.1:11434/v1".into()),
            ..LlmOverrideConfig::default()
        });
        let choices = config.assistant_model_ids();
        let assistant = models_for(&config, &keys(&[RouteProvider::OpenAI]));
        let compose = choices
            .iter()
            .map(|id| judge_compose(&config, &keys(&[RouteProvider::OpenAI]), id))
            .collect::<Vec<_>>();
        assert!(!choice(&assistant, "gpt-6-luna").available);
        let gpt = compose
            .iter()
            .find(|choice| choice.id == "gpt-6-luna")
            .unwrap();
        assert!(gpt.available);
        assert!(gpt.cloud);
        assert_eq!(gpt.sees, "this conversation only");
    }

    #[test]
    fn with_the_cloud_gate_off_hosted_models_name_the_setting_to_turn_on() {
        let mut config = config("gemma4");
        config.allow_cloud_relationship_data = false;
        let all_keys = keys(&[
            RouteProvider::Anthropic,
            RouteProvider::OpenAI,
            RouteProvider::OpenRouter,
        ]);
        let models = models_for(&config, &all_keys);
        assert!(choice(&models, "gemma4:latest").available);
        for id in ["gpt-6-luna", "claude-sonnet-5-5"] {
            let reason = choice(&models, id).unavailable_reason.clone().unwrap();
            assert!(
                reason.contains("llm.allow_cloud_relationship_data"),
                "{reason}"
            );
        }
    }

    #[test]
    fn with_ai_off_every_choice_says_how_to_turn_it_on() {
        let mut config = config("gemma4");
        config.enabled = false;
        let models = models_for(&config, &keys(&[RouteProvider::OpenAI]));
        assert!(models.choices.iter().all(|c| !c.available
            && c.unavailable_reason
                .as_deref()
                .is_some_and(|r| r.contains("llm.enabled"))));
    }

    #[test]
    fn an_id_naming_an_unknown_vendor_is_listed_not_fatal() {
        let mut config = config("gemma4");
        config.models = vec!["mystery-model-9".into(), "openai/gpt-4o".into()];
        let models = models_for(&config, &keys(&[]));
        let mystery = choice(&models, "mystery-model-9");
        assert_eq!(mystery.label, "mystery-model-9");
        // Not routed: it goes to the local endpoint like any other name.
        assert!(!mystery.cloud && mystery.available);
        // A vendor/model slug is OpenRouter's, so it needs that key.
        assert_eq!(
            choice(&models, "openai/gpt-4o")
                .unavailable_reason
                .as_deref(),
            Some("Add an OpenRouter key: mxr llm key set openrouter")
        );
    }

    #[test]
    fn the_current_model_is_marked_even_when_config_spells_it_differently() {
        let models = models_for(&config("gemma4"), &keys(&[]));
        assert_eq!(models.current.as_deref(), Some("gemma4:latest"));
        let mut hand_set = config("gemma4");
        hand_set.overrides.assistant = Some(LlmOverrideConfig {
            model: Some("my-own-model".into()),
            ..LlmOverrideConfig::default()
        });
        let models = models_for(&hand_set, &keys(&[]));
        assert_eq!(models.current.as_deref(), Some("my-own-model"));
    }

    #[test]
    fn the_assistant_override_decides_the_current_model_not_the_base() {
        let mut config = config("claude-haiku-5-5");
        config.overrides.assistant = Some(LlmOverrideConfig {
            model: Some("gpt-6-luna".into()),
            ..LlmOverrideConfig::default()
        });
        let models = models_for(&config, &keys(&[]));
        assert_eq!(models.current.as_deref(), Some("gpt-6-luna"));
    }
}
