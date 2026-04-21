#![cfg_attr(test, allow(clippy::unwrap_used))]

use serde::Deserialize;
use std::sync::{Arc, Mutex};

type PasswordReader = Arc<dyn Fn(&str, &str) -> Result<String, SmtpError> + Send + Sync>;

#[derive(Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Keyring reference (e.g., "mxr/work-smtp"). Looked up at runtime.
    pub password_ref: String,
    #[serde(default = "default_true")]
    pub auth_required: bool,
    #[serde(default = "default_true")]
    pub use_tls: bool,
    #[serde(skip, default = "default_password_cache")]
    password_cache: Arc<Mutex<Option<String>>>,
    #[serde(skip, default = "default_password_reader")]
    password_reader: PasswordReader,
}

fn default_true() -> bool {
    true
}

fn default_password_cache() -> Arc<Mutex<Option<String>>> {
    Arc::new(Mutex::new(None))
}

fn default_password_reader() -> PasswordReader {
    Arc::new(|password_ref, username| {
        mxr_keychain::get_password(password_ref, username)
            .map_err(|e| SmtpError::Keyring(e.to_string()))
    })
}

impl SmtpConfig {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        auth_required: bool,
        use_tls: bool,
    ) -> Self {
        Self {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            password_cache: default_password_cache(),
            password_reader: default_password_reader(),
        }
    }

    /// Retrieve the SMTP password from the system keyring.
    pub fn resolve_password(&self) -> Result<String, SmtpError> {
        let mut cached = self
            .password_cache
            .lock()
            .map_err(|_| SmtpError::Keyring("Failed to lock SMTP password cache".to_string()))?;

        if let Some(password) = cached.as_ref() {
            return Ok(password.clone());
        }

        let password = (self.password_reader)(&self.password_ref, &self.username)?;
        *cached = Some(password.clone());
        Ok(password)
    }

    #[cfg(test)]
    fn with_password_reader_for_tests(mut self, password_reader: PasswordReader) -> Self {
        self.password_reader = password_reader;
        self
    }
}

impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password_ref", &self.password_ref)
            .field("auth_required", &self.auth_required)
            .field("use_tls", &self.use_tls)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SmtpError {
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("SMTP transport error: {0}")]
    Transport(String),
    #[error("Message build error: {0}")]
    MessageBuild(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn default_use_tls_is_true() {
        let json =
            r#"{"host":"smtp.example.com","port":587,"username":"user","password_ref":"mxr/test"}"#;
        let config: SmtpConfig = serde_json::from_str(json).unwrap();
        assert!(config.auth_required);
        assert!(config.use_tls);
    }

    #[test]
    fn resolve_password_caches_first_lookup() {
        let lookup_count = Arc::new(AtomicUsize::new(0));
        let reader_count = lookup_count.clone();
        let config = SmtpConfig::new(
            "smtp.example.com".into(),
            587,
            "user".into(),
            "mxr/test".into(),
            true,
            true,
        )
        .with_password_reader_for_tests(Arc::new(move |_, _| {
            reader_count.fetch_add(1, Ordering::SeqCst);
            Ok("app-password".to_string())
        }));

        assert_eq!(config.resolve_password().unwrap(), "app-password");
        assert_eq!(config.resolve_password().unwrap(), "app-password");
        assert_eq!(lookup_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cloned_configs_share_cached_password() {
        let lookup_count = Arc::new(AtomicUsize::new(0));
        let reader_count = lookup_count.clone();
        let config = SmtpConfig::new(
            "smtp.example.com".into(),
            587,
            "user".into(),
            "mxr/test".into(),
            true,
            true,
        )
        .with_password_reader_for_tests(Arc::new(move |_, _| {
            reader_count.fetch_add(1, Ordering::SeqCst);
            Ok("app-password".to_string())
        }));
        let clone = config.clone();

        assert_eq!(config.resolve_password().unwrap(), "app-password");
        assert_eq!(clone.resolve_password().unwrap(), "app-password");
        assert_eq!(lookup_count.load(Ordering::SeqCst), 1);
    }
}
