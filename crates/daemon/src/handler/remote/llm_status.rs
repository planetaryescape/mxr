//! Keep host configuration private when reporting model availability to remote clients.
pub(super) fn project(
    snapshot: &mut mxr_protocol::LlmStatusSnapshot,
    allow_ai: bool,
    policy_key: String,
) {
    snapshot.policy_key = Some(policy_key);
    snapshot.base_url = None;
    snapshot.api_key_env = None;
    snapshot.keys.clear();
    if !allow_ai {
        snapshot.enabled = false;
        snapshot.provider = "disabled".into();
        snapshot.model.clear();
        snapshot.configured_model.clear();
        snapshot.api_key_present = false;
        snapshot.supports_streaming = false;
        snapshot.observations.clear();
        snapshot.features.clear();
        snapshot.fallback_model = None;
        snapshot.policy_key = None;
        snapshot.fallback_error = None;
        snapshot.background_paused_until = None;
        return;
    }
    if snapshot.fallback_error.is_some() {
        snapshot.fallback_error = Some("Fallback is unavailable; inspect it on the host".into());
    }
    for feature in &mut snapshot.features {
        feature.base_url = None;
        if let Some(endpoint) = &mut feature.endpoint {
            endpoint.base_url.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn configured() -> mxr_protocol::LlmStatusSnapshot {
        serde_json::from_value(serde_json::json!({
            "enabled": true, "provider": "openai_compatible", "model": "visible-model",
            "configured_model": "visible-model", "base_url": "https://PRIVATE_ENDPOINT_SENTINEL.invalid/key",
            "api_key_env": "PRIVATE_ENV_SENTINEL", "api_key_present": true,
            "context_window": 8192, "supports_streaming": true, "request_timeout_secs": 30,
            "keys": [{"provider":"openrouter", "present":true, "source":"stored",
                "location":"/private/PRIVATE_FILE_SENTINEL", "env_var":"PRIVATE_ENV_SENTINEL"}],
            "fallback_error": "PRIVATE_ERROR_SENTINEL",
            "features": [{"feature":"summarize", "enabled":true, "model":"visible-model",
                "routed":false, "cloud":false, "base_url":"https://PRIVATE_ENDPOINT_SENTINEL.invalid/key",
                "endpoint":{"base_url":"https://PRIVATE_ENDPOINT_SENTINEL.invalid/key", "model":"visible-model",
                    "state":"cooling", "reason":"unreachable", "next":false}}],
        })).unwrap()
    }
    #[test]
    fn remote_health_preserves_models_and_outcomes_without_host_credential_locations() {
        let mut status = configured();
        project(&mut status, true, "boot:1".into());
        assert!(status.enabled);
        assert_eq!(status.model, "visible-model");
        assert_eq!(
            status.features[0].endpoint.as_ref().unwrap().state,
            mxr_protocol::LlmRouteStateData::Cooling
        );
        assert!(!serde_json::to_string(&status).unwrap().contains("SENTINEL"));
        assert!(status.keys.is_empty());
    }
    #[test]
    fn a_read_only_installation_cannot_advertise_ai_as_enabled() {
        let mut status = configured();
        project(&mut status, false, "boot:1".into());
        assert!(!status.enabled);
        assert!(!status.api_key_present);
        assert!(status.features.is_empty());
        assert!(!serde_json::to_string(&status).unwrap().contains("SENTINEL"));
    }

    #[test]
    fn changing_a_redacted_endpoint_still_invalidates_cached_model_output() {
        let runtime = mxr_llm::LlmRuntime::new(std::sync::Arc::new(mxr_llm::NoopProvider));
        let mut first = configured();
        let mut second = configured();
        second.base_url = Some("https://OTHER_PRIVATE_ENDPOINT_SENTINEL.invalid".into());
        project(
            &mut first,
            true,
            format!("boot:{}", runtime.policy_generation()),
        );
        runtime.replace(std::sync::Arc::new(mxr_llm::NoopProvider));
        project(
            &mut second,
            true,
            format!("boot:{}", runtime.policy_generation()),
        );
        assert!(first.base_url.is_none() && second.base_url.is_none());
        assert!(first.policy_key.is_some());
        assert_ne!(first.policy_key, second.policy_key);
    }
}
