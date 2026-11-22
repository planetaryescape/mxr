use super::llm_routes::{isolated, send, LOCAL};
use super::*;
use mxr_protocol::{
    AssistantAvailabilityData, AssistantModelChangeData, AssistantModelsData, LlmKeySecret,
    LlmProviderData,
};

async fn save(state: &Arc<AppState>, config: mxr_config::MxrConfig) {
    mxr_config::save_config(&config).unwrap();
    state.set_config_for_test(config).await;
}

async fn use_model(state: &Arc<AppState>, model: &str, base_url: &str) {
    let mut config = state.config_snapshot();
    config.llm.enabled = true;
    config.llm.model = model.to_string();
    config.llm.base_url = base_url.to_string();
    save(state, config).await;
}

const SECRET: &str = "sk-test-secret-that-must-never-leave-the-daemon";

async fn models(state: &Arc<AppState>) -> AssistantModelsData {
    match send(state, Request::GetAssistantModels).await {
        Response::Ok {
            data: ResponseData::AssistantModels { models },
        } => models,
        other => panic!("expected the models, got {other:?}"),
    }
}

async fn availability(state: &Arc<AppState>) -> AssistantAvailabilityData {
    match send(state, Request::GetAssistantAvailability).await {
        Response::Ok {
            data: ResponseData::AssistantAvailability { availability },
        } => availability,
        other => panic!("expected availability, got {other:?}"),
    }
}

async fn pick(state: &Arc<AppState>, model: &str, dry_run: bool) -> Response {
    send(
        state,
        Request::SetAssistantModel {
            model: model.into(),
            dry_run,
        },
    )
    .await
}

async fn picked(state: &Arc<AppState>, model: &str) -> AssistantModelChangeData {
    match pick(state, model, false).await {
        Response::Ok {
            data: ResponseData::AssistantModelChange { change },
        } => change,
        other => panic!("expected {model} to be set, got {other:?}"),
    }
}

fn refusal(response: Response) -> String {
    match response {
        Response::Error { message, .. } => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

async fn store_key(state: &Arc<AppState>, provider: LlmProviderData) {
    send(
        state,
        Request::SetLlmKey {
            provider,
            key: LlmKeySecret::new(SECRET.into()),
            dry_run: false,
        },
    )
    .await;
}

fn assistant_model_on_disk() -> Option<String> {
    mxr_config::load_config()
        .unwrap()
        .llm
        .overrides
        .assistant
        .and_then(|assistant| assistant.model)
}

async fn allow_cloud(state: &Arc<AppState>) {
    let mut config = state.config_snapshot();
    config.llm.allow_cloud_relationship_data = true;
    save(state, config).await;
}

#[test]
fn the_list_has_the_five_defaults_and_says_what_each_needs() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let listed = models(&state).await;

        let ids: Vec<&str> = listed.choices.iter().map(|c| c.id.as_str()).collect();
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
        assert_eq!(listed.current.as_deref(), Some("gemma4:latest"));
        let by_id = |id: &str| listed.choices.iter().find(|c| c.id == id).unwrap();
        assert!(by_id("gemma4:latest").available && !by_id("gemma4:latest").cloud);
        assert_eq!(
            by_id("gpt-6-luna").unavailable_reason.as_deref(),
            Some("Add an OpenAI key: mxr llm key set openai")
        );
        assert_eq!(
            by_id("claude-sonnet-5-5").unavailable_reason.as_deref(),
            Some("Add an Anthropic key: mxr llm key set anthropic")
        );
    });
}

#[test]
fn a_model_outside_the_list_is_refused_and_nothing_changes() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let message = refusal(pick(&state, "gpt-9-imaginary", false).await);
        assert!(
            message.contains("isn't one of the assistant's models"),
            "{message}"
        );
        assert!(message.contains("gemma4:latest"), "{message}");
        assert_eq!(assistant_model_on_disk(), None);
        assert_eq!(availability(&state).await.model.as_deref(), Some("gemma4"));
    });
}

#[test]
fn a_cloud_pick_with_the_cloud_setting_off_is_refused_naming_the_setting() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        store_key(&state, LlmProviderData::Anthropic).await;
        assert!(!state.config_snapshot().llm.allow_cloud_relationship_data);

        let message = refusal(pick(&state, "claude-haiku-5-5", false).await);
        assert!(
            message.contains("mxr config set llm.allow_cloud_relationship_data true"),
            "{message}"
        );
        assert!(!state.config_snapshot().llm.allow_cloud_relationship_data);
        assert_eq!(assistant_model_on_disk(), None);
        let listed = models(&state).await;
        let haiku = listed
            .choices
            .iter()
            .find(|c| c.id == "claude-haiku-5-5")
            .unwrap();
        assert!(!haiku.available);
        assert!(listed.choices[0].available);
    });
}

#[test]
fn a_pick_applies_to_the_next_turn_persists_and_can_be_undone() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        allow_cloud(&state).await;
        store_key(&state, LlmProviderData::Openai).await;

        let change = picked(&state, "gpt-6-luna").await;
        assert!(change.changed && !change.dry_run);
        assert_eq!(change.previous.as_deref(), Some("gemma4:latest"));
        assert_eq!(change.models.current.as_deref(), Some("gpt-6-luna"));

        let now = availability(&state).await;
        assert_eq!(now.model.as_deref(), Some("gpt-6-luna"));
        assert!(now.leaves_machine);
        assert_eq!(assistant_model_on_disk().as_deref(), Some("gpt-6-luna"));
        assert_eq!(mxr_config::load_config().unwrap().llm.model, "gemma4");

        let back = picked(&state, "gemma4:latest").await;
        assert!(back.changed);
        let local = availability(&state).await;
        assert_eq!(local.model.as_deref(), Some("gemma4:latest"));
        assert!(!local.leaves_machine);
        assert!(!picked(&state, "gemma4:latest").await.changed);
    });
}

#[test]
fn a_dry_run_says_what_would_change_and_writes_nothing() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        allow_cloud(&state).await;
        store_key(&state, LlmProviderData::Anthropic).await;

        let Response::Ok {
            data: ResponseData::AssistantModelChange { change },
        } = pick(&state, "claude-sonnet-5-5", true).await
        else {
            panic!("expected a preview");
        };
        assert!(change.dry_run && change.changed);
        assert_eq!(change.models.current.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(assistant_model_on_disk(), None);
        assert_eq!(availability(&state).await.model.as_deref(), Some("gemma4"));
        refusal(pick(&state, "gpt-6-luna", true).await);
    });
}

async fn cloud_allowed_on_disk(state: &Arc<AppState>) {
    use_model(state, "gemma4", LOCAL).await;
    allow_cloud(state).await;
    store_key(state, LlmProviderData::Anthropic).await;
}

fn revoke_cloud_on_disk() {
    let mut on_disk = mxr_config::load_config().unwrap();
    on_disk.llm.allow_cloud_relationship_data = false;
    mxr_config::save_config(&on_disk).unwrap();
}

#[test]
fn a_revoke_racing_a_pick_is_never_overwritten_by_the_picks_old_snapshot() {
    isolated(|state| async move {
        for round in 0..20 {
            cloud_allowed_on_disk(&state).await;
            let mut config = state.config_snapshot();
            config.llm.overrides.assistant = None;
            save(&state, config).await;
            let revoke = async {
                for _ in 0..(round % 4) {
                    tokio::task::yield_now().await;
                }
                revoke_cloud_on_disk();
                send(&state, Request::ReloadLlmConfig).await
            };
            let (_, reload) = tokio::join!(pick(&state, "claude-haiku-5-5", false), revoke);
            assert!(matches!(reload, Response::Ok { .. }), "{reload:?}");

            assert!(
                !state.config_snapshot().llm.allow_cloud_relationship_data,
                "round {round}: the daemon allows cloud again"
            );
            assert!(
                !mxr_config::load_config()
                    .unwrap()
                    .llm
                    .allow_cloud_relationship_data,
                "round {round}: the file allows cloud again"
            );
            assert!(
                state
                    .llm
                    .feature_block_reason(mxr_llm::LlmFeature::Assistant)
                    .is_some()
                    || state.config_snapshot().llm.overrides.assistant.is_none(),
                "round {round}: the assistant would reach a cloud model"
            );
        }
    });
}

#[test]
fn agents_cannot_change_the_assistants_model() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let mut config = state.config_snapshot();
        config
            .agent_surfaces
            .profiles
            .insert("mcp".into(), mxr_config::AgentProfileConfig::default());
        state.set_config_for_test(config).await;
        for request in [
            Request::GetAssistantModels,
            Request::SetAssistantModel {
                model: "gemma4:latest".into(),
                dry_run: false,
            },
        ] {
            let response = handle_request(
                &state,
                &IpcMessage {
                    expected_workspace: None,
                    id: 1,
                    source: ClientKind::Mcp,
                    payload: IpcPayload::Request(request),
                },
            )
            .await;
            let IpcPayload::Response(Response::Error { message, .. }) = response.payload else {
                panic!("expected a refusal");
            };
            assert!(
                message.contains("can't drive the in-app assistant"),
                "{message}"
            );
        }
    });
}
