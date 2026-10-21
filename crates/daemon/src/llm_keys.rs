//! Provider API keys for the hosted routes (Anthropic, OpenAI, OpenRouter): stored in `secrets.toml` with the
//! account passwords (disk is authoritative, mode 0600, written by the daemon
//! alone), read from the provider's environment variable when none is stored.
//!
//! Unlike account passwords these are not mirrored to the OS keychain: the
//! keychain crate can't delete an entry, and a removed key must stay removed
//! instead of coming back through the keychain fallback.
//!
//! Keys are read from disk on every request, never cached, so `key set` and
//! `key remove` apply to the next request without a restart.

use mxr_llm::{ApiKey, ApiKeySource, RouteProvider};
use mxr_protocol::{LlmKeySourceData, LlmKeyStatusData, LlmProviderData};
use std::path::Path;

/// Longest key accepted. Real keys are around 100 characters.
const MAX_KEY_LEN: usize = 512;

pub(crate) struct LlmKeys {
    store: mxr_config::SecretStore,
}

impl LlmKeys {
    pub(crate) fn at_default_path() -> Self {
        Self {
            store: mxr_config::SecretStore::at_default_path(),
        }
    }

    #[cfg(test)]
    pub(crate) fn at(path: std::path::PathBuf) -> Self {
        Self {
            store: mxr_config::SecretStore::new(path),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        self.store.path()
    }

    /// Scoped to the runtime instance like account secrets, so a dev daemon
    /// never reads the production key.
    fn service() -> String {
        mxr_config::scoped_credential_service("mxr/llm-key")
    }

    fn stored(&self, provider: RouteProvider) -> Option<ApiKey> {
        match self.store.get(&Self::service(), provider.key()) {
            Ok(key) => key.map(ApiKey::new),
            Err(error) => {
                // The error text names the file, never its contents.
                tracing::warn!(provider = provider.key(), %error, "couldn't read the stored LLM key");
                None
            }
        }
    }

    fn from_env(provider: RouteProvider) -> Option<ApiKey> {
        let value = std::env::var(provider.env_var()).ok()?;
        let value = value.trim();
        (!value.is_empty()).then(|| ApiKey::new(value))
    }

    pub(crate) fn store(&self, provider: RouteProvider, key: &str) -> std::io::Result<()> {
        self.store.set(&Self::service(), provider.key(), key)
    }

    /// Whether a key was there to remove.
    pub(crate) fn remove(&self, provider: RouteProvider) -> std::io::Result<bool> {
        let service = Self::service();
        let existed = self.store.get(&service, provider.key())?.is_some();
        self.store.remove(&service, provider.key())?;
        Ok(existed)
    }

    pub(crate) fn status(&self, provider: RouteProvider) -> LlmKeyStatusData {
        let stored = self.stored(provider).is_some().then(|| self.path());
        status_of(provider, stored)
    }

    /// What `status` would report with the stored key gone.
    pub(crate) fn status_without_stored(provider: RouteProvider) -> LlmKeyStatusData {
        status_of(provider, None)
    }
}

/// A provider's key status: the stored key at `stored_at` wins, then the
/// environment variable.
pub(crate) fn status_of(provider: RouteProvider, stored_at: Option<&Path>) -> LlmKeyStatusData {
    let (source, location) = if let Some(path) = stored_at {
        (
            Some(LlmKeySourceData::Stored),
            Some(path.display().to_string()),
        )
    } else if LlmKeys::from_env(provider).is_some() {
        (
            Some(LlmKeySourceData::Env),
            Some(provider.env_var().to_string()),
        )
    } else {
        (None, None)
    };
    LlmKeyStatusData {
        provider: provider_data(provider),
        present: source.is_some(),
        source,
        location,
        env_var: provider.env_var().to_string(),
    }
}

impl ApiKeySource for LlmKeys {
    /// The stored key, then the environment.
    fn api_key(&self, provider: RouteProvider) -> Option<ApiKey> {
        self.stored(provider).or_else(|| Self::from_env(provider))
    }
}

pub(crate) fn provider_from_data(provider: LlmProviderData) -> RouteProvider {
    match provider {
        LlmProviderData::Anthropic => RouteProvider::Anthropic,
        LlmProviderData::Openai => RouteProvider::OpenAI,
        LlmProviderData::Openrouter => RouteProvider::OpenRouter,
    }
}

pub(crate) fn provider_data(provider: RouteProvider) -> LlmProviderData {
    match provider {
        RouteProvider::Anthropic => LlmProviderData::Anthropic,
        RouteProvider::OpenAI => LlmProviderData::Openai,
        RouteProvider::OpenRouter => LlmProviderData::Openrouter,
    }
}

/// A key as stdin delivers it: surrounding whitespace (the newline `echo`
/// and `op read` add) is dropped; anything else odd is refused, since a key
/// ends up in an HTTP header.
pub(crate) fn clean_key(raw: &str) -> Result<String, String> {
    let key = raw.trim();
    if key.is_empty() {
        return Err("the key is empty".to_string());
    }
    if key.len() > MAX_KEY_LEN {
        return Err(format!("the key is longer than {MAX_KEY_LEN} characters"));
    }
    if !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(
            "the key has spaces, line breaks or non-ASCII characters; pass only the key itself"
                .to_string(),
        );
    }
    Ok(key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Environment variables are process-wide: the first test owns
    // ANTHROPIC_API_KEY and the second OPENROUTER_API_KEY.
    fn keys(dir: &tempfile::TempDir) -> LlmKeys {
        LlmKeys::at(dir.path().join("secrets.toml"))
    }

    #[test]
    fn a_stored_key_wins_over_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let keys = keys(&dir);
        std::env::set_var("ANTHROPIC_API_KEY", "env-key-value");

        assert_eq!(
            keys.api_key(RouteProvider::Anthropic).unwrap().expose(),
            "env-key-value"
        );
        assert_eq!(
            keys.status(RouteProvider::Anthropic).source,
            Some(LlmKeySourceData::Env)
        );

        keys.store(RouteProvider::Anthropic, "stored-key-value")
            .unwrap();
        assert_eq!(
            keys.api_key(RouteProvider::Anthropic).unwrap().expose(),
            "stored-key-value"
        );
        let status = keys.status(RouteProvider::Anthropic);
        assert_eq!(status.source, Some(LlmKeySourceData::Stored));
        assert_eq!(
            status.location.as_deref(),
            Some(dir.path().join("secrets.toml").to_str().unwrap())
        );

        // Removing the stored key falls back to the environment.
        assert!(keys.remove(RouteProvider::Anthropic).unwrap());
        assert_eq!(
            keys.api_key(RouteProvider::Anthropic).unwrap().expose(),
            "env-key-value"
        );
        std::env::remove_var("ANTHROPIC_API_KEY");
        assert!(keys.api_key(RouteProvider::Anthropic).is_none());
        assert!(!keys.status(RouteProvider::Anthropic).present);
    }

    #[test]
    fn providers_have_separate_keys_and_an_empty_env_var_is_no_key() {
        let dir = tempfile::tempdir().unwrap();
        let keys = keys(&dir);
        keys.store(RouteProvider::OpenRouter, "or-key-value")
            .unwrap();
        assert_eq!(
            keys.api_key(RouteProvider::OpenRouter).unwrap().expose(),
            "or-key-value"
        );
        std::env::set_var("OPENROUTER_API_KEY", "   ");
        assert!(!LlmKeys::status_without_stored(RouteProvider::OpenRouter).present);
        std::env::remove_var("OPENROUTER_API_KEY");
    }

    #[test]
    fn keys_are_cleaned_and_odd_ones_refused() {
        assert_eq!(clean_key("  sk-ant-abc123\n").unwrap(), "sk-ant-abc123");
        for bad in [
            "",
            "   \n",
            "has space",
            "two\nlines",
            "naïve-key",
            &"k".repeat(600),
        ] {
            assert!(clean_key(bad).is_err(), "{bad:?}");
        }
    }
}
