//! LLM provider trait and OpenAI-compatible HTTP client.
//!
//! Supports any backend that exposes the OpenAI Chat Completions API:
//!
//! * **Ollama** (`http://localhost:11434/v1`, no API key)
//! * **LM Studio** (`http://localhost:1234/v1`, no API key)
//! * **OpenAI** (`https://api.openai.com/v1`)
//! * **Groq** (`https://api.groq.com/openai/v1`)
//! * **OpenRouter** (`https://openrouter.ai/api/v1`)
//! * **Together AI**, **Mistral La Plateforme**, **Anthropic via proxy**, etc.
//!
//! mxr stays local-first by default: the recommended config points at
//! a local Ollama or LM Studio instance, and no completions ever leave
//! the user's machine. Cloud endpoints are an explicit opt-in via
//! `MXR_LLM_API_KEY` (or whatever env var the config names).
//!
//! Streaming is intentionally not part of the trait yet — the
//! consumers (thread summarize, draft assist) are short-form
//! interactive: a single completion call returning the full response
//! is simpler and just as fast in practice for ≤2KB outputs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LlmFeature {
    Summarize,
    RelationshipSummary,
    Commitments,
    DraftAssist,
    DraftNew,
    DraftRefine,
    VoiceMatch,
    HumanizeRewrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub model: String,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmCapabilities {
    pub context_window: u32,
    pub supports_streaming: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("LLM is disabled in config")]
    Disabled,
    #[error("LLM endpoint unreachable: {0}")]
    Unreachable(String),
    #[error("LLM rate-limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("LLM request timed out after {0:?}")]
    Timeout(Duration),
    #[error("LLM authentication failed (check API key)")]
    Unauthorized,
    #[error("LLM returned an empty completion")]
    Empty,
    #[error("LLM error: {0}")]
    Other(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn capabilities(&self) -> LlmCapabilities;
    fn model_name(&self) -> &str;
}

pub struct LlmRuntime {
    provider: RwLock<Arc<dyn LlmProvider>>,
}

impl LlmRuntime {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider: RwLock::new(provider),
        }
    }

    pub fn replace(&self, provider: Arc<dyn LlmProvider>) {
        *self
            .provider
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = provider;
    }

    pub fn for_feature(self: &Arc<Self>, _feature: LlmFeature) -> Arc<Self> {
        self.clone()
    }

    pub async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.current().complete(req).await
    }

    pub fn capabilities(&self) -> LlmCapabilities {
        self.current().capabilities()
    }

    pub fn model_name(&self) -> String {
        self.current().model_name().to_string()
    }

    fn current(&self) -> Arc<dyn LlmProvider> {
        self.provider
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Stub provider used when LLM features are disabled. All calls return
/// `LlmError::Disabled` so callers can degrade gracefully.
#[derive(Debug, Clone, Default)]
pub struct NoopProvider;

#[async_trait]
impl LlmProvider for NoopProvider {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Err(LlmError::Disabled)
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: 0,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        "noop"
    }
}

/// OpenAI-compatible chat-completions client. One implementation
/// covers Ollama, LM Studio, OpenAI, Groq, OpenRouter, and any other
/// service that speaks the OpenAI v1 chat-completions schema.
///
/// `api_key` is optional — Ollama and LM Studio don't require one;
/// hosted endpoints typically do. The header is omitted entirely when
/// `api_key` is `None`.
pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    context_window: u32,
    request_timeout: Duration,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key: config.api_key,
            model: config.model,
            context_window: config.context_window,
            request_timeout: config.request_timeout,
            client,
        }
    }

    /// Convenience constructor for a local Ollama instance.
    pub fn ollama(model: impl Into<String>) -> Self {
        Self::new(OpenAiCompatibleConfig {
            base_url: "http://localhost:11434/v1".into(),
            api_key: None,
            model: model.into(),
            context_window: 8192,
            request_timeout: Duration::from_secs(120),
        })
    }

    /// Convenience constructor for a local LM Studio instance.
    pub fn lm_studio(model: impl Into<String>) -> Self {
        Self::new(OpenAiCompatibleConfig {
            base_url: "http://localhost:1234/v1".into(),
            api_key: None,
            model: model.into(),
            context_window: 8192,
            request_timeout: Duration::from_secs(120),
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub context_window: u32,
    pub request_timeout: Duration,
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = ChatCompletionsRequestBody {
            model: &self.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
        };

        let mut request = self.client.post(&url).json(&body);
        if let Some(key) = self.api_key.as_deref() {
            request = request.bearer_auth(key);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                LlmError::Timeout(self.request_timeout)
            } else if e.is_connect() {
                LlmError::Unreachable(redact_key(e.to_string(), self.api_key.as_deref()))
            } else {
                LlmError::Other(redact_key(e.to_string(), self.api_key.as_deref()))
            }
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(LlmError::Unauthorized);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30);
            return Err(LlmError::RateLimited {
                retry_after_secs: retry_after,
            });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::Other(format!(
                "{} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                redact_key(body, self.api_key.as_deref())
            )));
        }

        let raw = response
            .json::<ChatCompletionsResponseBody>()
            .await
            .map_err(|e| LlmError::Other(format!("response parse error: {e}")))?;

        let choice = raw.choices.into_iter().next().ok_or(LlmError::Empty)?;
        let content = choice.message.content;
        if content.trim().is_empty() {
            return Err(LlmError::Empty);
        }
        Ok(CompletionResponse {
            content,
            model: raw.model,
            finish_reason: choice.finish_reason,
        })
    }

    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            context_window: self.context_window,
            supports_streaming: false,
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[derive(Serialize)]
struct ChatCompletionsRequestBody<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatCompletionsResponseBody {
    model: String,
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    #[allow(dead_code)]
    role: String,
    content: String,
}

/// Strip a known API key from a string before logging or surfacing.
/// Defensive: callers shouldn't be sending the key into errors, but
/// reqwest sometimes embeds it in URLs or hostnames.
fn redact_key(s: String, key: Option<&str>) -> String {
    let Some(key) = key else { return s };
    if key.len() < 8 {
        return s;
    }
    s.replace(key, "<redacted>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_provider_returns_disabled_error() {
        let p = NoopProvider;
        let err = p
            .complete(CompletionRequest {
                messages: vec![ChatMessage::user("hello")],
                max_tokens: None,
                temperature: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::Disabled));
    }

    #[test]
    fn redact_replaces_api_key_substring() {
        let key = "sk-this-is-a-secret-key";
        let s = format!("error from https://api/v1?key={key} oops");
        let redacted = redact_key(s, Some(key));
        assert!(!redacted.contains(key));
        assert!(redacted.contains("<redacted>"));
    }

    #[test]
    fn redact_leaves_short_keys_alone_to_avoid_collateral_damage() {
        // Very short keys (e.g., empty / corrupted) shouldn't trigger
        // mass replacement since they're likely to match unrelated text.
        let s = "the model name is gpt".to_string();
        assert_eq!(redact_key(s.clone(), Some("gpt")), s);
    }

    #[test]
    fn ollama_defaults_to_localhost_with_no_api_key() {
        let p = OpenAiCompatibleProvider::ollama("llama3.2");
        assert_eq!(p.base_url, "http://localhost:11434/v1");
        assert!(p.api_key.is_none());
        assert_eq!(p.model_name(), "llama3.2");
    }

    #[test]
    fn lm_studio_defaults_to_localhost_with_no_api_key() {
        let p = OpenAiCompatibleProvider::lm_studio("local-model");
        assert_eq!(p.base_url, "http://localhost:1234/v1");
        assert!(p.api_key.is_none());
    }

    #[test]
    fn capabilities_surface_context_window() {
        let p = OpenAiCompatibleProvider::ollama("llama3.2");
        assert_eq!(p.capabilities().context_window, 8192);
    }
}
