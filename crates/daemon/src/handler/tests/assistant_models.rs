//! The assistant's model picker over IPC: what it lists, what it refuses,
//! what a pick changes, and that no key ever shows.

use super::llm_routes::{isolated, send, LOCAL};
use super::*;
use mxr_protocol::{
    AssistantAvailabilityData, AssistantModelChangeData, AssistantModelsData, LlmKeySecret,
    LlmProviderData,
};

/// A daemon rereads `[llm]` from the file before it changes config, so a
/// test's setup has to be on disk as well as in memory.
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

/// What `[llm.overrides.assistant]` says on disk.
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
        // `gemma4` in config and `gemma4:latest` in the list are one model.
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
fn a_cloud_model_without_its_key_is_refused_with_the_command_to_add_one() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        allow_cloud(&state).await;
        let message = refusal(pick(&state, "gpt-6-luna", false).await);
        assert!(message.contains("mxr llm key set openai"), "{message}");
        assert_eq!(assistant_model_on_disk(), None);
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
        // Not flipped silently: the setting and the model are as they were.
        assert!(!state.config_snapshot().llm.allow_cloud_relationship_data);
        assert_eq!(assistant_model_on_disk(), None);
        // The list says the same, so a client shows it disabled.
        let listed = models(&state).await;
        let haiku = listed
            .choices
            .iter()
            .find(|c| c.id == "claude-haiku-5-5")
            .unwrap();
        assert!(!haiku.available);
        // Local stays pickable.
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

        // The running provider is the new one, and says it leaves the machine.
        let now = availability(&state).await;
        assert_eq!(now.model.as_deref(), Some("gpt-6-luna"));
        assert!(now.leaves_machine);
        // On disk as the assistant's own override; the base model is untouched.
        assert_eq!(assistant_model_on_disk().as_deref(), Some("gpt-6-luna"));
        assert_eq!(mxr_config::load_config().unwrap().llm.model, "gemma4");

        let back = picked(&state, "gemma4:latest").await;
        assert!(back.changed);
        let local = availability(&state).await;
        assert_eq!(local.model.as_deref(), Some("gemma4:latest"));
        assert!(!local.leaves_machine);
        // Picking what is already in use changes nothing.
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
        // A refused pick is refused as a preview too.
        refusal(pick(&state, "gpt-6-luna", true).await);
    });
}

#[test]
fn a_key_never_appears_in_a_response_a_status_or_a_log_line() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        allow_cloud(&state).await;
        let logs = LogCapture::default();
        let _guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(logs.clone())
                .finish(),
        );

        store_key(&state, LlmProviderData::Openai).await;
        let mut seen = vec![
            format!("{:?}", models(&state).await),
            format!("{:?}", pick(&state, "gpt-5.6-terra", true).await),
            format!("{:?}", pick(&state, "gpt-5.6-terra", false).await),
            format!("{:?}", pick(&state, "claude-haiku-5-5", false).await),
            format!("{:?}", availability(&state).await),
            format!("{:?}", send(&state, Request::GetLlmStatus).await),
            format!("{:?}", send(&state, Request::GetLlmConfig).await),
        ];
        for response in &seen {
            assert!(
                !response.contains(SECRET),
                "a key reached a response: {response}"
            );
        }
        // The wire form too, not only Debug.
        seen.clear();
        let wire = serde_json::to_string(&send(&state, Request::GetLlmStatus).await).unwrap();
        assert!(!wire.contains(SECRET));
        assert!(wire.contains("\"present\":true"));
        assert!(
            !logs.text().contains(SECRET),
            "a key reached the log: {}",
            logs.text()
        );
        assert!(logs.text().contains("assistant model set"));
    });
}

/// The state a person has with cloud data allowed and an Anthropic key, on
/// disk as well as in the daemon.
async fn cloud_allowed_on_disk(state: &Arc<AppState>) {
    use_model(state, "gemma4", LOCAL).await;
    allow_cloud(state).await;
    store_key(state, LlmProviderData::Anthropic).await;
}

/// What `mxr config set llm.allow_cloud_relationship_data false` writes.
fn revoke_cloud_on_disk() {
    let mut on_disk = mxr_config::load_config().unwrap();
    on_disk.llm.allow_cloud_relationship_data = false;
    mxr_config::save_config(&on_disk).unwrap();
}

#[test]
fn a_revoke_that_reached_the_file_first_stops_the_pick_and_survives_it() {
    isolated(|state| async move {
        cloud_allowed_on_disk(&state).await;
        // The CLI has written the file; its reload request has not arrived.
        revoke_cloud_on_disk();

        let message = refusal(pick(&state, "claude-haiku-5-5", false).await);
        assert!(
            message.contains("llm.allow_cloud_relationship_data"),
            "{message}"
        );
        assert!(!state.config_snapshot().llm.allow_cloud_relationship_data);
        assert!(
            !mxr_config::load_config()
                .unwrap()
                .llm
                .allow_cloud_relationship_data
        );
        assert_eq!(assistant_model_on_disk(), None);
    });
}

#[test]
fn a_revoke_racing_a_pick_is_never_overwritten_by_the_picks_old_snapshot() {
    isolated(|state| async move {
        // Both orders matter, and the interleaving inside a pick (it awaits
        // while applying config) can differ from run to run.
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
            // With the setting off, a cloud model for the assistant is blocked.
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
fn a_config_set_in_another_process_is_never_restored_by_a_pick_in_flight() {
    isolated(|state| async move {
        for round in 0..20 {
            cloud_allowed_on_disk(&state).await;
            let mut config = state.config_snapshot();
            config.llm.overrides.assistant = None;
            save(&state, config).await;
            // What `mxr config set llm.allow_cloud_relationship_data false`
            // does in its own process: one locked read-modify-write of the
            // file, then a reload request.
            let cli = tokio::task::spawn_blocking(|| {
                mxr_config::update_config(|config| {
                    config.llm.allow_cloud_relationship_data = false;
                    Ok::<_, mxr_config::ConfigError>(())
                })
                .unwrap();
            });
            let pick = pick(&state, "claude-haiku-5-5", false);
            let (_, cli) = tokio::join!(pick, cli);
            cli.unwrap();
            send(&state, Request::ReloadLlmConfig).await;

            assert!(
                !mxr_config::load_config()
                    .unwrap()
                    .llm
                    .allow_cloud_relationship_data,
                "round {round}: the pick restored the setting on disk"
            );
            assert!(
                !state.config_snapshot().llm.allow_cloud_relationship_data,
                "round {round}: the daemon allows cloud again"
            );
        }
    });
}

#[test]
fn a_config_set_of_another_key_made_while_the_daemon_runs_survives_a_pick() {
    isolated(|state| async move {
        cloud_allowed_on_disk(&state).await;
        // `mxr config set search.max_results 200`: the file changes, the
        // daemon's copy is older and doesn't know.
        mxr_config::update_config(|config| {
            config.search.max_results = 200;
            Ok::<_, mxr_config::ConfigError>(())
        })
        .unwrap();
        assert_ne!(state.config_snapshot().search.max_results, 200);

        let change = picked(&state, "claude-haiku-5-5").await;
        assert!(change.changed);

        // The pick is on disk and the other key was not reverted by it.
        assert_eq!(
            assistant_model_on_disk().as_deref(),
            Some("claude-haiku-5-5")
        );
        assert_eq!(mxr_config::load_config().unwrap().search.max_results, 200);
        // And the daemon now runs with both.
        assert_eq!(state.config_snapshot().search.max_results, 200);
        assert_eq!(
            state
                .config_snapshot()
                .llm
                .overrides
                .assistant
                .and_then(|assistant| assistant.model)
                .as_deref(),
            Some("claude-haiku-5-5")
        );
    });
}

#[test]
fn saving_llm_settings_without_overrides_keeps_a_pick_made_at_the_same_time() {
    isolated(|state| async move {
        for round in 0..10 {
            cloud_allowed_on_disk(&state).await;
            let mut config = state.config_snapshot();
            config.llm.overrides.assistant = None;
            save(&state, config).await;
            let Response::Ok {
                data: ResponseData::LlmConfig { mut config },
            } = send(&state, Request::GetLlmConfig).await
            else {
                panic!("expected the config");
            };
            // A client that doesn't send overrides.
            config.overrides = None;

            let (pick, update) = tokio::join!(
                pick(&state, "claude-haiku-5-5", false),
                send(
                    &state,
                    Request::UpdateLlmConfig {
                        config: Box::new(config)
                    }
                )
            );
            assert!(
                matches!(pick, Response::Ok { .. }),
                "round {round}: {pick:?}"
            );
            assert!(
                matches!(update, Response::Ok { .. }),
                "round {round}: {update:?}"
            );

            assert_eq!(
                assistant_model_on_disk().as_deref(),
                Some("claude-haiku-5-5"),
                "round {round}: the settings save undid the pick on disk"
            );
            assert_eq!(
                state
                    .config_snapshot()
                    .llm
                    .overrides
                    .assistant
                    .and_then(|assistant| assistant.model)
                    .as_deref(),
                Some("claude-haiku-5-5"),
                "round {round}: the daemon lost the pick"
            );
        }
    });
}

#[test]
fn a_pick_during_a_pending_restore_does_not_turn_ai_back_on() {
    isolated(|state| async move {
        // The file says AI is on; a restored workspace keeps it off until it
        // is activated, in memory and in what the daemon applies.
        use_model(&state, "gemma4", LOCAL).await;
        state
            .restore_pending
            .store(true, std::sync::atomic::Ordering::Release);
        state.reload_llm_config_from_disk().await.unwrap();
        assert!(!state.config_snapshot().llm.enabled);

        // The handler stops changes while the workspace is paused...
        let message = refusal(pick(&state, "gpt-6-luna", false).await);
        assert!(
            message.contains("Restored workspace is paused"),
            "{message}"
        );
        // ...and the model change itself would refuse too: choosing a model
        // doesn't switch AI on, even a local one.
        for model in ["gemma4:latest", "gpt-6-luna"] {
            let error = crate::assistant::set_model(&state, model, false)
                .await
                .unwrap_err();
            assert!(error.contains("llm.enabled"), "{error}");
        }

        assert!(!state.config_snapshot().llm.enabled);
        assert!(state.config_snapshot().llm.overrides.assistant.is_none());
        let pinned = state.llm.for_feature(mxr_llm::LlmFeature::Assistant).pin();
        assert!(!pinned.is_configured());
    });
}

#[test]
fn a_list_naming_an_unknown_id_shows_it_instead_of_failing() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let mut config = state.config_snapshot();
        config.llm.assistant_models = vec!["gemma4:latest".into(), "mystery-model-9".into()];
        state.set_config_for_test(config).await;

        let listed = models(&state).await;
        assert_eq!(listed.choices.len(), 2);
        let mystery = &listed.choices[1];
        assert_eq!(
            (mystery.label.as_str(), mystery.cloud),
            ("mystery-model-9", false)
        );
        assert!(mystery.available);
    });
}

#[test]
fn a_model_set_by_hand_is_current_even_when_it_is_not_listed() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let mut config = state.config_snapshot();
        config.llm.overrides.assistant = Some(mxr_config::LlmOverrideConfig {
            model: Some("my-own-model".into()),
            ..Default::default()
        });
        state.set_config_for_test(config).await;
        assert_eq!(
            models(&state).await.current.as_deref(),
            Some("my-own-model")
        );
    });
}

#[test]
fn saving_llm_settings_keeps_the_edited_model_list() {
    isolated(|state| async move {
        use_model(&state, "gemma4", LOCAL).await;
        let mut config = state.config_snapshot();
        config.llm.assistant_models = vec!["gemma4:latest".into(), "claude-haiku-5-5".into()];
        save(&state, config).await;

        let Response::Ok {
            data: ResponseData::LlmConfig { config },
        } = send(&state, Request::GetLlmConfig).await
        else {
            panic!("expected the config");
        };
        let saved = send(
            &state,
            Request::UpdateLlmConfig {
                config: Box::new(config),
            },
        )
        .await;
        assert!(matches!(saved, Response::Ok { .. }), "{saved:?}");
        assert_eq!(
            state.config_snapshot().llm.assistant_models,
            ["gemma4:latest", "claude-haiku-5-5"]
        );
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

/// A `tracing` writer that keeps what it is given.
#[derive(Clone, Default)]
struct LogCapture(Arc<std::sync::Mutex<Vec<u8>>>);

impl LogCapture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl std::io::Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
