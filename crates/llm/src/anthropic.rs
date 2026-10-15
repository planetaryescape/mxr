//! Anthropic's Messages API over plain HTTP. There is no official Rust SDK,
//! and Anthropic's OpenAI-compatible endpoint drops features this relies on,
//! so the request and reply are mapped here.

use crate::failure::{classify_status, parse_retry_after, AttemptError};
use crate::health::{FailureReason, RouteFailure};
use crate::route::ApiKey;
use crate::tool_chat::{
    ProviderTurn, TokenUsage, ToolCall, ToolChatMessage, ToolChatRequest, ToolChatResponse,
};
use crate::{endpoint, ChatRole, CompletionRequest, CompletionResponse, LlmError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub(crate) const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// `max_tokens` is mandatory on this API; used when the caller sets none.
const DEFAULT_MAX_TOKENS: u32 = 1024;
/// Longest error message kept from an error reply.
const ERROR_MESSAGE_LIMIT: usize = 200;

/// Claude 5.x thinks adaptively and thinking tokens count against
/// `max_tokens`. mxr's features are short extractions and summaries, so
/// thinking is kept at the lowest effort.
const EFFORT: &str = "low";
/// The assistant plans multi-step tool use for a person who is waiting:
/// more thought than an extraction, less than a coding agent.
const ASSISTANT_EFFORT: &str = "medium";
/// Room for thinking plus a long answer in one non-streamed reply.
const ASSISTANT_MAX_TOKENS: u32 = 16_000;
/// Provider tag on replayed turns.
pub(crate) const PROVIDER_TAG: &str = "anthropic";

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    /// Absent when the request has no system prompt. Claude 5.x rejects
    /// non-default `temperature`, `top_p` and `top_k`, so this request type
    /// has no field for them at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<WireMessage<'a>>,
    output_config: OutputConfig,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct OutputConfig {
    effort: &'static str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
}

/// Only `text` blocks are the answer. `thinking` and anything newer are
/// skipped.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    message: String,
}

pub(crate) struct AnthropicClient {
    base_url: String,
    request_timeout: Duration,
    client: Result<reqwest::Client, String>,
}

impl AnthropicClient {
    pub(crate) fn new(base_url: &str, request_timeout: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            request_timeout,
            client: endpoint::http_client(base_url, request_timeout),
        }
    }

    /// Send one request with the key as it is right now. `timeout` bounds
    /// this attempt when it is shorter than the client's own.
    pub(crate) async fn complete(
        &self,
        key: &ApiKey,
        model: &str,
        req: &CompletionRequest,
        timeout: Option<Duration>,
    ) -> Result<CompletionResponse, AttemptError> {
        let body = build_request(model, req)?;
        let reply: MessagesResponse = self.post(key, &body, timeout).await?;
        interpret(reply, model)
    }

    /// One tool-calling turn. The reply's content blocks are kept whole so
    /// they can be replayed unchanged (thinking blocks must be).
    pub(crate) async fn chat(
        &self,
        key: &ApiKey,
        model: &str,
        req: &ToolChatRequest,
        timeout: Option<Duration>,
    ) -> Result<ToolChatResponse, AttemptError> {
        let body = build_tool_request(model, req)?;
        let reply: ToolMessagesResponse = self.post(key, &body, timeout).await?;
        interpret_tool_reply(reply, model)
    }

    /// [`Self::chat`] with the reply streamed and rebuilt block by block.
    pub(crate) async fn chat_stream(
        &self,
        key: &ApiKey,
        model: &str,
        req: &ToolChatRequest,
        timeout: Option<Duration>,
        on_text: crate::stream::TextSink<'_>,
    ) -> Result<ToolChatResponse, AttemptError> {
        let mut body = build_tool_request(model, req)?;
        body.stream = true;
        let response = self.send_checked(key, &body, timeout).await?;
        let mut folded = crate::stream::AnthropicStream::default();
        crate::stream::read_events(response, |data| folded.fold(data, on_text)).await?;
        let reply: ToolMessagesResponse =
            serde_json::from_value(folded.finish()).map_err(|error| {
                AttemptError::route(
                    LlmError::Other(format!("Anthropic reply could not be read: {error}")),
                    RouteFailure::Cooling {
                        reason: FailureReason::plain("unreadable reply"),
                        retry_after: None,
                    },
                )
            })?;
        interpret_tool_reply(reply, model)
    }

    async fn post<B: Serialize, R: serde::de::DeserializeOwned>(
        &self,
        key: &ApiKey,
        body: &B,
        timeout: Option<Duration>,
    ) -> Result<R, AttemptError> {
        let response = self.send_checked(key, body, timeout).await?;
        response.json::<R>().await.map_err(|error| {
            AttemptError::route(
                LlmError::Other(format!("Anthropic reply could not be read: {error}")),
                RouteFailure::Cooling {
                    reason: FailureReason::plain("unreadable reply"),
                    retry_after: None,
                },
            )
        })
    }

    /// Send one request and return the response once its status says it
    /// succeeded, for reading whole or as a stream.
    async fn send_checked<B: Serialize>(
        &self,
        key: &ApiKey,
        body: &B,
        timeout: Option<Duration>,
    ) -> Result<reqwest::Response, AttemptError> {
        let client = self
            .client
            .as_ref()
            .map_err(|error| AttemptError::request(LlmError::Other(error.clone())))?;
        let mut key_header =
            reqwest::header::HeaderValue::from_str(key.expose()).map_err(|_| {
                AttemptError::route(
                    LlmError::Other(
                        "the Anthropic API key has characters a header can't carry".to_string(),
                    ),
                    RouteFailure::Broken {
                        reason: FailureReason::plain("unusable key"),
                    },
                )
            })?;
        // Keeps the key out of reqwest's debug output.
        key_header.set_sensitive(true);
        let mut request = client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", key_header)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(body);
        let budget = timeout.map_or(self.request_timeout, |t| t.min(self.request_timeout));
        request = request.timeout(budget);

        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                AttemptError::timed_out(budget)
            } else {
                AttemptError::unreachable(redact(error.to_string(), key))
            }
        })?;

        let status = response.status();
        if status.is_redirection() {
            return Err(AttemptError::route(
                LlmError::Other(format!(
                    "Anthropic redirected the request ({status}); mxr doesn't follow LLM \
                     redirects, so the prompt was not resent"
                )),
                RouteFailure::Broken {
                    reason: FailureReason::plain("unexpected redirect"),
                },
            ));
        }
        if !status.is_success() {
            let retry_after = parse_retry_after(
                response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok()),
            );
            let text = response.text().await.unwrap_or_default();
            let class = classify_status(status.as_u16(), retry_after, &text);
            let error = match status.as_u16() {
                401 => LlmError::Unauthorized,
                429 => LlmError::RateLimited {
                    retry_after_secs: retry_after.map_or(30, |after| after.as_secs()),
                },
                code => LlmError::Other(format!(
                    "Anthropic {code}: {}",
                    redact(describe_error(&text), key)
                )),
            };
            return Err(AttemptError { error, class });
        }

        Ok(response)
    }
}

/// System turns go in the top-level `system` field (the API has no system
/// role); the rest keep their order.
fn build_request<'a>(
    model: &'a str,
    req: &'a CompletionRequest,
) -> Result<MessagesRequest<'a>, AttemptError> {
    let system: Vec<&str> = req
        .messages
        .iter()
        .filter(|message| message.role == ChatRole::System)
        .map(|message| message.content.as_str())
        .collect();
    let messages: Vec<WireMessage<'a>> = req
        .messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                ChatRole::System => return None,
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
            };
            Some(WireMessage {
                role,
                content: &message.content,
            })
        })
        .collect();
    if messages.is_empty() {
        return Err(AttemptError::request(LlmError::Other(
            "the request has no user message".to_string(),
        )));
    }
    Ok(MessagesRequest {
        model,
        max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        system: (!system.is_empty()).then(|| system.join("\n\n")),
        messages,
        output_config: OutputConfig { effort: EFFORT },
    })
}

/// A refusal or an empty answer is about this request; the API worked.
fn interpret(
    reply: MessagesResponse,
    requested_model: &str,
) -> Result<CompletionResponse, AttemptError> {
    if reply.stop_reason.as_deref() == Some("refusal") {
        return Err(AttemptError::request(LlmError::Other(
            "the model declined to answer this request".to_string(),
        )));
    }
    let content: String = reply
        .content
        .into_iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text),
            ContentBlock::Other => None,
        })
        .collect();
    if content.trim().is_empty() {
        if reply.stop_reason.as_deref() == Some("max_tokens") {
            tracing::warn!(
                model = requested_model,
                "Anthropic spent the whole token budget before writing an answer"
            );
        }
        return Err(AttemptError::request(LlmError::Empty));
    }
    Ok(CompletionResponse {
        content,
        model: if reply.model.trim().is_empty() {
            requested_model.to_string()
        } else {
            reply.model
        },
        finish_reason: reply.stop_reason,
    })
}

#[derive(Serialize)]
struct ToolMessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<Value>,
    output_config: OutputConfig,
    /// Automatic caching: the breakpoint follows the end of the history, so
    /// the tools, system prompt and earlier turns are read from cache.
    cache_control: Value,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Deserialize)]
struct ToolMessagesResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    content: Vec<Value>,
    stop_reason: Option<String>,
    #[serde(default)]
    usage: Option<TokenUsage>,
}

/// History must reach the API append-only: an assistant turn this provider
/// produced goes back exactly as it came, and tool results for one turn
/// share one user message.
fn build_tool_request<'a>(
    model: &'a str,
    req: &ToolChatRequest,
) -> Result<ToolMessagesRequest<'a>, AttemptError> {
    let mut system = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    let push_user_blocks = |messages: &mut Vec<Value>, blocks: Vec<Value>| {
        if let Some(last) = messages.last_mut() {
            if last["role"] == "user" {
                if let Some(content) = last["content"].as_array_mut() {
                    content.extend(blocks);
                    return;
                }
            }
        }
        messages.push(json!({"role": "user", "content": blocks}));
    };
    for message in &req.messages {
        match message {
            ToolChatMessage::System { content } => system.push(content.as_str()),
            ToolChatMessage::User { content } => {
                push_user_blocks(
                    &mut messages,
                    vec![json!({"type": "text", "text": content})],
                );
            }
            ToolChatMessage::Tool {
                call_id,
                content,
                is_error,
            } => push_user_blocks(
                &mut messages,
                vec![json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                    "is_error": is_error,
                })],
            ),
            ToolChatMessage::Assistant {
                text,
                tool_calls,
                provider_turn,
            } => {
                let content = match provider_turn {
                    Some(turn) if turn.provider == PROVIDER_TAG => turn.content.clone(),
                    _ => {
                        let mut blocks = Vec::new();
                        if !text.is_empty() {
                            blocks.push(json!({"type": "text", "text": text}));
                        }
                        for call in tool_calls {
                            blocks.push(json!({
                                "type": "tool_use",
                                "id": call.id,
                                "name": call.name,
                                "input": call.arguments,
                            }));
                        }
                        Value::Array(blocks)
                    }
                };
                messages.push(json!({"role": "assistant", "content": content}));
            }
        }
    }
    if !messages.iter().any(|message| message["role"] == "user") {
        return Err(AttemptError::request(LlmError::Other(
            "the request has no user message".to_string(),
        )));
    }
    let tools = req
        .tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": crate::tool_chat::portable_schema(&tool.input_schema),
            })
        })
        .collect();
    Ok(ToolMessagesRequest {
        model,
        max_tokens: req.max_tokens.unwrap_or(ASSISTANT_MAX_TOKENS),
        system: (!system.is_empty()).then(|| system.join("\n\n")),
        messages,
        tools,
        output_config: OutputConfig {
            effort: ASSISTANT_EFFORT,
        },
        cache_control: json!({"type": "ephemeral"}),
        stream: false,
    })
}

fn interpret_tool_reply(
    reply: ToolMessagesResponse,
    requested_model: &str,
) -> Result<ToolChatResponse, AttemptError> {
    if reply.stop_reason.as_deref() == Some("refusal") {
        return Err(AttemptError::request(LlmError::Other(
            "the model declined to answer this request".to_string(),
        )));
    }
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for block in &reply.content {
        match block["type"].as_str() {
            Some("text") => text.push_str(block["text"].as_str().unwrap_or_default()),
            Some("tool_use") => tool_calls.push(ToolCall {
                id: block["id"].as_str().unwrap_or_default().to_string(),
                name: block["name"].as_str().unwrap_or_default().to_string(),
                arguments: block["input"].clone(),
            }),
            _ => {}
        }
    }
    if text.trim().is_empty() && tool_calls.is_empty() {
        return Err(AttemptError::request(LlmError::Empty));
    }
    Ok(ToolChatResponse {
        text,
        tool_calls,
        model: if reply.model.trim().is_empty() {
            requested_model.to_string()
        } else {
            reply.model
        },
        finish_reason: reply.stop_reason,
        usage: reply.usage,
        provider_turn: Some(ProviderTurn {
            provider: PROVIDER_TAG.to_string(),
            content: Value::Array(reply.content),
        }),
    })
}

/// The error's type and a short message. The reply is not echoed whole, in
/// case a provider ever quotes the request in it.
fn describe_error(body: &str) -> String {
    match serde_json::from_str::<ErrorEnvelope>(body) {
        Ok(ErrorEnvelope { error }) => {
            let message: String = error.message.chars().take(ERROR_MESSAGE_LIMIT).collect();
            format!("{}: {message}", error.kind)
        }
        Err(_) => "no error details".to_string(),
    }
}

fn redact(text: String, key: &ApiKey) -> String {
    let key = key.expose();
    if key.len() < 8 {
        return text;
    }
    text.replace(key, "<redacted>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatMessage;
    use serde_json::json;

    fn request(messages: Vec<ChatMessage>) -> CompletionRequest {
        CompletionRequest {
            messages,
            max_tokens: Some(400),
            temperature: Some(0.1),
        }
    }

    fn wire(req: &CompletionRequest) -> serde_json::Value {
        serde_json::to_value(build_request("claude-haiku-5-5", req).unwrap()).unwrap()
    }

    fn reply(content: serde_json::Value, stop_reason: &str) -> MessagesResponse {
        serde_json::from_value(json!({
            "model": "claude-haiku-5-5",
            "content": content,
            "stop_reason": stop_reason,
        }))
        .unwrap()
    }

    #[test]
    fn system_turns_move_to_the_system_field_and_the_rest_keep_their_order() {
        let body = wire(&request(vec![
            ChatMessage::system("Be brief."),
            ChatMessage::user("Summarise."),
            ChatMessage::assistant("Sure."),
            ChatMessage::user("Go."),
            ChatMessage::system("Only JSON."),
        ]));
        assert_eq!(body["system"], "Be brief.\n\nOnly JSON.");
        assert_eq!(
            body["messages"],
            json!([
                {"role": "user", "content": "Summarise."},
                {"role": "assistant", "content": "Sure."},
                {"role": "user", "content": "Go."},
            ])
        );
        assert_eq!(body["model"], "claude-haiku-5-5");
        assert_eq!(body["max_tokens"], 400);
        assert_eq!(body["output_config"], json!({"effort": "low"}));
    }

    #[test]
    fn a_request_never_carries_sampling_parameters() {
        let body = wire(&request(vec![ChatMessage::user("hi")]));
        let object = body.as_object().unwrap();
        for field in ["temperature", "top_p", "top_k", "stream"] {
            assert!(!object.contains_key(field), "{field} must not be sent");
        }
        assert!(!object.contains_key("system"));
    }

    #[test]
    fn a_request_without_a_user_turn_is_refused_before_any_network_call() {
        let req = request(vec![ChatMessage::system("only a system prompt")]);
        let error = build_request("claude-haiku-5-5", &req).err().unwrap();
        assert!(matches!(error.class, crate::failure::ErrorClass::Request));
    }

    #[test]
    fn max_tokens_defaults_when_the_caller_sets_none() {
        let mut req = request(vec![ChatMessage::user("hi")]);
        req.max_tokens = None;
        assert_eq!(wire(&req)["max_tokens"], DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn the_answer_is_the_text_blocks_and_thinking_is_ignored() {
        let response = interpret(
            reply(
                json!([
                    {"type": "thinking", "thinking": "let me think", "signature": "abc"},
                    {"type": "text", "text": "Hello"},
                    {"type": "redacted_thinking", "data": "xyz"},
                    {"type": "text", "text": ", world"},
                ]),
                "end_turn",
            ),
            "claude-haiku-5-5",
        )
        .unwrap();
        assert_eq!(response.content, "Hello, world");
        assert_eq!(response.model, "claude-haiku-5-5");
        assert_eq!(response.finish_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn a_refusal_fails_the_request_not_the_route() {
        let error = interpret(
            reply(json!([{"type": "text", "text": "I can't help"}]), "refusal"),
            "claude-haiku-5-5",
        )
        .err()
        .unwrap();
        assert!(matches!(error.class, crate::failure::ErrorClass::Request));
        assert!(matches!(error.error, LlmError::Other(_)));
    }

    #[test]
    fn thinking_that_used_the_whole_budget_is_an_empty_answer_for_this_request() {
        let error = interpret(
            reply(
                json!([{"type": "thinking", "thinking": "hmm", "signature": "s"}]),
                "max_tokens",
            ),
            "claude-haiku-5-5",
        )
        .err()
        .unwrap();
        assert!(matches!(error.class, crate::failure::ErrorClass::Request));
        assert!(matches!(error.error, LlmError::Empty));
    }

    #[test]
    fn a_cut_off_answer_with_text_is_returned_with_its_stop_reason() {
        let response = interpret(
            reply(json!([{"type": "text", "text": "{\"a\": 1"}]), "max_tokens"),
            "claude-haiku-5-5",
        )
        .unwrap();
        assert_eq!(response.finish_reason.as_deref(), Some("max_tokens"));
    }

    #[test]
    fn error_descriptions_keep_type_and_a_short_message_only() {
        let long = "x".repeat(1000);
        let body =
            json!({"type": "error", "error": {"type": "invalid_request_error", "message": long}})
                .to_string();
        let described = describe_error(&body);
        assert!(described.starts_with("invalid_request_error: "));
        assert!(described.len() < 300);
        assert_eq!(
            describe_error("<html>proxy page with a prompt</html>"),
            "no error details"
        );
    }

    fn tool_request(messages: Vec<ToolChatMessage>) -> serde_json::Value {
        let req = ToolChatRequest {
            messages,
            tools: vec![crate::ToolSpec {
                name: "mxr_search".into(),
                description: "Search mail".into(),
                input_schema: json!({"type": "object"}),
            }],
            max_tokens: None,
        };
        serde_json::to_value(build_tool_request("claude-opus-5-5", &req).unwrap()).unwrap()
    }

    #[test]
    fn a_turn_anthropic_produced_is_replayed_byte_for_byte() {
        let raw = json!([
            {"type": "thinking", "thinking": "", "signature": "sig"},
            {"type": "tool_use", "id": "t1", "name": "mxr_search", "input": {"query": "x"}},
        ]);
        let body = tool_request(vec![
            ToolChatMessage::User {
                content: "find x".into(),
            },
            ToolChatMessage::Assistant {
                text: String::new(),
                tool_calls: vec![ToolCall {
                    id: "t1".into(),
                    name: "mxr_search".into(),
                    arguments: json!({"query": "x"}),
                }],
                provider_turn: Some(ProviderTurn {
                    provider: PROVIDER_TAG.into(),
                    content: raw.clone(),
                }),
            },
            ToolChatMessage::Tool {
                call_id: "t1".into(),
                content: "[]".into(),
                is_error: false,
            },
        ]);
        assert_eq!(body["messages"][1]["content"], raw);
    }

    #[test]
    fn parallel_tool_results_and_a_following_user_turn_share_one_user_message() {
        let body = tool_request(vec![
            ToolChatMessage::System {
                content: "sys".into(),
            },
            ToolChatMessage::User {
                content: "go".into(),
            },
            ToolChatMessage::Assistant {
                text: "Looking.".into(),
                tool_calls: vec![
                    ToolCall {
                        id: "a".into(),
                        name: "mxr_search".into(),
                        arguments: json!({}),
                    },
                    ToolCall {
                        id: "b".into(),
                        name: "mxr_search".into(),
                        arguments: json!({}),
                    },
                ],
                provider_turn: None,
            },
            ToolChatMessage::Tool {
                call_id: "a".into(),
                content: "1".into(),
                is_error: false,
            },
            ToolChatMessage::Tool {
                call_id: "b".into(),
                content: "no".into(),
                is_error: true,
            },
            ToolChatMessage::User {
                content: "and then?".into(),
            },
        ]);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages[1]["content"][0],
            json!({"type": "text", "text": "Looking."})
        );
        assert_eq!(messages[1]["content"][2]["id"], "b");
        let results = messages[2]["content"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[1]["is_error"], true);
        assert_eq!(results[2]["text"], "and then?");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["cache_control"], json!({"type": "ephemeral"}));
        assert_eq!(body["output_config"], json!({"effort": ASSISTANT_EFFORT}));
        assert_eq!(body["tools"][0]["input_schema"], json!({"type": "object"}));
    }

    #[test]
    fn a_tool_reply_keeps_its_raw_content_for_replay() {
        let reply: ToolMessagesResponse = serde_json::from_value(json!({
            "model": "claude-opus-5-5",
            "content": [
                {"type": "thinking", "thinking": "", "signature": "s"},
                {"type": "text", "text": "Checking."},
                {"type": "tool_use", "id": "t", "name": "mxr_status", "input": {}},
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 7}
        }))
        .unwrap();
        let response = interpret_tool_reply(reply, "claude-opus-5-5").unwrap();
        assert_eq!(response.text, "Checking.");
        assert_eq!(response.tool_calls[0].name, "mxr_status");
        assert_eq!(response.usage.unwrap().cache_read_input_tokens, 7);
        let turn = response.provider_turn.unwrap();
        assert_eq!(turn.content.as_array().unwrap().len(), 3);
    }

    #[test]
    fn redaction_removes_the_key_from_text() {
        let key = ApiKey::new("sk-ant-api03-abcdefgh");
        assert_eq!(
            redact("bad key sk-ant-api03-abcdefgh here".to_string(), &key),
            "bad key <redacted> here"
        );
    }
}
