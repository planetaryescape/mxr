//! Streamed tool-chat replies: server-sent events read off the response,
//! folded into the same reply a non-streamed request returns, with the text
//! so far reported as it arrives.
//!
//! The accumulators are pure (events in, reply out) so they are tested
//! without a server. Anthropic's reply is rebuilt block by block, thinking
//! signatures included, because the turn is replayed verbatim later.

use crate::failure::AttemptError;
use crate::health::{FailureReason, RouteFailure};
use crate::tool_chat::{TokenUsage, ToolCall, ToolChatResponse};
use crate::LlmError;
use serde_json::{json, Value};

/// Called with the whole reply text so far, each time it grows.
pub type TextSink<'a> = crate::TextCallback<'a>;

/// Server-sent events split out of arbitrary byte chunks.
#[derive(Default)]
struct SseBuffer {
    pending: String,
}

impl SseBuffer {
    /// Feed bytes; return each complete event's data (multi-line data
    /// joined with newlines).
    fn push(&mut self, chunk: &str) -> Vec<String> {
        // Normalised over the whole buffer: a CRLF can straddle two chunks.
        self.pending.push_str(chunk);
        if self.pending.contains('\r') {
            self.pending = self.pending.replace("\r\n", "\n");
        }
        let mut events = Vec::new();
        while let Some(end) = self.pending.find("\n\n") {
            let raw: String = self.pending.drain(..end + 2).collect();
            let data: Vec<&str> = raw
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|data| data.strip_prefix(' ').unwrap_or(data))
                .collect();
            if !data.is_empty() {
                events.push(data.join("\n"));
            }
        }
        events
    }
}

/// Read the response's events into `fold` until it ends.
pub(crate) async fn read_events(
    mut response: reqwest::Response,
    mut fold: impl FnMut(&str) -> Result<(), AttemptError>,
) -> Result<(), AttemptError> {
    let mut buffer = SseBuffer::default();
    // Bytes of a UTF-8 character split across chunks wait for the rest.
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|error| {
            if error.is_timeout() {
                AttemptError::route(
                    LlmError::Other("the reply stopped arriving before it finished".into()),
                    RouteFailure::Cooling {
                        reason: FailureReason::plain("timed out"),
                        retry_after: None,
                    },
                )
            } else {
                AttemptError::unreachable(format!("the reply stream broke: {error}"))
            }
        })?;
        let Some(chunk) = chunk else { break };
        carry.extend_from_slice(&chunk);
        let valid = match std::str::from_utf8(&carry) {
            Ok(text) => text.len(),
            Err(error) => error.valid_up_to(),
        };
        let text = String::from_utf8_lossy(&carry[..valid]).into_owned();
        carry.drain(..valid);
        for data in buffer.push(&text) {
            fold(&data)?;
        }
    }
    // A last event without its blank line still counts.
    for data in buffer.push("\n\n") {
        fold(&data)?;
    }
    Ok(())
}

fn unreadable(detail: String) -> AttemptError {
    AttemptError::route(
        LlmError::Other(detail),
        RouteFailure::Cooling {
            reason: FailureReason::plain("unreadable reply"),
            retry_after: None,
        },
    )
}

// --- OpenAI chat-completions ---------------------------------------------

#[derive(Default)]
pub(crate) struct OpenAiStream {
    model: String,
    text: String,
    calls: Vec<(String, String, String)>,
    finish_reason: Option<String>,
    usage: Option<TokenUsage>,
}

impl OpenAiStream {
    pub(crate) fn fold(&mut self, data: &str, on_text: TextSink<'_>) -> Result<(), AttemptError> {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let event: Value = serde_json::from_str(data)
            .map_err(|error| unreadable(format!("a streamed chunk wasn't JSON: {error}")))?;
        if let Some(message) = event.pointer("/error/message").and_then(Value::as_str) {
            return Err(AttemptError::request(LlmError::Other(message.to_string())));
        }
        if let Some(model) = event["model"].as_str() {
            self.model = model.to_string();
        }
        if let Some(usage) = event.get("usage").filter(|usage| usage.is_object()) {
            self.usage = Some(TokenUsage {
                input_tokens: usage["prompt_tokens"].as_u64().unwrap_or_default(),
                output_tokens: usage["completion_tokens"].as_u64().unwrap_or_default(),
                ..TokenUsage::default()
            });
        }
        let Some(choice) = event["choices"].get(0) else {
            return Ok(());
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(reason.to_string());
        }
        let delta = &choice["delta"];
        if let Some(text) = delta["content"].as_str().filter(|text| !text.is_empty()) {
            self.text.push_str(text);
            on_text(&self.text);
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = call["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(self.calls.len());
            while self.calls.len() <= index {
                self.calls.push(Default::default());
            }
            let slot = &mut self.calls[index];
            if let Some(id) = call["id"].as_str() {
                slot.0 = id.to_string();
            }
            if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                slot.1.push_str(name);
            }
            if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str) {
                slot.2.push_str(arguments);
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self, requested_model: &str) -> Result<ToolChatResponse, AttemptError> {
        let tool_calls: Vec<ToolCall> = self
            .calls
            .into_iter()
            .enumerate()
            .filter(|(_, (_, name, _))| !name.is_empty())
            .map(|(index, (id, name, arguments))| ToolCall {
                // Some servers leave the id out of streamed calls.
                id: if id.is_empty() {
                    format!("call_{index}")
                } else {
                    id
                },
                name,
                arguments: crate::tool_chat::parse_arguments(&arguments),
            })
            .collect();
        if self.text.trim().is_empty() && tool_calls.is_empty() {
            return Err(AttemptError::request(LlmError::Empty));
        }
        Ok(ToolChatResponse {
            text: self.text,
            tool_calls,
            model: if self.model.is_empty() {
                requested_model.to_string()
            } else {
                self.model
            },
            finish_reason: self.finish_reason,
            usage: self.usage,
            provider_turn: None,
        })
    }
}

// --- Anthropic Messages --------------------------------------------------

#[derive(Default)]
pub(crate) struct AnthropicStream {
    model: String,
    blocks: Vec<Value>,
    /// Tool inputs arrive as JSON text in pieces.
    partial_inputs: Vec<String>,
    stop_reason: Option<String>,
    usage: TokenUsage,
}

impl AnthropicStream {
    pub(crate) fn fold(&mut self, data: &str, on_text: TextSink<'_>) -> Result<(), AttemptError> {
        let event: Value = serde_json::from_str(data)
            .map_err(|error| unreadable(format!("a streamed event wasn't JSON: {error}")))?;
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let message = &event["message"];
                if let Some(model) = message["model"].as_str() {
                    self.model = model.to_string();
                }
                let usage = &message["usage"];
                self.usage.input_tokens = usage["input_tokens"].as_u64().unwrap_or_default();
                self.usage.cache_read_input_tokens = usage["cache_read_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
                self.usage.cache_creation_input_tokens = usage["cache_creation_input_tokens"]
                    .as_u64()
                    .unwrap_or_default();
            }
            "content_block_start" => {
                let index = block_index(&event);
                while self.blocks.len() <= index {
                    self.blocks.push(Value::Null);
                    self.partial_inputs.push(String::new());
                }
                self.blocks[index] = event["content_block"].clone();
            }
            "content_block_delta" => {
                let index = block_index(&event);
                let Some(block) = self.blocks.get_mut(index) else {
                    return Err(unreadable(
                        "a delta arrived for a block never started".into(),
                    ));
                };
                let delta = &event["delta"];
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        append(block, "text", delta["text"].as_str().unwrap_or_default());
                        on_text(&self.text());
                    }
                    "thinking_delta" => append(
                        block,
                        "thinking",
                        delta["thinking"].as_str().unwrap_or_default(),
                    ),
                    "signature_delta" => {
                        append(
                            block,
                            "signature",
                            delta["signature"].as_str().unwrap_or_default(),
                        );
                    }
                    "input_json_delta" => self.partial_inputs[index]
                        .push_str(delta["partial_json"].as_str().unwrap_or_default()),
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = block_index(&event);
                if let Some(block) = self.blocks.get_mut(index) {
                    if block["type"] == "tool_use" {
                        let raw = &self.partial_inputs[index];
                        block["input"] = if raw.trim().is_empty() {
                            json!({})
                        } else {
                            serde_json::from_str(raw)
                                .unwrap_or_else(|_| json!({ "_unparsed": raw }))
                        };
                    }
                }
            }
            "message_delta" => {
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(reason.to_string());
                }
                if let Some(tokens) = event
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.usage.output_tokens = tokens;
                }
            }
            "error" => {
                let kind = event
                    .pointer("/error/type")
                    .and_then(Value::as_str)
                    .unwrap_or("error");
                let message = event
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("the stream failed");
                let error = LlmError::Other(format!("Anthropic {kind}: {message}"));
                return Err(if kind == "overloaded_error" || kind == "api_error" {
                    AttemptError::route(
                        error,
                        RouteFailure::Cooling {
                            reason: FailureReason::plain("overloaded"),
                            retry_after: None,
                        },
                    )
                } else {
                    AttemptError::request(error)
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn text(&self) -> String {
        self.blocks
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect()
    }

    /// The reply in the shape the Messages API returns it unstreamed.
    pub(crate) fn finish(self) -> Value {
        json!({
            "model": self.model,
            "content": self.blocks.into_iter().filter(|block| !block.is_null()).collect::<Vec<_>>(),
            "stop_reason": self.stop_reason,
            "usage": self.usage,
        })
    }
}

fn block_index(event: &Value) -> usize {
    event["index"]
        .as_u64()
        .and_then(|index| usize::try_from(index).ok())
        .unwrap_or_default()
}

fn append(block: &mut Value, field: &str, more: &str) {
    let mut current = block[field].as_str().unwrap_or_default().to_string();
    current.push_str(more);
    block[field] = Value::String(current);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Every text the stream reported, in order.
    #[derive(Default)]
    struct Seen(Mutex<Vec<String>>);

    impl Seen {
        fn push(&self, text: &str) {
            self.0.lock().unwrap().push(text.to_string());
        }
        fn all(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    #[test]
    fn sse_events_survive_any_chunking() {
        let stream = "data: {\"a\":1}\n\nevent: x\ndata: {\"b\":\r\ndata: 2}\r\n\r\n";
        for split in 1..stream.len() {
            let mut buffer = SseBuffer::default();
            let mut events = buffer.push(&stream[..split]);
            events.extend(buffer.push(&stream[split..]));
            assert_eq!(events, vec!["{\"a\":1}", "{\"b\":\n2}"], "split at {split}");
        }
    }

    #[test]
    fn openai_chunks_build_text_and_tool_calls() {
        let seen = Seen::default();
        let sink = |text: &str| seen.push(text);
        let mut stream = OpenAiStream::default();
        for data in [
            r#"{"model":"m","choices":[{"delta":{"content":"Look"}}]}"#,
            r#"{"choices":[{"delta":{"content":"ing."}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"mxr_search","arguments":"{\"qu"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ery\":\"x\"}"}}]},"finish_reason":"tool_calls"}]}"#,
            "[DONE]",
        ] {
            stream.fold(data, &sink).unwrap();
        }
        let reply = stream.finish("m").unwrap();
        assert_eq!(reply.text, "Looking.");
        assert_eq!(reply.tool_calls[0].id, "c1");
        assert_eq!(reply.tool_calls[0].arguments, json!({"query": "x"}));
        assert_eq!(reply.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(seen.all(), vec!["Look", "Looking."]);
    }

    #[test]
    fn anthropic_events_rebuild_the_unstreamed_reply_for_replay() {
        let seen = Seen::default();
        let sink = |text: &str| seen.push(text);
        let mut stream = AnthropicStream::default();
        for data in [
            r#"{"type":"message_start","message":{"model":"claude-opus-5-5","usage":{"input_tokens":10,"cache_read_input_tokens":8}}}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hm"}}"#,
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hi"}}"#,
            r#"{"type":"content_block_stop","index":1}"#,
            r#"{"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"t1","name":"mxr_status","input":{}}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"a\":"}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"1}"}}"#,
            r#"{"type":"content_block_stop","index":2}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}"#,
            r#"{"type":"message_stop"}"#,
        ] {
            stream.fold(data, &sink).unwrap();
        }
        let reply = stream.finish();
        assert_eq!(
            reply["content"],
            json!([
                {"type": "thinking", "thinking": "hm", "signature": "sig"},
                {"type": "text", "text": "Hi"},
                {"type": "tool_use", "id": "t1", "name": "mxr_status", "input": {"a": 1}}
            ])
        );
        assert_eq!(reply["stop_reason"], "tool_use");
        assert_eq!(reply["usage"]["output_tokens"], 5);
        assert_eq!(reply["usage"]["cache_read_input_tokens"], 8);
        assert_eq!(seen.all(), vec!["Hi"]);
    }

    #[test]
    fn an_overloaded_stream_counts_against_the_route() {
        let mut stream = AnthropicStream::default();
        let error = stream
            .fold(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#,
                &|_: &str| {},
            )
            .err()
            .unwrap();
        assert!(matches!(error.class, crate::failure::ErrorClass::Route(_)));
    }
}
