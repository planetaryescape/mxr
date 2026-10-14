//! Which model each AI feature uses, which providers it goes through, and
//! whether those are working. Key values never appear here: a key goes in
//! through [`LlmKeySecret`] and only its presence comes back out.

use serde::{Deserialize, Serialize};
use std::fmt;

/// An LLM provider key on its way to the daemon. `Debug` never shows it, so
/// a logged request can't leak it.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct LlmKeySecret(String);

impl LlmKeySecret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LlmKeySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// A hosted provider mxr can hold a key for.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderData {
    Anthropic,
    Openrouter,
}

impl LlmProviderData {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Openrouter => "openrouter",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LlmKeySourceData {
    /// Stored by `mxr llm key set`; wins over the environment.
    Stored,
    /// Read from the provider's environment variable.
    Env,
}

/// Whether a provider has a key and where it comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmKeyStatusData {
    pub provider: LlmProviderData,
    pub present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<LlmKeySourceData>,
    /// The file holding a stored key, or the environment variable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// The environment variable that is read when no key is stored.
    pub env_var: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LlmRouteStateData {
    /// Working, as far as mxr knows.
    Ok,
    /// Failed recently for a reason that passes (rate limit, outage).
    Cooling,
    /// Failed for a reason that needs a change (bad key, unknown model).
    Broken,
    /// The wait is over; the next request will try it.
    Recovering,
    /// A request is trying it now.
    Probing,
    /// No key for this provider, so it is not tried.
    NoKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmRouteStatusData {
    pub provider: LlmProviderData,
    /// The id this provider knows the model by.
    pub model: String,
    pub state: LlmRouteStateData,
    /// When a cooling or broken route may be tried again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Why it is out, e.g. `429 rate limited`. Fixed phrases, never text
    /// from the provider's reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// A request would go here now.
    pub next: bool,
}

/// One AI feature's model and where its requests go.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmFeatureStatusData {
    /// The feature's name, e.g. `summarize`.
    pub feature: String,
    pub enabled: bool,
    pub model: String,
    /// The model goes through Anthropic and/or OpenRouter with stored keys
    /// rather than `llm.base_url`.
    pub routed: bool,
    /// Requests can leave this machine.
    pub cloud: bool,
    /// The endpoint for a model that isn't routed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Routes in the order they are tried. Empty when not routed.
    #[serde(default)]
    pub routes: Vec<LlmRouteStatusData>,
    /// Privacy settings keep this feature from its model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

/// The outcome of storing or removing a provider key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmKeyChangeData {
    pub provider: LlmProviderData,
    /// This was a preview: nothing was stored or removed.
    pub dry_run: bool,
    /// The store changed (or would have): a key was written or deleted.
    pub changed: bool,
    /// What the key status is after the change (or would be).
    pub after: LlmKeyStatusData,
}
