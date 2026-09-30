//! Privacy: with `HTTP_PROXY` set and no matching `NO_PROXY`, a loopback
//! LLM endpoint is still called directly, never through the proxy, so a
//! "local" model provably stays on this machine. Its own test binary: the
//! proxy variables are process-wide and would leak into other tests.

use std::time::Duration;

use mxr_llm::{
    ChatMessage, CompletionRequest, LlmProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn a_loopback_endpoint_skips_the_system_proxy() {
    let proxy = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(502))
        .mount(&proxy)
        .await;
    let endpoint = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "local-7b",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "hello"},
                "finish_reason": "stop"
            }]
        })))
        .mount(&endpoint)
        .await;
    // Process-wide, which is why this test has its own binary.
    for name in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        std::env::set_var(name, proxy.uri());
    }
    for name in ["NO_PROXY", "no_proxy"] {
        std::env::remove_var(name);
    }

    // The environment really does route plain clients through the proxy.
    let plain = reqwest::Client::new()
        .get(format!("{}/ping", endpoint.uri()))
        .send()
        .await
        .expect("the proxy answers");
    assert_eq!(
        plain.status().as_u16(),
        502,
        "a plain client goes via the proxy"
    );
    let proxied = proxy.received_requests().await.unwrap_or_default().len();
    assert_eq!(proxied, 1);

    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: endpoint.uri(),
        api_key: None,
        model: "local-7b".into(),
        context_window: 8192,
        request_timeout: Duration::from_secs(5),
    });
    let response = provider
        .complete(CompletionRequest {
            messages: vec![ChatMessage::user("private history")],
            max_tokens: Some(50),
            temperature: None,
        })
        .await
        .expect("the local endpoint answers directly");
    assert_eq!(response.content, "hello");
    assert_eq!(
        proxy.received_requests().await.unwrap_or_default().len(),
        proxied,
        "the prompt never went through the proxy"
    );
}
