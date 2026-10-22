//! The `trace` calls without the `langfuse` feature: same signatures, no
//! work. Closures passed in are never called, so nothing is serialized.

use crate::failure::AttemptError;
use crate::{
    ChatMessage, CompletionResponse, LlmError, LlmFeature, ToolChatMessage, ToolChatResponse,
};
use serde_json::Value;
use std::borrow::Cow;
use std::future::Future;

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Span,
    Generation,
    Agent,
    Tool,
}

pub struct Observation;

impl Observation {
    pub fn start(_name: impl Into<Cow<'static, str>>, _kind: Kind) -> Self {
        Self
    }

    #[must_use]
    #[inline]
    pub fn in_session(self, _id: &str) -> Self {
        self
    }

    #[inline]
    pub fn is_recording(&self) -> bool {
        false
    }

    pub fn instrument<F: Future>(&self, future: F) -> F {
        future
    }

    pub fn input(&self, _value: impl FnOnce() -> Value) {}

    pub fn output(&self, _value: impl FnOnce() -> Value) {}

    pub fn metadata(&self, _key: &str, _value: impl Into<String>) {}

    #[inline]
    pub fn fail(&self, _message: &str) {}
}

pub fn text_or_json(_text: &str) -> Value {
    Value::Null
}

#[inline]
pub(crate) fn feature(_feature: Option<LlmFeature>) -> Observation {
    Observation
}

pub(crate) fn completion_input(_messages: &[ChatMessage]) -> Value {
    Value::Null
}

pub(crate) fn chat_input(_messages: &[ToolChatMessage]) -> Value {
    Value::Null
}

pub(crate) fn parameter_f32(_value: Option<f32>) -> Value {
    Value::Null
}

#[inline]
pub(crate) fn finish_completion(
    _span: &Observation,
    _result: &Result<CompletionResponse, LlmError>,
) {
}

#[inline]
pub(crate) fn finish_chat(_span: &Observation, _result: &Result<ToolChatResponse, LlmError>) {}

pub(crate) fn generation(
    _name: &'static str,
    _model: &str,
    _endpoint: &str,
    _input: impl FnOnce() -> Value,
    _parameters: impl FnOnce() -> Value,
) -> Observation {
    Observation
}

#[inline]
pub(crate) fn finish_chat_attempt(
    _span: &Observation,
    _result: &Result<ToolChatResponse, AttemptError>,
) {
}
