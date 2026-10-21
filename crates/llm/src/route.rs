//! Which providers a model goes to, and the API keys they need.
//!
//! A `claude-*` model is Anthropic's own id: it goes to Anthropic's API first
//! and to OpenRouter (under OpenRouter's slug for the same model) when
//! Anthropic isn't working. A `gpt-<digit>...` model is OpenAI's own id and
//! goes the same way: OpenAI first, then OpenRouter as `openai/<id>`.
//! Everything else keeps using the single OpenAI-compatible endpoint from
//! config, except OpenRouter-style `vendor/model` ids when the user hasn't
//! pointed mxr at an endpoint of their own.

use std::fmt;
use std::sync::Arc;

/// A hosted API mxr can reach with a stored key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RouteProvider {
    Anthropic,
    OpenAI,
    OpenRouter,
}

impl RouteProvider {
    pub const ALL: [Self; 3] = [Self::Anthropic, Self::OpenAI, Self::OpenRouter];

    /// Stable machine name, used in IPC, CLI arguments and the key store.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
            Self::OpenRouter => "openrouter",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::OpenAI => "OpenAI",
            Self::OpenRouter => "OpenRouter",
        }
    }

    /// The environment variable read when no key is stored.
    pub const fn env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAI => "OPENAI_API_KEY",
            Self::OpenRouter => "OPENROUTER_API_KEY",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|provider| provider.key() == key)
    }
}

impl fmt::Display for RouteProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// One place a request can go: a provider and the model id that provider
/// knows the model by.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Route {
    pub provider: RouteProvider,
    pub model: String,
}

/// An API key. `Debug` never shows it, so it can sit in a struct that gets
/// logged.
#[derive(Clone)]
pub struct ApiKey(Arc<str>);

impl ApiKey {
    pub fn new(key: impl AsRef<str>) -> Self {
        Self(Arc::from(key.as_ref()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey([REDACTED])")
    }
}

/// Where keys come from. Asked on every request, so a key stored or removed
/// while the daemon runs applies to the next request without a restart.
pub trait ApiKeySource: Send + Sync {
    fn api_key(&self, provider: RouteProvider) -> Option<ApiKey>;
}

/// Vendors whose `vendor/model` ids are OpenRouter's. A vendor/model id is
/// also how LM Studio and Ollama name some local models, so only these
/// prefixes route, and only without an endpoint of the user's own.
const OPENROUTER_VENDORS: &[&str] = &[
    "openai",
    "google",
    "meta-llama",
    "mistralai",
    "deepseek",
    "x-ai",
    "qwen",
    "cohere",
    "perplexity",
    "amazon",
    "nvidia",
    "microsoft",
];

/// Where `llm.base_url` points, as far as routing cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// The default local Ollama.
    Default,
    /// A server of the user's own on this machine (LM Studio, another port).
    Local,
    /// A server of the user's own off this machine, whose model names are
    /// theirs: only Anthropic's own ids still route.
    Remote,
}

/// The routes for `model`, in the order they are tried, or `None` when the
/// model isn't routed and `llm.base_url` serves it as before.
///
/// A `gpt-*` id stays with a remote endpoint of the user's own (it may be
/// OpenAI itself or a proxy with its own keys), but goes to OpenAI for the
/// local ones: a local server doesn't serve these ids.
pub fn routes_for_model(model: &str, endpoint: Endpoint) -> Option<Vec<Route>> {
    let model = model.trim();
    if is_anthropic_id(model) {
        let slug = openrouter_slug(model)?;
        return Some(vec![
            Route {
                provider: RouteProvider::Anthropic,
                model: model.to_string(),
            },
            Route {
                provider: RouteProvider::OpenRouter,
                model: slug,
            },
        ]);
    }
    if is_openai_id(model) {
        return (endpoint != Endpoint::Remote).then(|| {
            vec![
                Route {
                    provider: RouteProvider::OpenAI,
                    model: model.to_string(),
                },
                Route {
                    provider: RouteProvider::OpenRouter,
                    model: format!("openai/{model}"),
                },
            ]
        });
    }
    let (vendor, name) = model.split_once('/')?;
    if name.is_empty() {
        return None;
    }
    // `anthropic/...` names the OpenRouter slug directly: OpenRouter only.
    let openrouter_only = vendor == "anthropic"
        || (endpoint == Endpoint::Default && OPENROUTER_VENDORS.contains(&vendor));
    openrouter_only.then(|| {
        vec![Route {
            provider: RouteProvider::OpenRouter,
            model: model.to_string(),
        }]
    })
}

/// Whether the model is a Claude model, under either naming. Claude 5.x
/// rejects non-default sampling parameters, so requests to these never carry
/// any.
pub fn is_claude_model(model: &str) -> bool {
    let model = model.trim();
    is_anthropic_id(model)
        || model
            .strip_prefix("anthropic/")
            .is_some_and(|name| name.starts_with("claude-"))
}

/// Whether the model is one of OpenAI's, under either naming. The newer
/// families reject `max_tokens` and non-default sampling parameters.
pub fn is_openai_model(model: &str) -> bool {
    let model = model.trim();
    is_openai_id(model) || model.strip_prefix("openai/").is_some_and(is_openai_id)
}

/// OpenAI's own ids: `gpt-` and a digit, so a local `gpt-oss` never matches.
fn is_openai_id(model: &str) -> bool {
    model
        .strip_prefix("gpt-")
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        && model
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

fn is_anthropic_id(model: &str) -> bool {
    model.starts_with("claude-")
        && model.len() > "claude-".len()
        && model
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
}

/// OpenRouter's slug for an Anthropic id: `claude-haiku-5-5` becomes
/// `anthropic/claude-haiku-5.5`, `claude-3-5-haiku-20241022` becomes
/// `anthropic/claude-3.5-haiku`. A trailing date or `latest` is dropped
/// (OpenRouter names a model, not a snapshot) and adjacent version numbers
/// are joined with a dot.
fn openrouter_slug(id: &str) -> Option<String> {
    let mut parts: Vec<&str> = id.split('-').collect();
    if parts
        .last()
        .is_some_and(|last| *last == "latest" || (last.len() == 8 && is_digits(last)))
    {
        parts.pop();
    }
    let mut slug = String::from("anthropic/");
    let mut previous_numeric = false;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return None;
        }
        let numeric = is_digits(part);
        if index > 0 {
            slug.push(if numeric && previous_numeric {
                '.'
            } else {
                '-'
            });
        }
        slug.push_str(part);
        previous_numeric = numeric;
    }
    Some(slug)
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(provider: RouteProvider, model: &str) -> Route {
        Route {
            provider,
            model: model.to_string(),
        }
    }

    #[test]
    fn anthropic_ids_map_to_openrouter_slugs() {
        let cases = [
            ("claude-haiku-5-5", "anthropic/claude-haiku-5.5"),
            ("claude-opus-5-5", "anthropic/claude-opus-5.5"),
            ("claude-sonnet-4-5", "anthropic/claude-sonnet-4.5"),
            ("claude-sonnet-4-20250514", "anthropic/claude-sonnet-4"),
            ("claude-opus-4-1-20250805", "anthropic/claude-opus-4.1"),
            ("claude-3-5-haiku-20241022", "anthropic/claude-3.5-haiku"),
            ("claude-3-5-haiku-latest", "anthropic/claude-3.5-haiku"),
            ("claude-3-haiku-20240307", "anthropic/claude-3-haiku"),
        ];
        for (id, slug) in cases {
            assert_eq!(openrouter_slug(id).as_deref(), Some(slug), "{id}");
        }
    }

    #[test]
    fn a_claude_model_tries_anthropic_then_openrouter() {
        assert_eq!(
            routes_for_model("claude-haiku-5-5", Endpoint::Default),
            Some(vec![
                route(RouteProvider::Anthropic, "claude-haiku-5-5"),
                route(RouteProvider::OpenRouter, "anthropic/claude-haiku-5.5"),
            ])
        );
        // A server of the user's own doesn't change where Anthropic's own
        // ids go.
        assert_eq!(
            routes_for_model("  claude-opus-5-5 ", Endpoint::Remote),
            Some(vec![
                route(RouteProvider::Anthropic, "claude-opus-5-5"),
                route(RouteProvider::OpenRouter, "anthropic/claude-opus-5.5"),
            ])
        );
    }

    #[test]
    fn an_explicit_anthropic_slug_means_openrouter_only() {
        for endpoint in [Endpoint::Default, Endpoint::Local, Endpoint::Remote] {
            assert_eq!(
                routes_for_model("anthropic/claude-haiku-5.5", endpoint),
                Some(vec![route(
                    RouteProvider::OpenRouter,
                    "anthropic/claude-haiku-5.5"
                )])
            );
        }
    }

    #[test]
    fn other_vendor_ids_route_to_openrouter_only_without_a_custom_endpoint() {
        assert_eq!(
            routes_for_model("openai/gpt-4o-mini", Endpoint::Default),
            Some(vec![route(RouteProvider::OpenRouter, "openai/gpt-4o-mini")])
        );
        assert_eq!(
            routes_for_model("google/gemini-2.5-flash", Endpoint::Default),
            Some(vec![route(
                RouteProvider::OpenRouter,
                "google/gemini-2.5-flash"
            )])
        );
        // The user's own server names models like this too.
        assert_eq!(routes_for_model("google/gemma-3-4b", Endpoint::Local), None);
    }

    #[test]
    fn local_and_unknown_model_names_are_not_routed() {
        for model in [
            "gemma4",
            "qwen2.5:3b-instruct",
            "gpt-oss:20b",
            "gpt-oss-20b",
            "gpt-",
            "GPT-6-luna",
            "llama-3.1-8b-instant",
            "hf.co/bartowski/Llama-3.2-3B-Instruct-GGUF",
            "lmstudio-community/gemma-3-4b",
            "claude",
            "claude-",
            "Claude-Haiku-5-5",
            "my-claude-haiku",
            "openai/",
            "",
        ] {
            assert_eq!(routes_for_model(model, Endpoint::Default), None, "{model}");
        }
    }

    #[test]
    fn a_gpt_model_tries_openai_then_openrouter() {
        for id in ["gpt-6-luna", "gpt-5.6-terra", "gpt-4o-mini"] {
            let expected = Some(vec![
                route(RouteProvider::OpenAI, id),
                route(RouteProvider::OpenRouter, &format!("openai/{id}")),
            ]);
            // The default Ollama and a server on this machine don't serve
            // these ids, so both route.
            for endpoint in [Endpoint::Default, Endpoint::Local] {
                assert_eq!(routes_for_model(id, endpoint), expected, "{id}");
            }
            // A remote server of the user's own keeps its model names.
            assert_eq!(routes_for_model(id, Endpoint::Remote), None, "{id}");
        }
    }

    #[test]
    fn openai_models_are_recognised_under_both_names() {
        assert!(is_openai_model("gpt-6-luna"));
        assert!(is_openai_model("openai/gpt-5.6-terra"));
        assert!(!is_openai_model("gpt-oss:20b"));
        assert!(!is_openai_model("claude-haiku-5-5"));
        assert!(!is_openai_model("gemma4"));
    }

    #[test]
    fn claude_models_are_recognised_under_both_names() {
        assert!(is_claude_model("claude-haiku-5-5"));
        assert!(is_claude_model("anthropic/claude-haiku-5.5"));
        assert!(!is_claude_model("openai/gpt-4o-mini"));
        assert!(!is_claude_model("gemma4"));
    }

    #[test]
    fn api_key_debug_never_shows_the_key() {
        let key = ApiKey::new("sk-ant-secret-value");
        assert!(!format!("{key:?}").contains("secret"));
        assert_eq!(key.expose(), "sk-ant-secret-value");
    }
}
