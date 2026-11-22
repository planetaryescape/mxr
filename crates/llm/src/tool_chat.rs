//! Multi-turn chat with tool calling, for the assistant's agent loop.
//!
//! Kept apart from [`crate::CompletionRequest`] because every one-shot
//! feature builds that type by hand and has no use for tools. Both share the
//! routing, route health and observations through [`crate::LlmProvider`].

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One tool the model may call. `input_schema` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// A tool call the model asked for. `arguments` is the parsed JSON object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// A provider's own copy of an assistant turn, replayed unchanged on the
/// next request to the same provider. Anthropic requires thinking blocks to
/// come back exactly as they were sent and rejects edited history, so the
/// rebuilt `text` + `tool_calls` form is only good for other providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTurn {
    /// `"anthropic"` today.
    pub provider: String,
    /// The turn's content exactly as the provider returned it.
    pub content: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ToolChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        #[serde(default)]
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_turn: Option<ProviderTurn>,
    },
    Tool {
        call_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ToolChatRequest {
    pub messages: Vec<ToolChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct ToolChatResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub model: String,
    pub finish_reason: Option<String>,
    pub usage: Option<TokenUsage>,
    /// Set by providers that need their own turn replayed (see
    /// [`ProviderTurn`]).
    pub provider_turn: Option<ProviderTurn>,
}

impl ToolChatResponse {
    /// The assistant message to append to the history.
    pub fn to_message(&self) -> ToolChatMessage {
        ToolChatMessage::Assistant {
            text: self.text.clone(),
            tool_calls: self.tool_calls.clone(),
            provider_turn: self.provider_turn.clone(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct OpenAiToolChatBody<'a> {
    pub model: &'a str,
    pub messages: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    pub stream: bool,
    /// Asks a stream to end with a usage chunk, which the trace records.
    /// Only tracing builds send it, so a shipped build's request is as it
    /// always was.
    #[cfg(feature = "langfuse")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<Value>,
}

/// How OpenAI's own API, and OpenRouter in front of it, differ from other
/// chat-completions servers.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OpenAiDialect {
    /// OpenAI's current models: name the token cap `max_completion_tokens`
    /// and send no temperature, which they reject unless it is the default.
    pub completion_token_cap: bool,
    /// Turn reasoning off on tool-calling requests: these models take tool
    /// calls on Chat Completions only with reasoning off, and default to
    /// medium.
    pub no_reasoning: bool,
    /// Say it the OpenRouter way, `reasoning: {"effort": "none"}`, instead of
    /// OpenAI's top-level `reasoning_effort`.
    pub nested_reasoning: bool,
    /// Add room for thinking to a completion's token cap. Set where no router
    /// has done it already: a `gpt-*` model on a server of the user's own.
    /// Tool-calling requests don't get it, as on the routed path: the models
    /// that take tools here run them with reasoning off.
    pub thinking_headroom: bool,
}

/// The request in chat-completions form.
pub(crate) fn openai_body<'a>(
    model: &'a str,
    req: &ToolChatRequest,
    dialect: OpenAiDialect,
) -> OpenAiToolChatBody<'a> {
    let completion_token_cap = dialect.completion_token_cap;
    let messages = req
        .messages
        .iter()
        .map(|message| match message {
            ToolChatMessage::System { content } => {
                serde_json::json!({"role": "system", "content": content})
            }
            ToolChatMessage::User { content } => {
                serde_json::json!({"role": "user", "content": content})
            }
            ToolChatMessage::Assistant {
                text, tool_calls, ..
            } => {
                let mut value = serde_json::json!({
                    "role": "assistant",
                    "content": if text.is_empty() { Value::Null } else { Value::String(text.clone()) },
                });
                if !tool_calls.is_empty() {
                    value["tool_calls"] = tool_calls
                        .iter()
                        .map(|call| {
                            serde_json::json!({
                                "id": call.id,
                                "type": "function",
                                "function": {
                                    "name": call.name,
                                    "arguments": call.arguments.to_string(),
                                },
                            })
                        })
                        .collect();
                }
                value
            }
            ToolChatMessage::Tool {
                call_id, content, ..
            } => serde_json::json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": content,
            }),
        })
        .collect();
    let tools = req
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": portable_schema(&tool.input_schema),
                },
            })
        })
        .collect();
    OpenAiToolChatBody {
        model,
        messages,
        tools,
        max_tokens: req.max_tokens.filter(|_| !completion_token_cap),
        max_completion_tokens: req.max_tokens.filter(|_| completion_token_cap),
        reasoning_effort: (dialect.no_reasoning && !dialect.nested_reasoning).then_some("none"),
        reasoning: (dialect.no_reasoning && dialect.nested_reasoning)
            .then(|| serde_json::json!({"effort": "none"})),
        stream: false,
        #[cfg(feature = "langfuse")]
        stream_options: None,
    }
}

#[derive(Deserialize)]
pub(crate) struct OpenAiToolChatReply {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub choices: Vec<OpenAiToolChoice>,
    #[serde(default)]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiToolChoice {
    pub message: OpenAiToolMessage,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiToolMessage {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<OpenAiWireToolCall>,
    #[cfg(feature = "langfuse")]
    #[serde(default)]
    pub reasoning: Option<String>,
    #[cfg(feature = "langfuse")]
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

#[cfg(feature = "langfuse")]
impl OpenAiToolChatReply {
    /// The reply's thinking, for the trace. Ollama calls it `reasoning`,
    /// DeepSeek-style servers `reasoning_content`.
    pub(crate) fn thinking(&self) -> Option<String> {
        let message = &self.choices.first()?.message;
        [&message.reasoning, &message.reasoning_content]
            .into_iter()
            .flatten()
            .find(|text| !text.trim().is_empty())
            .cloned()
    }
}

#[derive(Deserialize)]
pub(crate) struct OpenAiWireToolCall {
    pub id: String,
    pub function: OpenAiWireFunction,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiWireFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
}

/// Map a chat-completions reply. Arguments that aren't valid JSON are kept
/// as `{"_unparsed": "<raw>"}` so the loop can hand the model an error
/// result for that call instead of dropping it: every call in history needs
/// a result.
pub(crate) fn openai_reply(
    reply: OpenAiToolChatReply,
    requested_model: &str,
) -> Option<ToolChatResponse> {
    let choice = reply.choices.into_iter().next()?;
    let tool_calls = choice
        .message
        .tool_calls
        .into_iter()
        .map(|call| ToolCall {
            id: call.id,
            name: call.function.name,
            arguments: parse_arguments(&call.function.arguments),
        })
        .collect();
    Some(ToolChatResponse {
        text: choice.message.content.unwrap_or_default(),
        tool_calls,
        model: if reply.model.trim().is_empty() {
            requested_model.to_string()
        } else {
            reply.model
        },
        finish_reason: choice.finish_reason,
        usage: reply.usage.map(|usage| TokenUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            ..TokenUsage::default()
        }),
        provider_turn: None,
    })
}

/// A tool's JSON Schema in the subset every provider parses. JSON Schema
/// allows `true` and `false` as whole schemas (schemars writes `true` for a
/// free-form JSON field), and Ollama rejects the request when a property is
/// a bool rather than an object. Local `$ref`s into `$defs` are inlined:
/// small models don't follow them, and one that never saw an enum's values
/// invents its own. `$schema` is dropped too: it names a dialect, and some
/// servers refuse keys they don't know.
pub(crate) fn portable_schema(schema: &Value) -> Value {
    let empty = serde_json::Map::new();
    let defs = schema
        .get("$defs")
        .or_else(|| schema.get("definitions"))
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    portable(schema, defs, 0)
}

/// How deep `$ref`s are followed; past it (a recursive type) the reference
/// becomes an open object.
const MAX_REF_DEPTH: u8 = 8;

fn portable(schema: &Value, defs: &serde_json::Map<String, Value>, depth: u8) -> Value {
    match schema {
        Value::Bool(true) => Value::Object(serde_json::Map::new()),
        Value::Bool(false) => serde_json::json!({"not": {}}),
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                let name = reference
                    .strip_prefix("#/$defs/")
                    .or_else(|| reference.strip_prefix("#/definitions/"));
                if let Some(target) = name.and_then(|name| defs.get(name)) {
                    if depth >= MAX_REF_DEPTH {
                        return serde_json::json!({"type": "object"});
                    }
                    let mut inlined = portable(target, defs, depth + 1);
                    if let Value::Object(target_map) = &mut inlined {
                        for (key, value) in map {
                            if key != "$ref" {
                                target_map.insert(key.clone(), portable(value, defs, depth));
                            }
                        }
                    }
                    return inlined;
                }
            }
            Value::Object(
                map.iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "$schema" | "$defs" | "definitions"))
                    .map(|(key, value)| {
                        let value = match key.as_str() {
                            "properties" | "patternProperties" => match value {
                                Value::Object(entries) => Value::Object(
                                    entries
                                        .iter()
                                        .map(|(name, sub)| {
                                            (name.clone(), portable(sub, defs, depth))
                                        })
                                        .collect(),
                                ),
                                other => other.clone(),
                            },
                            "items"
                            | "additionalProperties"
                            | "not"
                            | "contains"
                            | "propertyNames"
                            | "if"
                            | "then"
                            | "else"
                            | "anyOf"
                            | "oneOf"
                            | "allOf"
                            | "prefixItems" => match value {
                                Value::Array(list) => Value::Array(
                                    list.iter().map(|sub| portable(sub, defs, depth)).collect(),
                                ),
                                other => portable(other, defs, depth),
                            },
                            _ => value.clone(),
                        };
                        (key.clone(), value)
                    })
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

pub(crate) fn parse_arguments(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return Value::Object(serde_json::Map::new());
    }
    match serde_json::from_str::<Value>(raw) {
        Ok(value @ Value::Object(_)) => value,
        _ => serde_json::json!({ "_unparsed": raw }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> ToolChatRequest {
        ToolChatRequest {
            messages: vec![
                ToolChatMessage::System {
                    content: "sys".into(),
                },
                ToolChatMessage::User {
                    content: "hi".into(),
                },
                ToolChatMessage::Assistant {
                    text: String::new(),
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "mxr_search".into(),
                        arguments: json!({"query": "from:bank"}),
                    }],
                    provider_turn: None,
                },
                ToolChatMessage::Tool {
                    call_id: "c1".into(),
                    content: "[]".into(),
                    is_error: false,
                },
            ],
            tools: vec![ToolSpec {
                name: "mxr_search".into(),
                description: "Search".into(),
                input_schema: json!({"type": "object"}),
            }],
            max_tokens: Some(100),
        }
    }

    #[test]
    fn openai_body_carries_tool_calls_as_json_strings_and_tool_results_by_id() {
        let body =
            serde_json::to_value(openai_body("m", &request(), OpenAiDialect::default())).unwrap();
        assert_eq!(body["messages"][2]["content"], Value::Null);
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{\"query\":\"from:bank\"}"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "c1");
        assert_eq!(body["tools"][0]["function"]["name"], "mxr_search");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn unparseable_arguments_are_kept_so_the_call_still_gets_a_result() {
        let reply: OpenAiToolChatReply = serde_json::from_value(json!({
            "model": "m",
            "choices": [{
                "message": {"content": null, "tool_calls": [
                    {"id": "a", "function": {"name": "x", "arguments": "{not json"}},
                    {"id": "b", "function": {"name": "y", "arguments": ""}}
                ]},
                "finish_reason": "tool_calls"
            }]
        }))
        .unwrap();
        let response = openai_reply(reply, "m").unwrap();
        assert_eq!(
            response.tool_calls[0].arguments,
            json!({"_unparsed": "{not json"})
        );
        assert_eq!(response.tool_calls[1].arguments, json!({}));
        assert_eq!(response.text, "");
    }
}
