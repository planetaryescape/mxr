use thiserror::Error;
use yup_oauth2::authenticator_delegate::{DeviceFlowDelegate, InstalledFlowDelegate};
use yup_oauth2::client::DefaultHyperClientBuilder;
use yup_oauth2::DeviceFlowAuthenticator;
use yup_oauth2::InstalledFlowAuthenticator;
use yup_oauth2::InstalledFlowReturnMethod;

use crate::auth_refresh::CachedTokenSource;
use crate::auth_storage::{has_keychain_token_cache, KeychainTokenStorage};

const UNSAFE_TOKEN_REF_CHARS: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];

/// Upper bound on one background token acquisition, including waiting for
/// another refresh and persisting its result. HTTP uses the tighter bound
/// below; browser authorization never runs inside `access_token()`.
const TOKEN_REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Per-request bound applied inside yup-oauth2's HTTP client. Keeps each
/// token-endpoint call from wedging on a dead connection well below the
/// outer `TOKEN_REFRESH_TIMEOUT`. The interactive browser wait is not an
/// HTTP client request, so slow first-time authorizations are unaffected.
pub(crate) const TOKEN_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// yup-oauth2 client builder with `TOKEN_HTTP_TIMEOUT` applied — used for
/// every authenticator this module constructs.
fn oauth_hyper_client_builder() -> DefaultHyperClientBuilder {
    DefaultHyperClientBuilder::default().with_timeout(TOKEN_HTTP_TIMEOUT)
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    #[error("Token refresh temporarily unavailable: {0}")]
    RefreshUnavailable(String),

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
    use std::sync::{Arc, Mutex};
    use yup_oauth2::storage::{TokenInfo, TokenStorage, TokenStorageError};

    #[derive(Clone)]
    struct MemoryTokens(Arc<Mutex<Option<TokenInfo>>>);

    #[async_trait::async_trait]
    impl TokenStorage for MemoryTokens {
        async fn set(&self, _scopes: &[&str], token: TokenInfo) -> Result<(), TokenStorageError> {
            *self.0.lock().unwrap() = Some(token);
            Ok(())
        }

        async fn get(&self, _scopes: &[&str]) -> Option<TokenInfo> {
            self.0.lock().unwrap().clone()
        }
    }

    async fn cached_auth(server_uri: &str) -> (GmailAuth, MemoryTokens) {
        let tokens = MemoryTokens(Arc::new(Mutex::new(Some(TokenInfo {
            access_token: Some("expired-access".into()),
            refresh_token: Some("synthetic-refresh".into()),
            expires_at: Some(std::time::SystemTime::UNIX_EPOCH.into()),
            id_token: None,
        }))));
        let mut auth = GmailAuth::new("client".into(), "secret".into(), "test".into());
        let mut secret = auth.make_secret();
        secret.token_uri = format!("{server_uri}/token");
        secret.auth_uri = format!("{server_uri}/authorize");
        auth.use_cached_auth(Box::new(tokens.clone()), secret)
            .unwrap();
        (auth, tokens)
    }

    #[tokio::test]
    async fn cached_refresh_failure_does_not_wait_for_browser_authorization() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(503)
                    .set_body_json(serde_json::json!({"error": "temporarily_unavailable"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (auth, tokens) = cached_auth(&server.uri()).await;

        let result = tokio::time::timeout(std::time::Duration::from_secs(1), auth.access_token())
            .await
            .expect("a failed refresh must return its error, not wait for a browser callback");
        assert!(matches!(result, Err(AuthError::RefreshUnavailable(_))));
        assert_eq!(
            tokens
                .get(GMAIL_SCOPES)
                .await
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("synthetic-refresh"),
            "a temporary failure must preserve the stored grant"
        );
    }

    #[tokio::test]
    async fn cached_refresh_recovers_after_a_temporary_failure_and_keeps_the_grant() {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .with_priority(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("refresh_token=synthetic-refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new-access",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        let (auth, tokens) = cached_auth(&server.uri()).await;

        let failed = tokio::time::timeout(std::time::Duration::from_secs(1), auth.access_token())
            .await
            .expect("temporary failure must return without starting interactive auth");
        assert!(failed.is_err());
        let (first, concurrent) = tokio::join!(auth.access_token(), auth.access_token());
        assert_eq!(first.unwrap(), "new-access");
        assert_eq!(concurrent.unwrap(), "new-access");
        assert_eq!(auth.access_token().await.unwrap(), "new-access");
        let saved = tokens.get(GMAIL_SCOPES).await.unwrap();
        assert_eq!(saved.refresh_token.as_deref(), Some("synthetic-refresh"));
        assert!(!saved.is_expired());
        let remaining = saved.expires_at.unwrap().unix_timestamp() - chrono::Utc::now().timestamp();
        assert!(
            (3590..=3600).contains(&remaining),
            "remaining lifetime: {remaining}s"
        );
    }

    #[tokio::test]
    async fn revoked_cached_refresh_returns_invalid_grant_without_waiting_for_browser() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({"error": "invalid_grant"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (auth, _) = cached_auth(&server.uri()).await;

        let error = tokio::time::timeout(std::time::Duration::from_secs(1), auth.access_token())
            .await
            .expect("a revoked grant must fail without starting interactive auth")
            .unwrap_err();
        assert!(matches!(error, AuthError::OAuth2(ref code) if code == "invalid_grant"));
    }

    #[tokio::test]
    async fn missing_cached_grant_does_not_start_interactive_auth() {
        let server = wiremock::MockServer::start().await;
        let (auth, tokens) = cached_auth(&server.uri()).await;
        *tokens.0.lock().unwrap() = None;

        let error = tokio::time::timeout(std::time::Duration::from_secs(1), auth.access_token())
            .await
            .expect("a missing grant must not wait for interactive auth")
            .unwrap_err();
        assert!(matches!(error, AuthError::TokenExpired));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn token_acquisition_guard_reports_a_retryable_timeout() {
        let mut auth = GmailAuth::for_test_token("unused");
        auth.token_fn = Some(Box::new(|| Box::pin(std::future::pending())));
        let error = auth.access_token().await.unwrap_err();
        assert!(matches!(error, AuthError::RefreshUnavailable(_)));
    }

    #[tokio::test]
    async fn cached_refresh_persists_rotated_grant_for_the_next_refresh() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/token"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": "new-access",
                    "refresh_token": "rotated-refresh",
                    "token_type": "Bearer",
                    "expires_in": 3600
                })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (auth, tokens) = cached_auth(&server.uri()).await;

        assert_eq!(auth.access_token().await.unwrap(), "new-access");
        assert_eq!(
            tokens
                .get(GMAIL_SCOPES)
                .await
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("rotated-refresh")
        );
    }

    #[tokio::test]
    async fn cached_refresh_bounds_response_body_not_just_headers() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}", listener.local_addr().unwrap());
        let (headers_sent, headers_received) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            stream.read(&mut bytes).await.unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nContent-Type: application/json\r\n\r\n{")
                .await
                .unwrap();
            headers_sent.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        let (auth, _) = cached_auth(&uri).await;
        let refresh = tokio::spawn(async move { auth.access_token().await });
        headers_received.await.unwrap();
        tokio::time::pause();
        tokio::time::advance(TOKEN_HTTP_TIMEOUT + std::time::Duration::from_secs(1)).await;
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), refresh).await;
        server.abort();

        let error = outcome
            .expect(
                "a stalled response body must time out within the HTTP bound, not the 60s guard",
            )
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
    }

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

pub(crate) const GMAIL_SCOPES: &[&str] = &[
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
    /// Cached and explicit refresh-token sources share this acquisition surface.
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
        // Built once and moved into the closure: this token_fn runs on every
        // Gmail API call (no token caching on the refresh-token path), and a
        // reqwest client carries a connection pool worth keeping. Short
        // connect timeout so dead/half-open networks fail in seconds; the
        // 30s total stays strictly inside `access_token()`'s 60s outer bound
        // so the timeout error is deterministic. Builder only fails on TLS
        // misconfiguration — fall back loudly to the default client.
        let token_client = reqwest::Client::builder()
            .timeout(TOKEN_HTTP_TIMEOUT)
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to configure token-refresh HTTP timeouts; using default client without timeouts");
                reqwest::Client::new()
            });
        let token_fn = Box::new(move || {
            let client_id = token_client_id.clone();
            let client_secret = token_client_secret.clone();
            let refresh_token = refresh_token.clone();
            let client = token_client.clone();
            Box::pin(async move {
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
                    .map_err(crate::auth_refresh::refresh_http_error)?;
                let status = response.status();
                if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    return Err(AuthError::RefreshUnavailable(format!("HTTP {status}")));
                }
                let body: RefreshTokenResponse = response
                    .json()
                    .await
                    .map_err(crate::auth_refresh::refresh_http_error)?;

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
                .with_storage(Box::new(storage.clone()));
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

                self.use_cached_auth(Box::new(storage), self.make_secret())?;
            }
            AuthFlow::Device => {
                let storage = KeychainTokenStorage::new_with_service(
                    self.token_ref.clone(),
                    token_path.clone(),
                    self.keychain_service.clone(),
                );
                // Device-flow polls are flow-fatal in yup-oauth2 (a transport
                // error aborts the whole authorization), so this path gets
                // the full 60s per-request bound instead of the 30s used for
                // refreshes — still hang-proof, but far less likely to
                // abandon a device code over one slow poll.
                let device_client =
                    DefaultHyperClientBuilder::default().with_timeout(TOKEN_REFRESH_TIMEOUT);
                let mut builder = DeviceFlowAuthenticator::with_client(secret, device_client)
                    .with_storage(Box::new(storage.clone()));
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

                self.use_cached_auth(Box::new(storage), self.make_secret())?;
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
        self.use_cached_auth(Box::new(storage), secret)
    }

    fn use_cached_auth(
        &mut self,
        storage: Box<dyn yup_oauth2::storage::TokenStorage>,
        secret: yup_oauth2::ApplicationSecret,
    ) -> Result<(), AuthError> {
        let source = std::sync::Arc::new(CachedTokenSource::new(storage, secret)?);
        self.token_fn = Some(Box::new(move || {
            let source = source.clone();
            Box::pin(async move { source.access_token().await })
        }));

        Ok(())
    }

    pub async fn access_token(&self) -> Result<String, AuthError> {
        let token_fn = self.token_fn.as_ref().ok_or(AuthError::TokenExpired)?;
        match tokio::time::timeout(TOKEN_REFRESH_TIMEOUT, (token_fn)()).await {
            Ok(result) => result,
            Err(_) => Err(AuthError::RefreshUnavailable(format!(
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
