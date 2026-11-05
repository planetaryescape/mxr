#![expect(
    clippy::unwrap_used,
    reason = "test fixtures unwrap mock servers and fail loudly"
)]

//! A model on the user's own endpoint with a hosted fallback behind it
//! (`[llm.fallback]`), against mock servers standing in for the endpoint,
//! OpenAI and OpenRouter. Real time, short waits: an endpoint that answers
//! 503 with `retry-after: 1` cools for one second instead of a minute.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mxr_llm::{
    chain_statuses, routes_for_model, ApiKey, ApiKeySource, ChatMessage, CompletionRequest,
    Endpoint, EndpointRoute, LlmError, LlmFeature, LlmProvider, LlmRuntime, OpenAiCompatibleConfig,
    RouteHealth, RouteProvider, RouteState, RoutedProvider, RoutedProviderConfig,
};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const LOCAL_MODEL: &str = "gemma4:latest";
const FALLBACK_MODEL: &str = "gpt-6-luna";
const OPENROUTER_KEY: &str = "sk-or-test-openrouter-key";

#[derive(Default)]
struct Keys(Mutex<HashMap<RouteProvider, String>>);

impl ApiKeySource for Keys {
    fn api_key(&self, provider: RouteProvider) -> Option<ApiKey> {
        self.0.lock().unwrap().get(&provider).map(ApiKey::new)
    }
}

/// BK's setup: only an OpenRouter key, so the chain is the endpoint, then
/// OpenRouter (OpenAI is skipped for want of a key).
fn openrouter_only() -> Arc<Keys> {
    let keys = Keys::default();
    keys.0
        .lock()
        .unwrap()
        .insert(RouteProvider::OpenRouter, OPENROUTER_KEY.to_string());
    Arc::new(keys)
}

struct Rig {
    local: MockServer,
    openai: MockServer,
    openrouter: MockServer,
    keys: Arc<Keys>,
    health: RouteHealth,
    runtime: Arc<LlmRuntime>,
    endpoint: EndpointRoute,
}

impl Rig {
    /// The endpoint with the fallback behind it.
    async fn new() -> Self {
        Self::build(true, None).await
    }

    /// The endpoint alone, as a feature withheld from the fallback gets it.
    async fn withheld() -> Self {
        Self::build(false, None).await
    }

    async fn build(with_fallback: bool, local_url: Option<String>) -> Self {
        let local = MockServer::start().await;
        let openai = MockServer::start().await;
        let openrouter = MockServer::start().await;
        let keys = openrouter_only();
        let health = RouteHealth::default();
        let base_url = local_url.unwrap_or_else(|| format!("{}/v1", local.uri()));
        let routes = if with_fallback {
            routes_for_model(FALLBACK_MODEL, Endpoint::Default).unwrap()
        } else {
            Vec::new()
        };
        let mut config = RoutedProviderConfig::new(
            FALLBACK_MODEL.to_string(),
            routes,
            128_000,
            Duration::from_secs(10),
            keys.clone(),
            health.clone(),
        );
        config.openai_base_url = openai.uri();
        config.openrouter_base_url = openrouter.uri();
        config.endpoint = Some(OpenAiCompatibleConfig {
            base_url: base_url.clone(),
            api_key: None,
            model: LOCAL_MODEL.to_string(),
            context_window: 32_768,
            request_timeout: Duration::from_secs(10),
        });
        let runtime = Arc::new(LlmRuntime::with_route_health(
            Arc::new(RoutedProvider::new(config)),
            health.clone(),
        ));
        Self {
            local,
            openai,
            openrouter,
            keys,
            health,
            runtime,
            endpoint: EndpointRoute::new(&base_url, LOCAL_MODEL),
        }
    }

    async fn background(
        &self,
        feature: LlmFeature,
    ) -> Result<mxr_llm::CompletionResponse, LlmError> {
        self.runtime
            .for_feature(feature)
            .complete_background(request())
            .await
    }

    async fn foreground(&self) -> Result<mxr_llm::CompletionResponse, LlmError> {
        self.runtime
            .for_feature(LlmFeature::Summarize)
            .complete(request())
            .await
    }

    fn endpoint_state(&self) -> RouteState {
        self.health.view(&self.endpoint).state
    }

    /// The local server's traffic as `METHOD /path`, in order.
    async fn local_calls(&self) -> Vec<String> {
        calls(&self.local).await
    }
}

async fn calls(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect()
}

async fn bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}

fn request() -> CompletionRequest {
    CompletionRequest {
        messages: vec![
            ChatMessage::system("Write a gist."),
            ChatMessage::user("A thread."),
        ],
        max_tokens: Some(400),
        temperature: Some(0.1),
    }
}

fn completion(model: &str, content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }]
    }))
}

/// Out for a second: what a sleeping machine behind a tunnel answers, with
/// a short `retry-after` so the test needn't wait a minute.
fn out_for_a_second() -> ResponseTemplate {
    ResponseTemplate::new(503).insert_header("retry-after", "1")
}

async fn local_answers(rig: &Rig) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(completion(LOCAL_MODEL, "from local"))
        .mount(&rig.local)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .mount(&rig.local)
        .await;
}

async fn local_is_out(rig: &Rig, reply: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(reply.clone())
        .mount(&rig.local)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(reply)
        .mount(&rig.local)
        .await;
}

async fn openrouter_answers(rig: &Rig) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(completion("openai/gpt-6-luna", "from openrouter"))
        .mount(&rig.openrouter)
        .await;
}

fn wait_past_the_retry_after() -> tokio::time::Sleep {
    tokio::time::sleep(Duration::from_millis(1_100))
}

#[tokio::test]
async fn a_local_outage_costs_one_attempt_for_a_batch_then_the_fallback_answers() {
    let rig = Rig::new().await;
    local_is_out(&rig, ResponseTemplate::new(503)).await;
    openrouter_answers(&rig).await;

    for _ in 0..10 {
        let response = rig.background(LlmFeature::Summarize).await.unwrap();
        assert_eq!(response.content, "from openrouter");
        assert_eq!(response.model, "openai/gpt-6-luna via OpenRouter");
    }

    assert_eq!(
        rig.local_calls().await,
        ["POST /v1/chat/completions"],
        "one failed local attempt, not ten"
    );
    assert_eq!(bodies(&rig.openrouter).await.len(), 10);
    assert!(
        calls(&rig.openai).await.is_empty(),
        "no OpenAI key, no call"
    );
    assert_eq!(rig.endpoint_state(), RouteState::Cooling);

    // The endpoint got the request as written; the fallback got it the way
    // OpenAI's models take it.
    let local = bodies(&rig.local).await;
    assert_eq!(local[0]["model"], LOCAL_MODEL);
    assert!(local[0]["temperature"].is_number());
    let cloud = bodies(&rig.openrouter).await;
    assert_eq!(cloud[0]["model"], "openai/gpt-6-luna");
    assert!(cloud[0].get("temperature").is_none(), "{}", cloud[0]);
}

#[tokio::test]
async fn a_closed_local_port_falls_back_within_the_same_call() {
    // A port nothing listens on: connection refused, as from a machine
    // that is off.
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let rig = Rig::build(true, Some(format!("http://{closed}/v1"))).await;
    openrouter_answers(&rig).await;

    let response = rig.foreground().await.unwrap();
    assert_eq!(response.content, "from openrouter");

    let (endpoint, routes) = chain_statuses(
        &rig.endpoint,
        &routes_for_model(FALLBACK_MODEL, Endpoint::Default).unwrap(),
        rig.keys.as_ref(),
        &rig.health,
    );
    assert_eq!(endpoint.view.state, RouteState::Cooling);
    assert_eq!(
        endpoint.view.reason.map(|reason| reason.to_string()),
        Some("unreachable".to_string())
    );
    assert!(!endpoint.next);
    let next: Vec<_> = routes
        .iter()
        .map(|status| (status.route.provider, status.key_present, status.next))
        .collect();
    assert_eq!(
        next,
        [
            (RouteProvider::OpenAI, false, false),
            (RouteProvider::OpenRouter, true, true)
        ]
    );
}

#[tokio::test]
async fn traffic_returns_to_the_endpoint_after_its_wait_with_a_cheap_check() {
    let rig = Rig::new().await;
    local_is_out(&rig, out_for_a_second()).await;
    openrouter_answers(&rig).await;
    let mut recovered = rig.runtime.subscribe_background_recovery();
    recovered.borrow_and_update();

    assert_eq!(
        rig.background(LlmFeature::Summarize).await.unwrap().content,
        "from openrouter"
    );
    rig.local.reset().await;
    local_answers(&rig).await;
    // Still cooling: the fallback keeps answering without asking the endpoint.
    assert_eq!(
        rig.background(LlmFeature::Summarize).await.unwrap().content,
        "from openrouter"
    );
    assert!(rig.local_calls().await.is_empty());

    wait_past_the_retry_after().await;
    let response = rig.background(LlmFeature::Summarize).await.unwrap();
    assert_eq!(response.content, "from local");
    assert_eq!(
        response.model, LOCAL_MODEL,
        "a local answer keeps its label"
    );
    assert_eq!(
        rig.local_calls().await,
        ["GET /v1/models", "POST /v1/chat/completions"],
        "the model listing is the check; the prompt goes once it answers"
    );
    assert!(recovered.has_changed().unwrap(), "recovery is signalled");
    assert_eq!(rig.endpoint_state(), RouteState::Ok);
    assert_eq!(bodies(&rig.openrouter).await.len(), 2);
}

#[tokio::test]
async fn an_endpoint_still_out_after_its_wait_costs_a_listing_not_a_prompt() {
    let rig = Rig::new().await;
    local_is_out(&rig, out_for_a_second()).await;
    openrouter_answers(&rig).await;

    rig.background(LlmFeature::Summarize).await.unwrap();
    wait_past_the_retry_after().await;
    let response = rig.background(LlmFeature::Summarize).await.unwrap();
    assert_eq!(response.content, "from openrouter", "same call moves on");
    assert_eq!(
        rig.local_calls().await,
        ["POST /v1/chat/completions", "GET /v1/models"]
    );
    assert_eq!(rig.endpoint_state(), RouteState::Cooling);
}

#[tokio::test]
async fn racing_callers_share_one_check_of_the_endpoint() {
    let rig = Rig::new().await;
    local_is_out(&rig, out_for_a_second()).await;
    openrouter_answers(&rig).await;
    rig.foreground().await.unwrap();

    rig.local.reset().await;
    // A slow listing, so every caller arrives while the check is running.
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": []}))
                .set_delay(Duration::from_millis(400)),
        )
        .mount(&rig.local)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(completion(LOCAL_MODEL, "from local"))
        .mount(&rig.local)
        .await;
    wait_past_the_retry_after().await;

    let rig = Arc::new(rig);
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let rig = rig.clone();
        tasks.push(tokio::spawn(async move {
            rig.background(LlmFeature::Summarize).await.unwrap().content
        }));
    }
    let mut answers = Vec::new();
    for task in tasks {
        answers.push(task.await.unwrap());
    }

    let listings = rig
        .local_calls()
        .await
        .into_iter()
        .filter(|call| call == "GET /v1/models")
        .count();
    assert_eq!(listings, 1, "one caller checks; the rest don't wait on it");
    assert_eq!(
        answers
            .iter()
            .filter(|answer| *answer == "from local")
            .count(),
        1,
        "{answers:?}"
    );
    assert_eq!(
        answers
            .iter()
            .filter(|answer| *answer == "from openrouter")
            .count(),
        7,
        "{answers:?}"
    );
    assert_eq!(rig.endpoint_state(), RouteState::Ok);
}

#[tokio::test]
async fn a_withheld_feature_waits_for_its_endpoint_and_resumes_there() {
    let rig = Rig::withheld().await;
    // OpenRouter would answer, if anything were sent there.
    openrouter_answers(&rig).await;
    local_is_out(&rig, out_for_a_second()).await;
    const FEATURE: LlmFeature = LlmFeature::RelationshipSummary;
    let feature = rig.runtime.for_feature(FEATURE);

    let error = rig.background(FEATURE).await.unwrap_err();
    assert!(error.is_endpoint_failure(), "{error:?}");
    feature.record_background_failure("contact", "fp", &error);
    // While the endpoint cools, more work is put off without a request.
    let error = rig.background(FEATURE).await.unwrap_err();
    assert!(matches!(error, LlmError::CircuitOpen { .. }), "{error:?}");
    assert!(rig
        .runtime
        .background_recovery_due(&[FEATURE])
        .await
        .is_empty());

    rig.local.reset().await;
    local_answers(&rig).await;
    wait_past_the_retry_after().await;
    assert_eq!(
        rig.runtime.background_recovery_due(&[FEATURE]).await,
        [FEATURE]
    );
    assert_eq!(rig.runtime.take_deferred_background(FEATURE), ["contact"]);
    let response = rig.background(FEATURE).await.unwrap();
    assert_eq!(response.content, "from local");

    assert!(
        calls(&rig.openrouter).await.is_empty(),
        "nothing reached the cloud"
    );
    assert!(
        calls(&rig.openai).await.is_empty(),
        "nothing reached the cloud"
    );
    let pinned = feature.pin();
    assert!(
        !pinned.leaves_machine(),
        "a loopback endpoint alone stays local"
    );
    assert_eq!(pinned.base_url(), Some(rig.endpoint.base_url.as_str()));
}

#[tokio::test]
async fn the_chain_names_the_endpoint_model_fits_both_windows_and_leaves_the_machine() {
    let rig = Rig::new().await;
    let pinned = rig.runtime.for_feature(LlmFeature::Summarize).pin();
    assert_eq!(pinned.model_name(), LOCAL_MODEL);
    assert_eq!(pinned.context_window(), 32_768, "the smaller window");
    assert!(pinned.leaves_machine(), "the fallback is a cloud service");
    assert_eq!(pinned.base_url(), None, "answers can come from either");
}

#[tokio::test]
async fn a_config_reload_forgets_that_the_endpoint_was_out() {
    let rig = Rig::new().await;
    local_is_out(&rig, ResponseTemplate::new(503)).await;
    openrouter_answers(&rig).await;
    rig.foreground().await.unwrap();
    assert_eq!(rig.endpoint_state(), RouteState::Cooling);

    rig.runtime.replace(Arc::new(mxr_llm::NoopProvider));
    assert_eq!(rig.endpoint_state(), RouteState::Ok);
}

#[tokio::test]
async fn a_foreground_call_with_every_route_out_tries_the_soonest_once() {
    let rig = Rig::new().await;
    local_is_out(&rig, ResponseTemplate::new(503)).await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&rig.openrouter)
        .await;

    let error = rig.foreground().await.unwrap_err().to_string();
    assert!(
        error.contains("base_url: failed (503 server error)"),
        "{error}"
    );
    assert!(
        error.contains("openrouter: failed (503 server error)"),
        "{error}"
    );

    // Background work is put off without a request.
    let error = rig.background(LlmFeature::Summarize).await.unwrap_err();
    assert!(matches!(error, LlmError::CircuitOpen { .. }), "{error:?}");
    assert_eq!(rig.local_calls().await.len(), 1);
}

#[tokio::test]
async fn the_provider_reports_its_chain() {
    let rig = Rig::new().await;
    let provider = RoutedProvider::new({
        let mut config = RoutedProviderConfig::new(
            FALLBACK_MODEL.to_string(),
            routes_for_model(FALLBACK_MODEL, Endpoint::Default).unwrap(),
            128_000,
            Duration::from_secs(10),
            rig.keys.clone(),
            rig.health.clone(),
        );
        config.endpoint = Some(OpenAiCompatibleConfig {
            base_url: format!("{}/v1/", rig.local.uri()),
            api_key: None,
            model: LOCAL_MODEL.to_string(),
            context_window: 32_768,
            request_timeout: Duration::from_secs(10),
        });
        config
    });
    assert_eq!(
        provider.endpoint(),
        Some(&rig.endpoint),
        "trailing slash or not"
    );
    assert!(provider.tracks_route_health());
}
