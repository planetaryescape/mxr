#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test fixtures unwrap mock servers and fail loudly"
)]

//! What a model call records for Langfuse, captured in memory: the span
//! tree, the attributes Langfuse maps, and a call cut short. Each test uses
//! its own feature, since the tracer provider is global to this binary.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use mxr_llm::{
    ChatMessage, CompletionRequest, LlmError, LlmFeature, LlmRuntime, OpenAiCompatibleConfig,
    OpenAiCompatibleProvider,
};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MODEL: &str = "gemma4:latest";

fn exporter() -> &'static InMemorySpanExporter {
    static EXPORTER: OnceLock<InMemorySpanExporter> = OnceLock::new();
    EXPORTER.get_or_init(|| {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        opentelemetry::global::set_tracer_provider(provider);
        exporter
    })
}

fn runtime(server: &MockServer) -> Arc<LlmRuntime> {
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: format!("{}/v1", server.uri()),
        api_key: None,
        model: MODEL.to_string(),
        context_window: 8192,
        request_timeout: Duration::from_secs(10),
    });
    Arc::new(LlmRuntime::new(Arc::new(provider)))
}

fn request() -> CompletionRequest {
    CompletionRequest {
        messages: vec![
            ChatMessage::system("Summarize the thread."),
            ChatMessage::user("Lunch on Friday?"),
        ],
        max_tokens: Some(64),
        temperature: Some(0.2),
    }
}

fn reply(content: &str, reasoning: Option<&str>) -> Value {
    json!({
        "model": MODEL,
        "choices": [{
            "message": {"role": "assistant", "content": content, "reasoning": reasoning},
            "finish_reason": if content.is_empty() { "length" } else { "stop" },
        }],
        "usage": {"prompt_tokens": 12, "completion_tokens": 3},
    })
}

fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == key)
        .map(|attribute| attribute.value.as_str().into_owned())
}

fn json_attribute(span: &SpanData, key: &str) -> Value {
    serde_json::from_str(&attribute(span, key).unwrap_or_else(|| panic!("{key} is missing")))
        .unwrap()
}

#[tokio::test]
async fn a_feature_call_is_one_span_with_a_generation_per_request() {
    let exporter = exporter();
    let server = MockServer::start().await;
    // A thinking model that spends the budget on reasoning, then answers
    // when asked again with reasoning off.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(json!({"reasoning_effort": "none"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply("Lunch is Friday.", None)))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply("", Some("Let me think"))))
        .with_priority(2)
        .mount(&server)
        .await;

    let response = runtime(&server)
        .for_feature(LlmFeature::Summarize)
        .complete(request())
        .await
        .unwrap();
    assert_eq!(response.content, "Lunch is Friday.");

    let spans = exporter.get_finished_spans().unwrap();
    let feature = spans
        .iter()
        .find(|span| span.name == "summarize")
        .expect("a span named after the feature");
    assert_eq!(
        attribute(feature, "langfuse.observation.type").as_deref(),
        Some("span")
    );
    assert_eq!(
        json_attribute(feature, "langfuse.observation.input"),
        json!([
            {"role": "system", "content": "Summarize the thread."},
            {"role": "user", "content": "Lunch on Friday?"},
        ])
    );
    assert_eq!(
        json_attribute(feature, "langfuse.observation.output"),
        json!("Lunch is Friday.")
    );

    let generations: Vec<&SpanData> = spans
        .iter()
        .filter(|span| span.parent_span_id == feature.span_context.span_id())
        .collect();
    assert_eq!(
        generations.len(),
        2,
        "the reasoning retry is its own generation"
    );
    let (first, retry) = (generations[0], generations[1]);
    for generation in [first, retry] {
        assert_eq!(generation.name, "generate");
        assert_eq!(
            attribute(generation, "langfuse.observation.type").as_deref(),
            Some("generation")
        );
        assert_eq!(
            attribute(generation, "langfuse.observation.model.name").as_deref(),
            Some(MODEL)
        );
        assert_eq!(
            json_attribute(generation, "langfuse.observation.usage_details"),
            json!({"input": 12, "output": 3})
        );
        assert_eq!(
            generation.span_context.trace_id(),
            feature.span_context.trace_id()
        );
        assert_eq!(
            attribute(generation, "langfuse.trace.name").as_deref(),
            Some("summarize"),
            "generations carry the trace name so they can be filtered by feature"
        );
        assert_eq!(
            json_attribute(generation, "langfuse.observation.model.parameters")["temperature"],
            json!(0.2)
        );
    }
    assert_eq!(
        json_attribute(first, "langfuse.observation.output"),
        json!({"role": "assistant", "content": "", "reasoning_content": "Let me think"}),
        "the empty answer and its reasoning show where the budget went"
    );
    assert!(
        json_attribute(first, "langfuse.observation.model.parameters")
            .get("reasoning_effort")
            .is_none(),
        "unset parameters are left out"
    );
    assert_eq!(
        json_attribute(retry, "langfuse.observation.model.parameters")["reasoning_effort"],
        json!("none")
    );
    assert_eq!(
        json_attribute(retry, "langfuse.observation.output"),
        json!({"role": "assistant", "content": "Lunch is Friday."})
    );
}

#[tokio::test]
async fn a_call_cut_off_by_the_background_deadline_says_so() {
    let exporter = exporter();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(reply("too late", None))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let runtime = runtime(&server);
    runtime.set_background_timeout(Duration::from_millis(200));

    let error = runtime
        .for_feature(LlmFeature::Commitments)
        .complete_background(request())
        .await
        .unwrap_err();
    assert!(matches!(error, LlmError::Timeout(_)), "{error:?}");

    let spans = exporter.get_finished_spans().unwrap();
    let feature = spans
        .iter()
        .find(|span| span.name == "commitments")
        .expect("the feature span ends when its call is dropped");
    assert_eq!(
        attribute(feature, "langfuse.observation.level").as_deref(),
        Some("WARNING")
    );
    assert!(attribute(feature, "langfuse.observation.output").is_none());
    let generation = spans
        .iter()
        .find(|span| span.parent_span_id == feature.span_context.span_id())
        .expect("the request in flight is recorded too");
    assert_eq!(
        attribute(generation, "langfuse.observation.level").as_deref(),
        Some("WARNING")
    );
}
