//! Claude models routed through Anthropic and OpenRouter: what the daemon
//! says about them, who may send what to them, and how keys and `[llm]`
//! edits reach a running daemon.

use super::*;
use crate::state::{llm_endpoint_is_local, relationship_data_allowed};
use mxr_llm::LlmFeature;
use mxr_protocol::{
    LlmKeySecret, LlmKeySourceData, LlmProviderData, LlmRouteStateData, LlmStatusSnapshot,
};

const SECRET: &str = "sk-ant-test-secret-do-not-leak";

pub(super) async fn send(state: &Arc<AppState>, request: Request) -> Response {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    };
    match handle_request(state, &msg).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

async fn status(state: &Arc<AppState>) -> LlmStatusSnapshot {
    match send(state, Request::GetLlmStatus).await {
        Response::Ok {
            data: ResponseData::LlmStatus { snapshot },
        } => snapshot,
        other => panic!("expected status, got {other:?}"),
    }
}

/// Runs `body` with config, data and secrets in a temp dir and no provider
/// keys in the environment.
pub(super) fn isolated<F, Fut>(body: F)
where
    F: FnOnce(Arc<AppState>) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let dir = tempfile::tempdir().expect("temp dir");
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    temp_env::with_vars(
        [
            ("MXR_CONFIG_DIR", Some(config_dir)),
            ("MXR_DATA_DIR", Some(dir.path().join("data"))),
            ("MXR_SECRETS_PATH", Some(dir.path().join("secrets.toml"))),
            ("ANTHROPIC_API_KEY", None),
            ("OPENAI_API_KEY", None),
            ("OPENROUTER_API_KEY", None),
        ],
        || {
            runtime.block_on(async {
                mxr_config::save_config(&mxr_config::MxrConfig::default()).expect("save config");
                let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
                body(state).await;
            });
        },
    );
}

pub(super) async fn use_model(state: &Arc<AppState>, model: &str, base_url: &str) {
    let mut config = state.config_snapshot();
    config.llm.enabled = true;
    config.llm.model = model.to_string();
    config.llm.base_url = base_url.to_string();
    state.set_config_for_test(config).await;
}

pub(super) const LOCAL: &str = "http://localhost:11434/v1";

#[test]
fn a_routed_model_counts_as_cloud_for_every_gate() {
    isolated(|state| async move {
        use_model(&state, "claude-haiku-5-5", LOCAL).await;
        let pinned = state.llm.for_feature(LlmFeature::DraftNew).pin();
        // No URL to inspect, and still not on this machine.
        assert_eq!(pinned.base_url(), None);
        assert!(pinned.leaves_machine());
        assert!(!llm_endpoint_is_local(&pinned));

        let mut config = state.config_snapshot().llm;
        assert!(!relationship_data_allowed(&config, &pinned));
        config.allow_cloud_relationship_data = true;
        assert!(relationship_data_allowed(&config, &pinned));

        // Relationship features are blocked outright without the opt-in,
        // however local `base_url` looks.
        let reason = state
            .llm
            .feature_block_reason(LlmFeature::RelationshipSummary)
            .expect("blocked without allow_cloud_relationship_data");
        assert!(reason.contains("non-local"), "{reason}");
        let mut config = state.config_snapshot();
        config.llm.allow_cloud_relationship_data = true;
        state.set_config_for_test(config).await;
        assert_eq!(
            state
                .llm
                .feature_block_reason(LlmFeature::RelationshipSummary),
            None
        );
    });
}

#[test]
fn a_local_model_is_still_local_and_unblocked() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let pinned = state.llm.for_feature(LlmFeature::DraftNew).pin();
        assert!(!pinned.leaves_machine());
        assert!(llm_endpoint_is_local(&pinned));
        assert_eq!(
            state
                .llm
                .feature_block_reason(LlmFeature::RelationshipSummary),
            None
        );
        let snapshot = status(&state).await;
        assert_eq!(snapshot.provider, "openai_compatible");
        assert_eq!(snapshot.base_url.as_deref(), Some(LOCAL));
        assert!(snapshot
            .features
            .iter()
            .all(|row| !row.routed && !row.cloud));
        assert!(snapshot.features.iter().all(|row| row.routes.is_empty()));
    });
}

#[test]
fn a_per_feature_override_routes_on_its_own() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let mut config = state.config_snapshot();
        config.llm.overrides.briefing = Some(mxr_config::LlmOverrideConfig {
            model: Some("claude-opus-5-5".into()),
            ..Default::default()
        });
        state.set_config_for_test(config).await;

        let snapshot = status(&state).await;
        let row = |name: &str| {
            snapshot
                .features
                .iter()
                .find(|row| row.feature == name)
                .unwrap_or_else(|| panic!("no {name} row"))
        };
        assert!(row("briefing").routed && row("briefing").cloud);
        assert_eq!(row("briefing").model, "claude-opus-5-5");
        assert_eq!(row("briefing").routes[1].model, "anthropic/claude-opus-5.5");
        assert!(!row("summarize").routed && !row("summarize").cloud);
        // The override's cloud model can't see relationship data without the
        // opt-in, while the local base can.
        assert!(state
            .llm
            .feature_block_reason(LlmFeature::Briefing)
            .is_some());
        assert_eq!(
            state.llm.feature_block_reason(LlmFeature::Commitments),
            None
        );
    });
}

#[test]
fn status_lists_every_feature_with_its_routes_and_which_has_a_key() {
    isolated(|state| async move {
        use_model(&state, "claude-haiku-5-5", LOCAL).await;
        let snapshot = status(&state).await;
        assert_eq!(snapshot.provider, "routed");
        assert_eq!(snapshot.model, "claude-haiku-5-5");
        assert_eq!(snapshot.base_url, None);
        assert_eq!(snapshot.features.len(), LlmFeature::ALL.len());
        let row = &snapshot.features[0];
        assert_eq!(row.feature, "summarize");
        assert!(row.routed && row.cloud);
        let providers: Vec<_> = row.routes.iter().map(|r| r.provider).collect();
        assert_eq!(
            providers,
            [LlmProviderData::Anthropic, LlmProviderData::Openrouter]
        );
        // Every hosted provider has a key row, so `mxr llm key status` lists OpenAI.
        let listed: Vec<_> = snapshot.keys.iter().map(|key| key.provider).collect();
        assert_eq!(
            listed,
            [
                LlmProviderData::Anthropic,
                LlmProviderData::Openai,
                LlmProviderData::Openrouter
            ]
        );
        assert_eq!(row.routes[0].model, "claude-haiku-5-5");
        assert_eq!(row.routes[1].model, "anthropic/claude-haiku-5.5");
        // No keys: nothing to try.
        assert!(row
            .routes
            .iter()
            .all(|route| route.state == LlmRouteStateData::NoKey && !route.next));
        assert!(!snapshot.api_key_present);
        assert!(snapshot.keys.iter().all(|key| !key.present));

        // An OpenRouter key alone: that is where requests go.
        send(
            &state,
            Request::SetLlmKey {
                provider: LlmProviderData::Openrouter,
                key: LlmKeySecret::new("sk-or-test".into()),
                dry_run: false,
            },
        )
        .await;
        let snapshot = status(&state).await;
        let row = &snapshot.features[0];
        assert_eq!(row.routes[0].state, LlmRouteStateData::NoKey);
        assert_eq!(row.routes[1].state, LlmRouteStateData::Ok);
        assert!(row.routes[1].next);
        assert!(snapshot.api_key_present);
    });
}

#[test]
fn route_health_shows_up_in_status_with_a_reason_and_a_retry_time() {
    isolated(|state| async move {
        use_model(&state, "claude-haiku-5-5", LOCAL).await;
        send(
            &state,
            Request::SetLlmKey {
                provider: LlmProviderData::Anthropic,
                key: LlmKeySecret::new(SECRET.into()),
                dry_run: false,
            },
        )
        .await;
        let route = mxr_llm::routes_for_model("claude-haiku-5-5", mxr_llm::Endpoint::Default)
            .unwrap()
            .remove(0);
        state
            .llm
            .route_health()
            .record_http_failure_for_test(&route, 429);
        let snapshot = status(&state).await;
        let anthropic = &snapshot.features[0].routes[0];
        assert_eq!(anthropic.state, LlmRouteStateData::Cooling);
        assert_eq!(anthropic.reason.as_deref(), Some("429 rate limited"));
        assert!(anthropic.retry_at.is_some());
        assert!(!anthropic.next);

        // A new key for the provider clears the verdict.
        send(
            &state,
            Request::SetLlmKey {
                provider: LlmProviderData::Anthropic,
                key: LlmKeySecret::new("sk-ant-replacement".into()),
                dry_run: false,
            },
        )
        .await;
        let snapshot = status(&state).await;
        assert_eq!(snapshot.features[0].routes[0].state, LlmRouteStateData::Ok);
    });
}

#[test]
fn a_key_is_stored_checked_removed_and_never_echoed() {
    isolated(|state| async move {
        let set = |dry_run| Request::SetLlmKey {
            provider: LlmProviderData::Anthropic,
            key: LlmKeySecret::new(format!("  {SECRET}\n")),
            dry_run,
        };

        // A dry run says what would happen and stores nothing.
        let response = send(&state, set(true)).await;
        let text = format!("{response:?}");
        assert!(!text.contains(SECRET), "{text}");
        let Response::Ok {
            data: ResponseData::LlmKeyChange { change },
        } = response
        else {
            panic!("expected a key change");
        };
        assert!(change.dry_run && change.changed);
        assert!(!status(&state).await.keys[0].present);

        // Stored for real; the response and status carry presence only.
        let response = send(&state, set(false)).await;
        assert!(!format!("{response:?}").contains(SECRET));
        let snapshot = status(&state).await;
        let anthropic = &snapshot.keys[0];
        assert!(anthropic.present);
        assert_eq!(anthropic.source, Some(LlmKeySourceData::Stored));
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains(SECRET), "{json}");
        let on_disk = std::fs::read_to_string(std::env::var("MXR_SECRETS_PATH").unwrap()).unwrap();
        assert!(on_disk.contains(SECRET), "stored trimmed in secrets.toml");
        let config_toml = std::fs::read_to_string(mxr_config::config_file_path()).unwrap();
        assert!(!config_toml.contains(SECRET), "never in config.toml");

        // Removal previews, then happens.
        let remove = |dry_run| Request::RemoveLlmKey {
            provider: LlmProviderData::Anthropic,
            dry_run,
        };
        send(&state, remove(true)).await;
        assert!(
            status(&state).await.keys[0].present,
            "dry run removed nothing"
        );
        let Response::Ok {
            data: ResponseData::LlmKeyChange { change },
        } = send(&state, remove(false)).await
        else {
            panic!("expected a key change");
        };
        assert!(change.changed && !change.after.present);
        assert!(!status(&state).await.keys[0].present);
        // Nothing left to remove.
        let Response::Ok {
            data: ResponseData::LlmKeyChange { change },
        } = send(&state, remove(false)).await
        else {
            panic!("expected a key change");
        };
        assert!(!change.changed);
    });
}

#[test]
fn a_bad_key_is_refused_without_echoing_it() {
    isolated(|state| async move {
        for bad in ["", "   ", "two words", "line\nbreak"] {
            let response = send(
                &state,
                Request::SetLlmKey {
                    provider: LlmProviderData::Anthropic,
                    key: LlmKeySecret::new(bad.into()),
                    dry_run: false,
                },
            )
            .await;
            let Response::Error { message, .. } = response else {
                panic!("{bad:?} should be refused");
            };
            assert!(bad.trim().is_empty() || !message.contains(bad), "{message}");
        }
        assert!(!status(&state).await.keys[0].present);
    });
}

#[test]
fn llm_edits_made_outside_the_daemon_apply_without_a_restart() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        assert_eq!(status(&state).await.provider, "openai_compatible");

        // What `mxr config set llm.model ...` does: write the file.
        let mut on_disk = mxr_config::load_config().unwrap();
        on_disk.llm.enabled = true;
        on_disk.llm.model = "claude-haiku-5-5".into();
        on_disk.llm.allow_cloud_relationship_data = true;
        mxr_config::save_config(&on_disk).unwrap();
        // The daemon still has the old one.
        assert_eq!(status(&state).await.model, "gemma4");

        let response = send(&state, Request::ReloadLlmConfig).await;
        assert!(matches!(response, Response::Ok { .. }), "{response:?}");
        let snapshot = status(&state).await;
        assert_eq!(snapshot.model, "claude-haiku-5-5");
        assert_eq!(snapshot.provider, "routed");
        assert!(snapshot.allow_cloud_relationship_data);
        // The in-memory config follows, so the next config mutation does not
        // write the old model back over the file.
        assert_eq!(state.config_snapshot().llm.model, "claude-haiku-5-5");
    });
}

#[test]
fn a_reload_forgets_what_the_old_setup_taught_about_a_route() {
    isolated(|state| async move {
        use_model(&state, "claude-haiku-5-5", LOCAL).await;
        send(
            &state,
            Request::SetLlmKey {
                provider: LlmProviderData::Anthropic,
                key: LlmKeySecret::new(SECRET.into()),
                dry_run: false,
            },
        )
        .await;
        let route = mxr_llm::routes_for_model("claude-haiku-5-5", mxr_llm::Endpoint::Default)
            .unwrap()
            .remove(0);
        state
            .llm
            .route_health()
            .record_http_failure_for_test(&route, 404);
        assert_eq!(
            status(&state).await.features[0].routes[0].state,
            LlmRouteStateData::Broken
        );

        // The file says the same thing; reloading still starts the routes over.
        let mut on_disk = mxr_config::load_config().unwrap();
        on_disk.llm.enabled = true;
        on_disk.llm.model = "claude-haiku-5-5".into();
        mxr_config::save_config(&on_disk).unwrap();
        send(&state, Request::ReloadLlmConfig).await;
        assert_eq!(
            status(&state).await.features[0].routes[0].state,
            LlmRouteStateData::Ok
        );
    });
}
