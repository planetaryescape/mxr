use thiserror::Error;
use yup_oauth2::authenticator_delegate::{DeviceFlowDelegate, InstalledFlowDelegate};
use yup_oauth2::client::DefaultHyperClientBuilder;
use yup_oauth2::DeviceFlowAuthenticator;
use yup_oauth2::InstalledFlowAuthenticator;
use yup_oauth2::InstalledFlowReturnMethod;

use crate::auth_storage::{has_keychain_token_cache, KeychainTokenStorage};

const UNSAFE_TOKEN_REF_CHARS: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];

/// Upper bound on a single token acquisition/refresh. The underlying
/// OAuth client (yup-oauth2 / reqwest) has no inherent timeout, so a
/// refresh that stalls on a dead connection — e.g. a half-open socket
/// left behind by a network blip — would otherwise hang the caller, and
/// any sync holding the provider lock, indefinitely.
const TOKEN_REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Per-request bound applied inside yup-oauth2's HTTP client. Keeps each
/// token-endpoint call from wedging on a dead connection well below the
/// outer `TOKEN_REFRESH_TIMEOUT`. The interactive browser wait is not an
/// HTTP client request, so slow first-time authorizations are unaffected.
const TOKEN_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// yup-oauth2 client builder with `TOKEN_HTTP_TIMEOUT` applied — used for
/// every authenticator this module constructs.
fn oauth_hyper_client_builder() -> DefaultHyperClientBuilder {
    DefaultHyperClientBuilder::default().with_timeout(TOKEN_HTTP_TIMEOUT)
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    #[error("Invalid token_ref: {0}")]
    InvalidTokenRef(String),

    #[error("Token expired or missing")]
    TokenExpired,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_storage_keeps_tokens_and_keychain_service_under_runtime_identity() {
        let token_root = std::path::PathBuf::from("/tmp/mxr-dev/tokens");
        let auth = GmailAuth::new(
            "client".to_string(),
            "secret".to_string(),
            "mxr/work-gmail".to_string(),
        )
        .with_storage(token_root.clone(), "mxr-dev-gmail-oauth".to_string());

        assert_eq!(
            auth.storage_config_for_test().expect("valid token_ref"),
            (
                token_root.join("mxr/work-gmail.json"),
                "mxr-dev-gmail-oauth".to_string()
            )
        );
    }

    #[test]
    fn token_ref_validation_rejects_unsafe_disk_paths() {
        for token_ref in [
            "",
            "/tmp/mxr-token",
            "../mxr-token",
            "mxr/../work-gmail",
            "mxr//work-gmail",
            "mxr/work-gmail/",
            "mxr/./work-gmail",
            "mxr\\work-gmail",
            "gmail:personal",
            "mxr/work*.gmail",
            "mxr/work\ngmail",
        ] {
            let err = GmailAuth::new(
                "client".to_string(),
                "secret".to_string(),
                token_ref.to_string(),
            )
            .storage_config_for_test()
            .expect_err("unsafe token_ref should be rejected");

            assert!(
                matches!(err, AuthError::InvalidTokenRef(_)),
                "expected InvalidTokenRef for {token_ref:?}, got {err:?}"
            );
        }
    }
}

const GMAIL_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/gmail.labels",
];

/// OAuth grant flow.
///
/// `Installed` is the legacy localhost-redirect flow — convenient on a
/// developer machine with a local browser, broken on SSH sessions and
/// headless containers because the redirect target (`http://localhost:N`)
/// isn't reachable from the user's actual browser.
///
/// `Device` is RFC 8628 Limited Input Device flow. The CLI prints a code +
/// `https://www.google.com/device` URL; the user opens it in any browser,
/// pastes the code, approves. Works anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlow {
    Installed,
    Device,
}

impl AuthFlow {
    /// Auto-pick a flow based on the environment. If we're not on a TTY, or
    /// there's no display server (`DISPLAY` / `WAYLAND_DISPLAY` unset on
    /// non-macOS), the `Installed` flow's localhost redirect is unlikely to
    /// reach a browser — fall back to `Device`. Otherwise prefer `Installed`
    /// (one less click for local users).
    pub fn auto_detect() -> Self {
        use std::io::IsTerminal;
        let no_tty = !std::io::stdin().is_terminal();
        let no_display = cfg!(not(target_os = "macos"))
            && std::env::var_os("DISPLAY").is_none()
            && std::env::var_os("WAYLAND_DISPLAY").is_none();
        let ssh_session =
            std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
        if no_tty || no_display || ssh_session {
            Self::Device
        } else {
            Self::Installed
        }
    }
}

/// Bundled OAuth client credentials for mxr.
/// Users can override these with their own in config.toml (BYOC).
/// Set GMAIL_CLIENT_ID and GMAIL_CLIENT_SECRET env vars at build time,
/// or users provide their own via config.
pub const BUNDLED_CLIENT_ID: Option<&str> = option_env!("GMAIL_CLIENT_ID");
pub const BUNDLED_CLIENT_SECRET: Option<&str> = option_env!("GMAIL_CLIENT_SECRET");

impl GmailAuth {
    /// Create with bundled credentials, falling back to error if not compiled in.
    pub fn with_bundled(token_ref: String) -> Result<Self, AuthError> {
        let client_id = BUNDLED_CLIENT_ID
            .ok_or_else(|| AuthError::OAuth2("no bundled client_id — rebuild with GMAIL_CLIENT_ID env var, or provide credentials in config.toml".into()))?;
        let client_secret = BUNDLED_CLIENT_SECRET
            .ok_or_else(|| AuthError::OAuth2("no bundled client_secret — rebuild with GMAIL_CLIENT_SECRET env var, or provide credentials in config.toml".into()))?;
        Ok(Self::new(
            client_id.to_string(),
            client_secret.to_string(),
            token_ref,
        ))
    }
}

pub struct GmailAuth {
    client_id: String,
    client_secret: String,
    token_ref: String,
    token_root: std::path::PathBuf,
    keychain_service: String,
    /// Stores a boxed function that returns an access token.
    /// We use a trait object to avoid spelling out yup-oauth2's internal Authenticator type.
    token_fn: Option<Box<dyn Fn() -> TokenFuture + Send + Sync>>,
}

type TokenFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AuthError>> + Send>>;

#[derive(serde::Deserialize)]
struct RefreshTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

impl GmailAuth {
    pub fn new(client_id: String, client_secret: String, token_ref: String) -> Self {
        Self {
            client_id,
            client_secret,
            token_ref,
            token_root: legacy_token_root(),
            keychain_service: crate::auth_storage::KEYCHAIN_SERVICE.to_string(),
            token_fn: None,
        }
    }

    pub fn with_storage(
        mut self,
        token_root: std::path::PathBuf,
        keychain_service: String,
    ) -> Self {
        self.token_root = token_root;
        self.keychain_service = keychain_service;
        self
    }

    #[cfg(test)]
    pub(crate) fn storage_config_for_test(
        &self,
    ) -> Result<(std::path::PathBuf, String), AuthError> {
        Ok((self.token_path()?, self.keychain_service.clone()))
    }

    pub fn with_refresh_token(
        client_id: String,
        client_secret: String,
        refresh_token: String,
    ) -> Self {
        let token_client_id = client_id.clone();
        let token_client_secret = client_secret.clone();
        let token_fn = Box::new(move || {
            let client_id = token_client_id.clone();
            let client_secret = token_client_secret.clone();
            let refresh_token = refresh_token.clone();
            Box::pin(async move {
                // A short connect timeout so dead/half-open networks fail in
                // seconds; `access_token()`'s TOKEN_REFRESH_TIMEOUT stays the
                // outer bound. Builder only fails on TLS misconfiguration.
                let client = reqwest::Client::builder()
                    .timeout(TOKEN_REFRESH_TIMEOUT)
                    .connect_timeout(std::time::Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                let response = client
                    .post("https://oauth2.googleapis.com/token")
                    .form(&[
                        ("client_id", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", refresh_token.as_str()),
                        ("grant_type", "refresh_token"),
                    ])
                    .send()
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;
                let status = response.status();
                let body: RefreshTokenResponse = response
                    .json()
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;

                if !status.is_success() {
                    return Err(AuthError::OAuth2(
                        body.error_description.or(body.error).unwrap_or_else(|| {
                            format!("token refresh failed with status {status}")
                        }),
                    ));
                }

                body.access_token.ok_or(AuthError::TokenExpired)
            }) as TokenFuture
        });

        Self {
            client_id,
            client_secret,
            token_ref: "refresh-token".into(),
            token_root: legacy_token_root(),
            keychain_service: crate::auth_storage::KEYCHAIN_SERVICE.to_string(),
            token_fn: Some(token_fn),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test_token(token: impl Into<String>) -> Self {
        let token = token.into();
        let token_fn = Box::new(move || {
            let token = token.clone();
            Box::pin(async move { Ok(token) }) as TokenFuture
        });

        Self {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            token_ref: "test-token".into(),
            token_root: legacy_token_root(),
            keychain_service: crate::auth_storage::KEYCHAIN_SERVICE.to_string(),
            token_fn: Some(token_fn),
        }
    }

    fn token_path(&self) -> Result<std::path::PathBuf, AuthError> {
        validate_token_ref(&self.token_ref)?;
        Ok(self.token_root.join(format!("{}.json", self.token_ref)))
    }

    fn make_secret(&self) -> yup_oauth2::ApplicationSecret {
        yup_oauth2::ApplicationSecret {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            auth_uri: "https://accounts.google.com/o/oauth2/auth".to_string(),
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
            redirect_uris: vec!["http://localhost".to_string()],
            ..Default::default()
        }
    }

    pub async fn interactive_auth(&mut self) -> Result<(), AuthError> {
        let flow = AuthFlow::auto_detect();
        self.interactive_auth_with_flow(flow).await
    }

    /// Run the OAuth flow chosen by the caller. `AuthFlow::Installed` opens a
    /// browser to a localhost callback (the legacy default; only works with a
    /// local browser). `AuthFlow::Device` prints a code + URL the user opens
    /// in any browser — works on SSH sessions, headless containers, or any
    /// box without a browser.
    pub async fn interactive_auth_with_flow(&mut self, flow: AuthFlow) -> Result<(), AuthError> {
        self.interactive_auth_with_delegates(flow, None, None).await
    }

    /// Run OAuth with caller-provided yup-oauth2 delegates. Desktop and web
    /// clients use this to expose the auth URL/device code through a structured
    /// session instead of relying on stdout.
    pub async fn interactive_auth_with_delegates(
        &mut self,
        flow: AuthFlow,
        installed_delegate: Option<Box<dyn InstalledFlowDelegate>>,
        device_delegate: Option<Box<dyn DeviceFlowDelegate>>,
    ) -> Result<(), AuthError> {
        let secret = self.make_secret();
        let token_path = self.token_path()?;

        if let Some(parent) = token_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        match flow {
            AuthFlow::Installed => {
                let storage = KeychainTokenStorage::new_with_service(
                    self.token_ref.clone(),
                    token_path.clone(),
                    self.keychain_service.clone(),
                );
                let mut builder = InstalledFlowAuthenticator::with_client(
                    secret,
                    InstalledFlowReturnMethod::HTTPRedirect,
                    oauth_hyper_client_builder(),
                )
                .with_storage(Box::new(storage));
                if let Some(delegate) = installed_delegate {
                    builder = builder.flow_delegate(delegate);
                }
                let auth = builder
                    .build()
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;

                let _token = auth
                    .token(GMAIL_SCOPES)
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;

                let auth = std::sync::Arc::new(auth);
                self.token_fn = Some(Box::new(move || {
                    let auth = auth.clone();
                    Box::pin(async move {
                        let tok = auth
                            .token(GMAIL_SCOPES)
                            .await
                            .map_err(|e| AuthError::OAuth2(e.to_string()))?;
                        tok.token()
                            .map(std::string::ToString::to_string)
                            .ok_or(AuthError::TokenExpired)
                    })
                }));
            }
            AuthFlow::Device => {
                let storage = KeychainTokenStorage::new_with_service(
                    self.token_ref.clone(),
                    token_path.clone(),
                    self.keychain_service.clone(),
                );
                let mut builder =
                    DeviceFlowAuthenticator::with_client(secret, oauth_hyper_client_builder())
                        .with_storage(Box::new(storage));
                if let Some(delegate) = device_delegate {
                    builder = builder.flow_delegate(delegate);
                }
                let auth = builder
                    .build()
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;

                let _token = auth
                    .token(GMAIL_SCOPES)
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;

                let auth = std::sync::Arc::new(auth);
                self.token_fn = Some(Box::new(move || {
                    let auth = auth.clone();
                    Box::pin(async move {
                        let tok = auth
                            .token(GMAIL_SCOPES)
                            .await
                            .map_err(|e| AuthError::OAuth2(e.to_string()))?;
                        tok.token()
                            .map(std::string::ToString::to_string)
                            .ok_or(AuthError::TokenExpired)
                    })
                }));
            }
        }

        Ok(())
    }

    pub async fn load_existing(&mut self) -> Result<(), AuthError> {
        let token_path = self.token_path()?;
        // Either the keychain entry or the legacy on-disk cache must exist.
        // The keychain check is fast enough (one OS call) that it's fine in
        // the common case even when nothing is stored.
        let has_keychain_entry = has_keychain_token_cache(&self.keychain_service, &self.token_ref);
        if !has_keychain_entry && !token_path.exists() {
            return Err(AuthError::TokenExpired);
        }

        let storage = KeychainTokenStorage::new_with_service(
            self.token_ref.clone(),
            token_path,
            self.keychain_service.clone(),
        );
        let secret = self.make_secret();
        let auth = InstalledFlowAuthenticator::with_client(
            secret,
            InstalledFlowReturnMethod::HTTPRedirect,
            oauth_hyper_client_builder(),
        )
        .with_storage(Box::new(storage))
        .build()
        .await
        .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        let auth = std::sync::Arc::new(auth);
        self.token_fn = Some(Box::new(move || {
            let auth = auth.clone();
            Box::pin(async move {
                let tok = auth
                    .token(GMAIL_SCOPES)
                    .await
                    .map_err(|e| AuthError::OAuth2(e.to_string()))?;
                tok.token()
                    .map(std::string::ToString::to_string)
                    .ok_or(AuthError::TokenExpired)
            })
        }));

        Ok(())
    }

    pub async fn access_token(&self) -> Result<String, AuthError> {
        let token_fn = self.token_fn.as_ref().ok_or(AuthError::TokenExpired)?;
        match tokio::time::timeout(TOKEN_REFRESH_TIMEOUT, (token_fn)()).await {
            Ok(result) => result,
            Err(_) => Err(AuthError::OAuth2(format!(
                "token refresh timed out after {TOKEN_REFRESH_TIMEOUT:?}"
            ))),
        }
    }
}

fn legacy_token_root() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("mxr")
        .join("tokens")
}

fn validate_token_ref(token_ref: &str) -> Result<(), AuthError> {
    if token_ref.is_empty() {
        return Err(AuthError::InvalidTokenRef("must not be empty".to_string()));
    }

    if std::path::Path::new(token_ref).is_absolute() {
        return Err(AuthError::InvalidTokenRef(
            "must be a relative path".to_string(),
        ));
    }

    if token_ref
        .chars()
        .any(|c| c.is_control() || UNSAFE_TOKEN_REF_CHARS.contains(&c))
    {
        return Err(AuthError::InvalidTokenRef(
            "contains unsafe path characters".to_string(),
        ));
    }

    for component in token_ref.split('/') {
        if component.is_empty() {
            return Err(AuthError::InvalidTokenRef(
                "must not contain empty path components".to_string(),
            ));
        }
        if component == "." || component == ".." {
            return Err(AuthError::InvalidTokenRef(
                "must not contain traversal path components".to_string(),
            ));
        }
    }

    Ok(())
}
