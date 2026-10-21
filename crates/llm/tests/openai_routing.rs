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
    routes_for_model, ApiKey, ApiKeySource, CallKind, ChatMessage, CompletionRequest, Endpoint,
    LlmProvider, RouteHealth, RouteProvider, RoutedProvider, RoutedProviderConfig, ToolChatMessage,
    ToolChatRequest,
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
    // OpenAI's current models refuse `max_tokens` and a non-default
    // temperature; the cap leaves room for reasoning on top of the 400 asked.
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
    // OpenRouter takes the usual name for the cap.
    assert_eq!(sent[0]["max_tokens"], 400 + 2048);
    assert!(sent[0].get("max_completion_tokens").is_none());
}

#[tokio::test]
async fn with_only_an_openrouter_key_openai_is_never_asked() {
    let rig = Rig::new(&[(RouteProvider::OpenRouter, OPENROUTER_KEY)]).await;
    mount(
        &rig.openrouter,
        reply("openai/gpt-6-luna", "from openrouter"),
    )
    .await;

    rig.provider.complete(completion()).await.unwrap();

    assert!(bodies(&rig.openai).await.is_empty());
    assert_eq!(bodies(&rig.openrouter).await.len(), 1);
}

#[tokio::test]
async fn with_no_key_the_error_names_the_command_and_never_a_key() {
    let rig = Rig::new(&[]).await;

    let error = rig
        .provider
        .complete(completion())
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("mxr llm key set"), "{error}");
    assert!(error.contains("OPENAI_API_KEY"), "{error}");
    assert!(bodies(&rig.openai).await.is_empty());
}

#[tokio::test]
async fn a_tool_chat_turn_carries_the_cap_and_tools_in_openai_form() {
    let rig = Rig::new(&[(RouteProvider::OpenAI, OPENAI_KEY)]).await;
    mount(
        &rig.openai,
        ResponseTemplate::new(200).set_body_json(json!({
            "model": "gpt-6-luna",
            "choices": [{
                "message": {"content": null, "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "mxr_search", "arguments": "{\"query\":\"bank\"}"}
                }]},
                "finish_reason": "tool_calls"
            }]
        })),
    )
    .await;

    let response = rig
        .provider
        .chat_as(
            ToolChatRequest {
                messages: vec![ToolChatMessage::User {
                    content: "find the bank".into(),
                }],
                tools: vec![mxr_llm::ToolSpec {
                    name: "mxr_search".into(),
                    description: "Search".into(),
                    input_schema: json!({"type": "object"}),
                }],
                max_tokens: Some(1000),
            },
            CallKind::Foreground,
        )
        .await
        .unwrap();

    assert_eq!(response.tool_calls[0].name, "mxr_search");
    assert_eq!(response.tool_calls[0].arguments, json!({"query": "bank"}));
    assert_eq!(response.model, "gpt-6-luna via OpenAI");
    let sent = bodies(&rig.openai).await;
    assert_eq!(sent[0]["max_completion_tokens"], 1000);
    // GPT-6 Luna takes tool calls only with reasoning off.
    assert_eq!(sent[0]["reasoning_effort"], "none");
    assert!(sent[0].get("max_tokens").is_none());
    assert_eq!(sent[0]["tools"][0]["function"]["name"], "mxr_search");
}
