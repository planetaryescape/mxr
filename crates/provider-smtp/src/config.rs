#![cfg_attr(test, allow(clippy::unwrap_used))]

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

    /// Retrieve the SMTP password through the daemon-provided credential resolver.
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;
    use std::time::Duration;

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
        .with_password_reader(Arc::new(move |_, _| {
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
        .with_password_reader(Arc::new(move |_, _| {
            reader_count.fetch_add(1, Ordering::SeqCst);
            Ok("app-password".to_string())
        }));
        let clone = config.clone();

        assert_eq!(config.resolve_password().unwrap(), "app-password");
        assert_eq!(clone.resolve_password().unwrap(), "app-password");
        assert_eq!(lookup_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_lookup_does_not_poison_cache() {
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
        .with_password_reader(Arc::new(move |_, _| {
            let attempt = reader_count.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                Err(SmtpError::Keyring("denied".to_string()))
            } else {
                Ok("app-password".to_string())
            }
        }));

        assert!(config.resolve_password().is_err());
        assert_eq!(config.resolve_password().unwrap(), "app-password");
        assert_eq!(config.resolve_password().unwrap(), "app-password");
        assert_eq!(lookup_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn parallel_lookups_share_one_reader_call() {
        let lookup_count = Arc::new(AtomicUsize::new(0));
        let reader_count = lookup_count.clone();
        let barrier = Arc::new(Barrier::new(8));
        let config = Arc::new(
            SmtpConfig::new(
                "smtp.example.com".into(),
                587,
                "user".into(),
                "mxr/test".into(),
                true,
                true,
            )
            .with_password_reader(Arc::new(move |_, _| {
                reader_count.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(25));
                Ok("app-password".to_string())
            })),
        );

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let config = config.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    assert_eq!(config.resolve_password().unwrap(), "app-password");
                });
            }
        });

        assert_eq!(lookup_count.load(Ordering::SeqCst), 1);
    }
}
