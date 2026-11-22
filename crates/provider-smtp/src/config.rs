#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::sync::Arc;

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
    password_cache: Arc<OnceCell<String>>,
    #[serde(skip, default = "default_password_reader")]
    password_reader: PasswordReader,
}

fn default_true() -> bool {
    true
}

fn default_password_cache() -> Arc<OnceCell<String>> {
    Arc::new(OnceCell::new())
}

fn default_password_reader() -> PasswordReader {
    Arc::new(|password_ref, _username| {
        Err(SmtpError::Keyring(format!(
            "credential resolver not configured for {password_ref}"
        )))
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

    pub fn with_password(mut self, password: String) -> Self {
        self.password_reader = Arc::new(move |_, _| Ok(password.clone()));
        self
    }

    pub fn with_password_reader(mut self, password_reader: PasswordReader) -> Self {
        self.password_reader = password_reader;
        self
    }

    /// Reject unusable configs at load time instead of at first send.
    ///
    /// Refusing `auth_required && !use_tls` here is the primary guard against
    /// leaking credentials over cleartext (`build_transport` repeats it as
    /// defense-in-depth).
    pub fn validate(&self) -> Result<(), SmtpError> {
        if self.host.trim().is_empty() {
            return Err(SmtpError::Transport("SMTP host must not be empty".into()));
        }
        if self.port == 0 {
            return Err(SmtpError::Transport("SMTP port must not be 0".into()));
        }
        if self.auth_required && !self.use_tls {
            return Err(SmtpError::Transport(
                "refusing to send SMTP credentials over an unencrypted connection (auth_required with use_tls=false)".into(),
            ));
        }
        Ok(())
    }

    pub fn resolve_password(&self) -> Result<String, SmtpError> {
        self.password_cache
            .get_or_try_init(|| (self.password_reader)(&self.password_ref, &self.username))
            .cloned()
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

    #[test]
    fn validate_rejects_cleartext_auth() {
        let config = SmtpConfig::new(
            "smtp.example.com".into(),
            587,
            "user".into(),
            "mxr/test".into(),
            true,
            false,
        );
        assert!(matches!(
            config.validate(),
            Err(SmtpError::Transport(msg)) if msg.contains("unencrypted")
        ));
    }
}
