#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test fixtures unwrap mock servers and fail loudly"
)]

//! Claude models through Anthropic first and OpenRouter second, against local
//! mock servers standing in for both. Real time, short waits: the mock
//! servers do real I/O, which paused tokio time does not mix with.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mxr_llm::{
    routes_for_model, ApiKey, ApiKeySource, ChatMessage, CompletionRequest, Endpoint, LlmError,
    LlmFeature, LlmRuntime, RouteHealth, RouteProvider, RouteState, RoutedProvider,
    RoutedProviderConfig,
};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MODEL: &str = "claude-haiku-5-5";
const ANTHROPIC_KEY: &str = "sk-ant-test-anthropic-key";
const OPENROUTER_KEY: &str = "sk-or-test-openrouter-key";

#[derive(Default)]
struct Keys(Mutex<HashMap<RouteProvider, String>>);

impl Keys {
    fn with(entries: &[(RouteProvider, &str)]) -> Arc<Self> {
        let keys = Self::default();
        for (provider, key) in entries {
            keys.set(*provider, key);
        }
        Arc::new(keys)
    }

    fn set(&self, provider: RouteProvider, key: &str) {
        self.0.lock().unwrap().insert(provider, key.to_string());
    }

    fn remove(&self, provider: RouteProvider) {
        self.0.lock().unwrap().remove(&provider);
    }
}

impl ApiKeySource for Keys {
    fn api_key(&self, provider: RouteProvider) -> Option<ApiKey> {
        self.0.lock().unwrap().get(&provider).map(ApiKey::new)
    }
}

struct Rig {
    anthropic: MockServer,
    openrouter: MockServer,
    keys: Arc<Keys>,
    runtime: Arc<LlmRuntime>,
    health: RouteHealth,
}

impl Rig {
    async fn new() -> Self {
        Self::with_keys(Keys::with(&[
            (RouteProvider::Anthropic, ANTHROPIC_KEY),
            (RouteProvider::OpenRouter, OPENROUTER_KEY),
        ]))
        .await
    }

    async fn with_keys(keys: Arc<Keys>) -> Self {
        let anthropic = MockServer::start().await;
        let openrouter = MockServer::start().await;
        let health = RouteHealth::default();
        let mut config = RoutedProviderConfig::new(
            MODEL.to_string(),
            routes_for_model(MODEL, Endpoint::Default).unwrap(),
            200_000,
            Duration::from_secs(10),
            keys.clone(),
            health.clone(),
        );
        config.anthropic_base_url = anthropic.uri();
        config.openrouter_base_url = openrouter.uri();
        let runtime = Arc::new(LlmRuntime::with_route_health(
            Arc::new(RoutedProvider::new(config)),
            health.clone(),
        ));
        Self {
            anthropic,
            openrouter,
            keys,
            runtime,
            health,
        }
    }

    async fn anthropic_requests(&self) -> Vec<Value> {
        bodies(&self.anthropic).await
    }

    async fn openrouter_requests(&self) -> Vec<Value> {
        bodies(&self.openrouter).await
    }

    async fn background(&self) -> Result<mxr_llm::CompletionResponse, LlmError> {
        self.runtime
            .for_feature(LlmFeature::Summarize)
            .complete_background(request())
            .await
    }

    async fn foreground(&self) -> Result<mxr_llm::CompletionResponse, LlmError> {
        self.runtime
            .for_feature(LlmFeature::Summarize)
            .complete(request())
            .await
    }
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

fn anthropic_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": MODEL,
        "content": [
            {"type": "thinking", "thinking": "hmm", "signature": "sig"},
            {"type": "text", "text": "from anthropic"}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 5, "output_tokens": 5}
    }))
}

fn openrouter_ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model": "anthropic/claude-haiku-5.5",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "from openrouter"},
            "finish_reason": "stop"
        }]
    }))
}

fn anthropic_error(status: u16) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "type": "error",
        "error": {"type": "authentication_error", "message": "invalid x-api-key"}
    }))
}

async fn mount_anthropic(rig: &Rig, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(response)
        .mount(&rig.anthropic)
        .await;
}

async fn mount_openrouter(rig: &Rig, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(response)
        .mount(&rig.openrouter)
        .await;
}

fn anthropic_route() -> mxr_llm::Route {
    routes_for_model(MODEL, Endpoint::Default)
        .unwrap()
        .remove(0)
}

#[tokio::test]
async fn a_bad_anthropic_key_costs_one_attempt_for_a_batch_of_twenty() {
    let rig = Rig::new().await;
    mount_anthropic(&rig, anthropic_error(401)).await;
    mount_openrouter(&rig, openrouter_ok()).await;

    for item in 0..20 {
        let response = rig.background().await.unwrap_or_else(|error| {
            panic!("item {item} should be answered by OpenRouter: {error}")
        });
        assert_eq!(response.content, "from openrouter");
        assert_eq!(response.model, "anthropic/claude-haiku-5.5 via OpenRouter");
    }

    assert_eq!(rig.anthropic_requests().await.len(), 1);
    assert_eq!(rig.openrouter_requests().await.len(), 20);
    let view = rig.health.view(&anthropic_route());
    assert_eq!(view.state, RouteState::Broken);
    assert_eq!(view.reason.unwrap().to_string(), "401 invalid key");
}

#[tokio::test]
async fn keys_travel_in_the_headers_each_provider_expects() {
    let rig = Rig::new().await;
    mount_anthropic(&rig, anthropic_ok()).await;
    mount_openrouter(&rig, openrouter_ok()).await;

    let response = rig.foreground().await.unwrap();
    assert_eq!(response.content, "from anthropic");
    assert_eq!(response.model, "claude-haiku-5-5 via Anthropic");
    let received = rig.anthropic.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let headers = &received[0].headers;
    assert_eq!(
        headers.get("x-api-key").unwrap().to_str().unwrap(),
        ANTHROPIC_KEY
    );
    assert_eq!(
        headers.get("anthropic-version").unwrap().to_str().unwrap(),
        "2023-06-01"
    );
    assert!(headers.get("authorization").is_none());
    assert!(rig.openrouter_requests().await.is_empty());

    rig.health.reset();
    rig.keys.remove(RouteProvider::Anthropic);
    rig.foreground().await.unwrap();
    let received = rig.openrouter.received_requests().await.unwrap();
    let headers = &received[0].headers;
    assert_eq!(
        headers.get("authorization").unwrap().to_str().unwrap(),
        format!("Bearer {OPENROUTER_KEY}")
    );
    assert!(headers.get("x-api-key").is_none());
}

#[tokio::test]
async fn with_every_route_out_background_work_fails_fast_and_a_person_gets_one_try() {
    let rig = Rig::new().await;
    mount_anthropic(&rig, anthropic_error(401)).await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&rig.openrouter)
        .await;

    let error = rig.background().await.unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("anthropic: failed (401 invalid key)"),
        "{message}"
    );
    assert!(
        message.contains("openrouter: failed (503 server error)"),
        "{message}"
    );
    assert!(!message.contains(ANTHROPIC_KEY) && !message.contains(OPENROUTER_KEY));

    let sent = rig.anthropic_requests().await.len() + rig.openrouter_requests().await.len();
    assert_eq!(sent, 2);

    assert!(matches!(
        rig.background().await,
        Err(LlmError::CircuitOpen { .. })
    ));
    assert_eq!(
        rig.anthropic_requests().await.len() + rig.openrouter_requests().await.len(),
        2
    );

    let error = rig.foreground().await.unwrap_err().to_string();
    assert_eq!(rig.anthropic_requests().await.len(), 1);
    assert_eq!(rig.openrouter_requests().await.len(), 2);
    assert!(
        error.contains("anthropic: broken (401 invalid key)"),
        "{error}"
    );
    assert!(
        error.contains("openrouter: failed (503 server error)"),
        "{error}"
    );
}

#[tokio::test]
async fn a_key_never_appears_in_an_error() {
    let rig = Rig::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": format!("bad request with {ANTHROPIC_KEY} in it")}
        })))
        .mount(&rig.anthropic)
        .await;
    let error = rig.foreground().await.unwrap_err().to_string();
    assert!(error.contains("invalid_request_error"), "{error}");
    assert!(!error.contains(ANTHROPIC_KEY), "{error}");
    assert_eq!(rig.health.view(&anthropic_route()).state, RouteState::Ok);
    assert!(rig.openrouter_requests().await.is_empty());
}
