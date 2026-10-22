//! Langfuse-shaped OpenTelemetry spans for model calls and the work around
//! them, following Langfuse's OTel attribute mapping
//! (<https://langfuse.com/integrations/native/opentelemetry>).
//!
//! The daemon installs an exporter only for a Langfuse on this machine.
//! Until it does, the global tracer is a no-op: nothing records and no
//! prompt is serialized, so these calls cost next to nothing.
//!
//! The plain OpenTelemetry API is used rather than `tracing` spans on
//! purpose: the daemon's log layer writes span fields to its log files, and
//! these spans carry whole prompts and replies.

use crate::failure::AttemptError;
use crate::tool_chat::{self, OpenAiDialect, TokenUsage, ToolChatMessage, ToolChatRequest};
use crate::{ChatMessage, CompletionResponse, LlmError, LlmFeature, ToolChatResponse};
use opentelemetry::trace::{FutureExt, Status, TraceContextExt, Tracer};
use opentelemetry::{global, Context, KeyValue};
use serde_json::{json, Map, Value};
use std::borrow::Cow;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

const TRACER: &str = "mxr";

/// Langfuse observation types mxr emits.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Span,
    Generation,
    Agent,
    Tool,
}

impl Kind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Span => "span",
            Self::Generation => "generation",
            Self::Agent => "agent",
            Self::Tool => "tool",
        }
    }
}

static SHARED: OnceLock<Vec<KeyValue>> = OnceLock::new();

/// Attributes every observation carries. Langfuse filters on environment
/// and release per observation, so setting them on the root alone isn't
/// enough. Set once, by the daemon, when it starts exporting.
pub fn set_shared_attributes(environment: Option<String>, release: String) {
    let mut shared = vec![KeyValue::new("langfuse.release", release)];
    if let Some(environment) = environment {
        shared.push(KeyValue::new("langfuse.environment", environment));
    }
    let _ = SHARED.set(shared);
}

/// Carried in the context so everything under a session's span gets its id.
struct SessionId(String);

/// The root observation's name, carried to every observation under it:
/// Langfuse filters observations by trace name only where it is set.
struct TraceName(String);

/// One Langfuse observation. Ends when dropped; one dropped before
/// [`Self::output`] or [`Self::fail`] was cut short (a timeout or a
/// cancelled turn) and says so.
pub struct Observation {
    cx: Context,
    settled: AtomicBool,
}

impl Observation {
    /// Start an observation under the current one, if any.
    pub fn start(name: impl Into<Cow<'static, str>>, kind: Kind) -> Self {
        let name = name.into();
        let parent = Context::current();
        let trace_name = match parent.get::<TraceName>() {
            Some(TraceName(trace_name)) if parent.has_active_span() => trace_name.clone(),
            _ => name.to_string(),
        };
        let span = global::tracer(TRACER).start_with_context(name, &parent);
        let observation = Self {
            cx: parent
                .with_span(span)
                .with_value(TraceName(trace_name.clone())),
            settled: AtomicBool::new(false),
        };
        if observation.is_recording() {
            let span = observation.cx.span();
            span.set_attribute(KeyValue::new("langfuse.observation.type", kind.as_str()));
            span.set_attribute(KeyValue::new("langfuse.trace.name", trace_name));
            for attribute in SHARED.get().into_iter().flatten() {
                span.set_attribute(attribute.clone());
            }
            if let Some(SessionId(id)) = observation.cx.get::<SessionId>() {
                span.set_attribute(KeyValue::new("session.id", id.clone()));
            }
        }
        observation
    }

    /// Group this observation and everything under it into a Langfuse
    /// session, e.g. one assistant conversation.
    #[must_use]
    pub fn in_session(mut self, id: &str) -> Self {
        if self.is_recording() {
            self.cx
                .span()
                .set_attribute(KeyValue::new("session.id", id.to_string()));
        }
        self.cx = self.cx.with_value(SessionId(id.to_string()));
        self
    }

    pub fn is_recording(&self) -> bool {
        self.cx.span().is_recording()
    }

    /// Run `future` with this observation as the parent of the ones it starts.
    pub fn instrument<F: Future>(&self, future: F) -> impl Future<Output = F::Output> {
        future.with_context(self.cx.clone())
    }

    pub fn input(&self, value: impl FnOnce() -> Value) {
        self.json("langfuse.observation.input", value);
    }

    pub fn output(&self, value: impl FnOnce() -> Value) {
        self.settled.store(true, Ordering::Relaxed);
        self.json("langfuse.observation.output", value);
    }

    /// A filterable metadata key on this observation.
    pub fn metadata(&self, key: &str, value: impl Into<String>) {
        if self.is_recording() {
            self.cx.span().set_attribute(KeyValue::new(
                format!("langfuse.observation.metadata.{key}"),
                value.into(),
            ));
        }
    }

    pub fn fail(&self, message: &str) {
        self.settled.store(true, Ordering::Relaxed);
        self.level("ERROR", message);
    }

    fn model(&self, name: &str) {
        if self.is_recording() {
            self.cx.span().set_attribute(KeyValue::new(
                "langfuse.observation.model.name",
                name.to_string(),
            ));
        }
    }

    /// Unset parameters are left out: Langfuse shows a null as `"null"`.
    fn model_parameters(&self, value: impl FnOnce() -> Value) {
        self.json("langfuse.observation.model.parameters", || match value() {
            Value::Object(mut parameters) => {
                parameters.retain(|_, value| !value.is_null());
                Value::Object(parameters)
            }
            other => other,
        });
    }

    /// Langfuse stores usage keys as given and counts each token once, so
    /// the buckets must not overlap. Anthropic's `input_tokens` already
    /// leaves out cache reads and writes; the names match Langfuse's own
    /// price table.
    fn usage(&self, usage: &TokenUsage) {
        self.json("langfuse.observation.usage_details", || {
            let mut details = Map::new();
            details.insert("input".into(), usage.input_tokens.into());
            details.insert("output".into(), usage.output_tokens.into());
            if usage.cache_read_input_tokens > 0 {
                details.insert(
                    "input_cache_read".into(),
                    usage.cache_read_input_tokens.into(),
                );
            }
            if usage.cache_creation_input_tokens > 0 {
                details.insert(
                    "input_cache_creation".into(),
                    usage.cache_creation_input_tokens.into(),
                );
            }
            Value::Object(details)
        });
    }

    fn level(&self, level: &'static str, message: &str) {
        if self.is_recording() {
            let span = self.cx.span();
            span.set_attribute(KeyValue::new("langfuse.observation.level", level));
            span.set_status(Status::error(message.to_string()));
        }
    }

    fn json(&self, key: &'static str, value: impl FnOnce() -> Value) {
        if self.is_recording() {
            self.cx
                .span()
                .set_attribute(KeyValue::new(key, value().to_string()));
        }
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        if !self.settled.load(Ordering::Relaxed) {
            self.level(
                "WARNING",
                "stopped before it finished (timed out or cancelled)",
            );
        }
        self.cx.span().end();
    }
}

// --- What the llm crate records ------------------------------------------

/// The span around one feature's call, named by its config key so a trace
/// lines up with `[llm.overrides.<key>]` and `mxr llm status`. Route
/// fallbacks and retries show as generations under it.
pub(crate) fn feature(feature: Option<LlmFeature>) -> Observation {
    Observation::start(feature.map_or("complete", LlmFeature::key), Kind::Span)
}

pub(crate) fn completion_input(messages: &[ChatMessage]) -> Value {
    json!(messages)
}

/// An `f32` setting as written: widened straight to JSON's `f64`, 0.1
/// would show as 0.10000000149011612.
pub(crate) fn parameter_f32(value: Option<f32>) -> Value {
    value
        .and_then(|value| value.to_string().parse::<f64>().ok())
        .map_or(Value::Null, Value::from)
}

/// Tool results are JSON text more often than not; parsed, Langfuse shows
/// them as an object instead of an escaped string.
pub fn text_or_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

pub(crate) fn chat_input(messages: &[ToolChatMessage]) -> Value {
    Value::Array(openai_messages(messages.to_vec()))
}

pub(crate) fn finish_completion(span: &Observation, result: &Result<CompletionResponse, LlmError>) {
    match result {
        Ok(response) => span.output(|| json!(response.content)),
        Err(error) => span.fail(&error.to_string()),
    }
}

pub(crate) fn finish_chat(span: &Observation, result: &Result<ToolChatResponse, LlmError>) {
    match result {
        Ok(response) => span.output(|| chat_message(response, None)),
        Err(error) => span.fail(&error.to_string()),
    }
}

/// One request to a model. `endpoint` is the base URL, never a key.
pub(crate) fn generation(
    name: &'static str,
    model: &str,
    endpoint: &str,
    input: impl FnOnce() -> Value,
    parameters: impl FnOnce() -> Value,
) -> Observation {
    let span = Observation::start(name, Kind::Generation);
    span.model(model);
    span.metadata("endpoint", endpoint.to_string());
    span.input(input);
    span.model_parameters(parameters);
    span
}

/// What one completion request returned. `thinking` is the model's
/// reasoning, when the reply carried it.
pub(crate) fn finish_completion_attempt(
    span: &Observation,
    response: &CompletionResponse,
    usage: Option<&TokenUsage>,
    thinking: Option<&str>,
) {
    if let Some(usage) = usage {
        span.usage(usage);
    }
    span.model(&response.model);
    finish_reason(span, response.finish_reason.as_deref());
    span.output(|| assistant_message(json!(response.content), thinking));
}

/// A reply the provider sent but mxr couldn't use (empty, refused). Its
/// usage and thinking still show where the budget went.
pub(crate) fn fail_attempt(
    span: &Observation,
    error: &LlmError,
    usage: Option<&TokenUsage>,
    thinking: Option<&str>,
) {
    if let Some(usage) = usage {
        span.usage(usage);
    }
    if thinking.is_some() {
        span.output(|| assistant_message(json!(""), thinking));
    }
    span.fail(&error.to_string());
}

/// What one tool-calling request returned.
pub(crate) fn finish_chat_attempt(
    span: &Observation,
    result: &Result<ToolChatResponse, AttemptError>,
) {
    match result {
        Ok(response) => {
            span.model(&response.model);
            if let Some(usage) = &response.usage {
                span.usage(usage);
            }
            finish_reason(span, response.finish_reason.as_deref());
            span.output(|| chat_message(response, anthropic_thinking(response).as_deref()));
        }
        Err(attempt) => span.fail(&attempt.error.to_string()),
    }
}

fn finish_reason(span: &Observation, reason: Option<&str>) {
    if let Some(reason) = reason {
        span.metadata("finish_reason", reason.to_string());
    }
}

/// The reply as an OpenAI-format assistant message, which Langfuse renders
/// as a conversation turn with its tool calls as cards.
fn chat_message(response: &ToolChatResponse, thinking: Option<&str>) -> Value {
    let message = openai_messages(vec![response.to_message()])
        .into_iter()
        .next()
        .unwrap_or(Value::Null);
    match (message, thinking) {
        (Value::Object(mut message), Some(thinking)) => {
            message.insert("reasoning_content".into(), json!(thinking));
            Value::Object(message)
        }
        (message, _) => message,
    }
}

fn assistant_message(content: Value, thinking: Option<&str>) -> Value {
    let mut message = json!({"role": "assistant", "content": content});
    if let Some(thinking) = thinking.filter(|text| !text.trim().is_empty()) {
        message["reasoning_content"] = json!(thinking);
    }
    message
}

/// Claude's thinking, from the turn kept for replay.
fn anthropic_thinking(response: &ToolChatResponse) -> Option<String> {
    let blocks = response.provider_turn.as_ref()?.content.as_array()?;
    let thinking: Vec<&str> = blocks
        .iter()
        .filter(|block| block["type"] == "thinking")
        .filter_map(|block| block["thinking"].as_str())
        .filter(|text| !text.trim().is_empty())
        .collect();
    (!thinking.is_empty()).then(|| thinking.join("\n\n"))
}

fn openai_messages(messages: Vec<ToolChatMessage>) -> Vec<Value> {
    let request = ToolChatRequest {
        messages,
        tools: Vec::new(),
        max_tokens: None,
    };
    tool_chat::openai_body("", &request, OpenAiDialect::default()).messages
}
