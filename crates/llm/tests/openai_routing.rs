#![expect(
    clippy::unwrap_used,
    reason = "test fixtures unwrap mock servers and fail loudly"
)]

//! `gpt-*` models through OpenAI's own API first and OpenRouter second,
//! against local mock servers standing in for both.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mxr_llm::{
    routes_for_model, ApiKey, ApiKeySource, ChatMessage, CompletionRequest, Endpoint, LlmProvider,
    RouteHealth, RouteProvider, RoutedProvider, RoutedProviderConfig,
};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MODEL: &str = "gpt-6-luna";
const OPENAI_KEY: &str = "sk-test-openai-key";
const OPENROUTER_KEY: &str = "sk-or-test-openrouter-key";

struct Keys(Mutex<HashMap<RouteProvider, String>>);

impl ApiKeySource for Keys {
    fn api_key(&self, provider: RouteProvider) -> Option<ApiKey> {
        self.0.lock().unwrap().get(&provider).map(ApiKey::new)
    }
}

struct Rig {
    openai: MockServer,
    openrouter: MockServer,
    provider: RoutedProvider,
}

impl Rig {
    async fn new(keys: &[(RouteProvider, &str)]) -> Self {
        let openai = MockServer::start().await;
        let openrouter = MockServer::start().await;
        let keys = Keys(Mutex::new(
            keys.iter().map(|(p, k)| (*p, (*k).to_string())).collect(),
        ));
        let mut config = RoutedProviderConfig::new(
            MODEL.to_string(),
            routes_for_model(MODEL, Endpoint::Default).unwrap(),
            128_000,
            Duration::from_secs(10),
            Arc::new(keys),
            RouteHealth::default(),
        );
        config.openai_base_url = openai.uri();
        config.openrouter_base_url = openrouter.uri();
        Self {
            openai,
            openrouter,
            provider: RoutedProvider::new(config),
        }
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

fn completion() -> CompletionRequest {
    CompletionRequest {
        messages: vec![
            ChatMessage::system("Write a gist."),
            ChatMessage::user("A thread."),
        ],
        max_tokens: Some(400),
        temperature: Some(0.1),
    }
}

fn reply(model: &str, text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": text},
            "finish_reason": "stop"
        }]
    }))
}

async fn mount(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(response)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_gpt_model_goes_to_openai_with_its_key_and_the_parameters_openai_accepts() {
    let rig = Rig::new(&[
        (RouteProvider::OpenAI, OPENAI_KEY),
        (RouteProvider::OpenRouter, OPENROUTER_KEY),
    ])
    .await;
    mount(&rig.openai, reply("gpt-6-luna-2026-10-01", "from openai")).await;
    mount(
        &rig.openrouter,
        reply("openai/gpt-6-luna", "from openrouter"),
    )
    .await;

    let response = rig.provider.complete(completion()).await.unwrap();

    assert_eq!(response.content, "from openai");
    assert_eq!(response.model, "gpt-6-luna-2026-10-01 via OpenAI");
    let sent = bodies(&rig.openai).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["model"], MODEL);
    assert!(sent[0].get("max_tokens").is_none(), "{}", sent[0]);
    assert!(sent[0].get("temperature").is_none(), "{}", sent[0]);
    assert_eq!(sent[0]["max_completion_tokens"], 400 + 2048);
    let received = rig.openai.received_requests().await.unwrap();
    assert_eq!(
        received[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Bearer {OPENAI_KEY}")
    );
    assert!(bodies(&rig.openrouter).await.is_empty());
}

#[tokio::test]
async fn a_failing_openai_key_falls_over_to_openrouter_under_its_slug() {
    let rig = Rig::new(&[
        (RouteProvider::OpenAI, OPENAI_KEY),
        (RouteProvider::OpenRouter, OPENROUTER_KEY),
    ])
    .await;
    mount(
        &rig.openai,
        ResponseTemplate::new(401).set_body_json(json!({"error": {"message": "bad key"}})),
    )
    .await;
    mount(
        &rig.openrouter,
        reply("openai/gpt-6-luna", "from openrouter"),
    )
    .await;

    let response = rig.provider.complete(completion()).await.unwrap();

    assert_eq!(response.content, "from openrouter");
    assert_eq!(response.model, "openai/gpt-6-luna via OpenRouter");
    let sent = bodies(&rig.openrouter).await;
    assert_eq!(sent[0]["model"], "openai/gpt-6-luna");
    assert_eq!(sent[0]["max_tokens"], 400 + 2048);
    assert!(sent[0].get("max_completion_tokens").is_none());
}
