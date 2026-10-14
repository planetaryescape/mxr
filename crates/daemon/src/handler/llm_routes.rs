//! LLM provider keys and routes: storing and removing keys, applying `[llm]`
//! edits made outside the daemon, and reporting each feature's model, route
//! order and route health.
//!
//! Key values never leave this module in a response, a log line or an error.

use super::HandlerResult;
use crate::llm_keys::{clean_key, provider_data, provider_from_data, LlmKeys};
use crate::state::{effective_llm_config, llm_model_routes, AppState};
use mxr_llm::{LlmFeature, RouteProvider, RouteState};
use mxr_protocol::{
    LlmFeatureStatusData, LlmKeyChangeData, LlmKeySecret, LlmKeySourceData, LlmKeyStatusData,
    LlmProviderData, LlmRouteStateData, LlmRouteStatusData, ResponseData,
};

pub(super) async fn reload_llm_config(state: &AppState) -> HandlerResult {
    state.reload_llm_config_from_disk().await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn set_llm_key(
    state: &AppState,
    provider: LlmProviderData,
    key: &LlmKeySecret,
    dry_run: bool,
) -> HandlerResult {
    let key = clean_key(key.expose())?;
    let provider = provider_from_data(provider);
    if dry_run {
        let keys = LlmKeys::at_default_path();
        let after = LlmKeyStatusData {
            provider: provider_data(provider),
            present: true,
            source: Some(LlmKeySourceData::Stored),
            location: Some(keys.path().display().to_string()),
            env_var: provider.env_var().to_string(),
        };
        return Ok(change(provider, true, true, after));
    }
    // fsync-ing the secrets file blocks.
    let after = tokio::task::spawn_blocking(move || {
        let keys = LlmKeys::at_default_path();
        keys.store(provider, &key)
            .map_err(|error| format!("couldn't store the {} key: {error}", provider.label()))?;
        Ok::<_, String>(keys.status(provider))
    })
    .await
    .map_err(|error| error.to_string())??;
    // A verdict reached with the old key says nothing about the new one.
    state.llm.route_health().forget_provider(provider);
    tracing::info!(provider = provider.key(), "LLM provider key stored");
    Ok(change(provider, false, true, after))
}

pub(super) async fn remove_llm_key(
    state: &AppState,
    provider: LlmProviderData,
    dry_run: bool,
) -> HandlerResult {
    let provider = provider_from_data(provider);
    if dry_run {
        let stored = tokio::task::spawn_blocking(move || {
            LlmKeys::at_default_path().status(provider).source == Some(LlmKeySourceData::Stored)
        })
        .await
        .map_err(|error| error.to_string())?;
        let after = LlmKeys::status_without_stored(provider);
        return Ok(change(provider, true, stored, after));
    }
    let (removed, after) = tokio::task::spawn_blocking(move || {
        let keys = LlmKeys::at_default_path();
        let removed = keys
            .remove(provider)
            .map_err(|error| format!("couldn't remove the {} key: {error}", provider.label()))?;
        Ok::<_, String>((removed, keys.status(provider)))
    })
    .await
    .map_err(|error| error.to_string())??;
    state.llm.route_health().forget_provider(provider);
    if removed {
        tracing::info!(provider = provider.key(), "LLM provider key removed");
    }
    Ok(change(provider, false, removed, after))
}

fn change(
    provider: RouteProvider,
    dry_run: bool,
    changed: bool,
    after: LlmKeyStatusData,
) -> ResponseData {
    ResponseData::LlmKeyChange {
        change: LlmKeyChangeData {
            provider: provider_data(provider),
            dry_run,
            changed,
            after,
        },
    }
}

/// Whether the base `[llm]` model is one mxr routes itself.
pub(crate) fn base_model_routed(config: &mxr_config::LlmConfig) -> bool {
    llm_model_routes(&crate::state::base_llm_config(config)).is_some()
}

/// Key presence for each provider, never the key.
pub(crate) fn key_statuses() -> Vec<LlmKeyStatusData> {
    let keys = LlmKeys::at_default_path();
    RouteProvider::ALL
        .into_iter()
        .map(|provider| keys.status(provider))
        .collect()
}

/// One row per feature: its model and, for a routed model, each route's
/// standing, in the order requests try them.
pub(crate) fn feature_statuses(state: &AppState) -> Vec<LlmFeatureStatusData> {
    let config = state.config_snapshot().llm;
    let keys = LlmKeys::at_default_path();
    let demo = mxr_config::is_demo_instance();
    let now = chrono::Utc::now();
    LlmFeature::ALL
        .into_iter()
        .map(|feature| {
            let effective = effective_llm_config(&config, feature);
            let routes = (effective.enabled && !demo)
                .then(|| llm_model_routes(&effective))
                .flatten();
            let routed = routes.is_some();
            let cloud = effective.enabled
                && !demo
                && (routed || !mxr_llm::is_loopback_endpoint(&effective.base_url));
            let statuses = routes.map_or_else(Vec::new, |routes| {
                mxr_llm::route_statuses(&routes, &keys, state.llm.route_health())
                    .into_iter()
                    .map(|status| route_status_data(&status, now))
                    .collect()
            });
            LlmFeatureStatusData {
                feature: feature.key().to_string(),
                enabled: effective.enabled,
                model: effective.model.trim().to_string(),
                routed,
                cloud,
                base_url: (effective.enabled && !routed && !demo).then_some(effective.base_url),
                routes: statuses,
                blocked: state.llm.feature_block_reason(feature),
            }
        })
        .collect()
}

fn route_status_data(
    status: &mxr_llm::RouteStatus,
    now: chrono::DateTime<chrono::Utc>,
) -> LlmRouteStatusData {
    let state = if !status.key_present {
        LlmRouteStateData::NoKey
    } else {
        match status.view.state {
            RouteState::Ok => LlmRouteStateData::Ok,
            RouteState::Cooling => LlmRouteStateData::Cooling,
            RouteState::Broken => LlmRouteStateData::Broken,
            RouteState::Recovering => LlmRouteStateData::Recovering,
            RouteState::Probing => LlmRouteStateData::Probing,
        }
    };
    let waiting = matches!(status.view.state, RouteState::Cooling | RouteState::Broken);
    LlmRouteStatusData {
        provider: provider_data(status.route.provider),
        model: status.route.model.clone(),
        state,
        retry_at: (status.key_present && waiting)
            .then(|| now + chrono::Duration::from_std(status.view.retry_in).unwrap_or_default()),
        reason: status
            .key_present
            .then(|| status.view.reason.map(|reason| reason.to_string()))
            .flatten(),
        next: status.next,
    }
}
