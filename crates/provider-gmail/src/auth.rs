use thiserror::Error;
use yup_oauth2::authenticator_delegate::{DeviceFlowDelegate, InstalledFlowDelegate};
use yup_oauth2::client::DefaultHyperClientBuilder;
use yup_oauth2::DeviceFlowAuthenticator;
use yup_oauth2::InstalledFlowAuthenticator;
use yup_oauth2::InstalledFlowReturnMethod;

use crate::auth_refresh::CachedTokenSource;
use crate::auth_storage::{has_keychain_token_cache, KeychainTokenStorage};

const GOOGLE_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

const UNSAFE_TOKEN_REF_CHARS: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];

/// Upper bound on one background token acquisition, including waiting for
/// another refresh. It cannot interrupt the synchronous keychain and disk write
/// that persists a result: that call returns on its own schedule (see
/// `a_blocked_cache_write_leaves_the_rest_of_the_runtime_running`). HTTP uses
/// the tighter bound below; browser authorization never runs inside
/// `access_token()`.
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

    /// The token endpoint answered 429. Not an authorization failure: the grant
    /// is intact and the caller should wait `retry_after_secs` (bounded).
    #[error("Token refresh rate limited: retry after {retry_after_secs}s")]
    RefreshRateLimited { retry_after_secs: u64 },

    /// A refreshed token could not be persisted. The grant and the in-memory
    /// token are fine; the local keychain or token file is the problem.
    #[error("Local token cache write failed: {0}")]
    TokenCacheWrite(String),

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
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
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

    mod explicit_authorization_handoff {
        use super::*;
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        use yup_oauth2::authenticator_delegate::DeviceAuthResponse;

        /// Counts interactive prompts and, for the installed flow, plays the
        /// browser by calling the local redirect with a synthetic code.
        #[derive(Clone, Default)]
        struct Prompts(Arc<AtomicUsize>);

        impl InstalledFlowDelegate for Prompts {
            fn present_user_url<'a>(
                &'a self,
                url: &'a str,
                need_code: bool,
            ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    assert!(!need_code, "HTTPRedirect never asks for a pasted code");
                    let redirect = url
                        .split("redirect_uri=")
                        .nth(1)
                        .and_then(|rest| rest.split('&').next())
                        .ok_or("authorization URL has no redirect_uri")?
                        .replace("localhost", "127.0.0.1");
                    tokio::spawn(async move {
                        let _ = reqwest::get(format!("{redirect}/?code=synthetic-code")).await;
                    });
                    Ok(String::new())
                })
            }
        }

        impl DeviceFlowDelegate for Prompts {
            fn present_user_code<'a>(
                &'a self,
                _: &'a DeviceAuthResponse,
            ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::SeqCst);
                })
            }
        }

        async fn mount_authorization(server: &MockServer) {
            let initial = serde_json::json!({
                "access_token": "explicit-access",
                "refresh_token": "explicit-refresh",
                // Inside yup-oauth2's one-minute margin: already due for refresh.
                "expires_in": 30,
                "token_type": "Bearer"
            });
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=authorization_code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(initial.clone()))
                .expect(0..=1)
                .mount(server)
                .await;
            Mock::given(method("POST"))
                .and(path("/device"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "device_code": "synthetic-device-code",
                    "user_code": "SYNTHETIC",
                    "verification_uri": format!("{}/verify", server.uri()),
                    "expires_in": 300,
                    "interval": 1
                })))
                .expect(0..=1)
                .mount(server)
                .await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("code=synthetic-device-code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(initial))
                .expect(0..=1)
                .mount(server)
                .await;
        }

        async fn authorize_then_refresh(flow: AuthFlow, rotated_refresh_token: Option<&str>) {
            let server = MockServer::start().await;
            mount_authorization(&server).await;
            let mut refreshed = serde_json::json!({
                "access_token": "refreshed-access",
                "token_type": "Bearer",
                "expires_in": 3600
            });
            if let Some(token) = rotated_refresh_token {
                refreshed["refresh_token"] = token.into();
            }
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .and(body_string_contains("refresh_token=explicit-refresh"))
                .respond_with(ResponseTemplate::new(200).set_body_json(refreshed))
                .expect(1)
                .mount(&server)
                .await;

            let tokens = MemoryTokens(Arc::new(Mutex::new(None)));
            let prompts = Prompts::default();
            let mut auth = GmailAuth::new("client".into(), "secret".into(), "handoff".into());
            let mut secret = auth.make_secret();
            secret.token_uri = format!("{}/token", server.uri());
            secret.auth_uri = format!("{}/authorize", server.uri());
            auth.authorize_with_storage(
                flow,
                secret,
                tokens.clone(),
                Delegates {
                    installed: Some(Box::new(prompts.clone())),
                    device: Some(Box::new(prompts.clone())),
                    device_code_url: Some(format!("{}/device", server.uri())),
                },
            )
            .await
            .expect("explicit authorization completes against the local endpoint");
            assert_eq!(prompts.0.load(Ordering::SeqCst), 1);
            let granted = tokens.get(GMAIL_SCOPES).await.unwrap();
            assert_eq!(granted.refresh_token.as_deref(), Some("explicit-refresh"));
            assert!(granted.is_expired());

            let token =
                tokio::time::timeout(std::time::Duration::from_secs(5), auth.access_token())
                    .await
                    .expect("the handoff refreshes without waiting for another prompt")
                    .unwrap();
            assert_eq!(token, "refreshed-access");
            // The refresh-only source reuses the cached token from here on.
            assert_eq!(auth.access_token().await.unwrap(), "refreshed-access");
            assert_eq!(prompts.0.load(Ordering::SeqCst), 1, "no second prompt");
            assert_eq!(
                tokens
                    .get(GMAIL_SCOPES)
                    .await
                    .unwrap()
                    .refresh_token
                    .as_deref(),
                Some(rotated_refresh_token.unwrap_or("explicit-refresh"))
            );
        }

        #[tokio::test]
        async fn installed_flow_hands_off_and_keeps_an_omitted_refresh_token() {
            authorize_then_refresh(AuthFlow::Installed, None).await;
        }

        #[tokio::test]
        async fn installed_flow_hands_off_and_stores_a_rotated_refresh_token() {
            authorize_then_refresh(AuthFlow::Installed, Some("rotated-refresh")).await;
        }

        #[tokio::test]
        async fn device_flow_hands_off_and_keeps_an_omitted_refresh_token() {
            authorize_then_refresh(AuthFlow::Device, None).await;
        }

        #[tokio::test]
        async fn device_flow_hands_off_and_stores_a_rotated_refresh_token() {
            authorize_then_refresh(AuthFlow::Device, Some("rotated-refresh")).await;
        }
    }

    mod explicit_refresh_token_helper {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        async fn error_for(response: ResponseTemplate) -> AuthError {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            GmailAuth::with_refresh_token_at(
                format!("{}/token", server.uri()),
                "client".into(),
                "secret".into(),
                "synthetic-refresh".into(),
            )
            .access_token()
            .await
            .unwrap_err()
        }

        #[tokio::test]
        async fn rejection_reports_only_the_oauth_code_not_the_description() {
            let error = error_for(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "description-must-not-leak"
            })))
            .await;

            assert!(matches!(error, AuthError::OAuth2(ref code) if code == "invalid_grant"));
            assert!(!error.to_string().contains("description-must-not-leak"));
        }

        #[tokio::test]
        async fn unreadable_rejection_and_throttle_match_the_background_helper() {
            let unreadable = error_for(ResponseTemplate::new(400).set_body_string("<html>")).await;
            assert!(
                matches!(&unreadable, AuthError::RefreshUnavailable(m) if m.contains("HTTP 400")),
                "{unreadable:?}"
            );

            let throttled =
                error_for(ResponseTemplate::new(429).insert_header("retry-after", "45")).await;
            assert!(matches!(
                throttled,
                AuthError::RefreshRateLimited {
                    retry_after_secs: 45
                }
            ));
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

/// Caller-provided hooks for one explicit authorization.
struct Delegates {
    installed: Option<Box<dyn InstalledFlowDelegate>>,
    device: Option<Box<dyn DeviceFlowDelegate>>,
    /// Test seam: Google's device-code endpoint when `None`.
    device_code_url: Option<String>,
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
        Self::with_refresh_token_at(
            GOOGLE_TOKEN_URI.to_string(),
            client_id,
            client_secret,
            refresh_token,
        )
    }

    fn with_refresh_token_at(
        token_uri: String,
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
            let token_uri = token_uri.clone();
            let client_id = token_client_id.clone();
            let client_secret = token_client_secret.clone();
            let refresh_token = refresh_token.clone();
            let client = token_client.clone();
            Box::pin(async move {
                let response = client
                    .post(&token_uri)
                    .form(&[
                        ("client_id", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", refresh_token.as_str()),
                        ("grant_type", "refresh_token"),
                    ])
                    .send()
                    .await
                    .map_err(crate::auth_refresh::refresh_http_error)?;
                crate::auth_refresh::read_refreshed_token(response)
                    .await
                    .map(|token| token.access_token)
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

    /// Cached-token auth over a caller-supplied storage and a local token endpoint.
    #[cfg(test)]
    pub(crate) fn for_test_storage(
        storage: Box<dyn yup_oauth2::storage::TokenStorage>,
        token_uri: String,
    ) -> Self {
        let mut auth = Self::new("client".into(), "secret".into(), "test".into());
        let mut secret = auth.make_secret();
        secret.token_uri = token_uri;
        auth.use_cached_auth(storage, secret)
            .expect("test HTTP client builds");
        auth
    }

    #[doc(hidden)]
    #[cfg(any(test, feature = "test-util"))]
    pub fn for_test_token(token: impl Into<String>) -> Self {
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
            token_uri: GOOGLE_TOKEN_URI.to_string(),
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

        let storage = KeychainTokenStorage::new_with_service(
            self.token_ref.clone(),
            token_path,
            self.keychain_service.clone(),
        );
        self.authorize_with_storage(
            flow,
            secret,
            storage,
            Delegates {
                installed: installed_delegate,
                device: device_delegate,
                device_code_url: None,
            },
        )
        .await
    }

    /// Run the explicit flow against `storage`, then install the refresh-only
    /// source on the same shared cache so later background refreshes never
    /// prompt again. Split out so tests can supply a fake storage and a local
    /// token endpoint.
    async fn authorize_with_storage<S>(
        &mut self,
        flow: AuthFlow,
        secret: yup_oauth2::ApplicationSecret,
        storage: S,
        delegates: Delegates,
    ) -> Result<(), AuthError>
    where
        S: yup_oauth2::storage::TokenStorage + Clone + 'static,
    {
        let Delegates {
            installed,
            device,
            device_code_url,
        } = delegates;
        let cached_secret = secret.clone();
        match flow {
            AuthFlow::Installed => {
                let mut builder = InstalledFlowAuthenticator::with_client(
                    secret,
                    InstalledFlowReturnMethod::HTTPRedirect,
                    oauth_hyper_client_builder(),
                )
                .with_storage(Box::new(storage.clone()));
                if let Some(delegate) = installed {
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
            }
            AuthFlow::Device => {
                // Device-flow polls are flow-fatal in yup-oauth2 (a transport
                // error aborts the whole authorization), so this path gets
                // the full 60s per-request bound instead of the 30s used for
                // refreshes — still hang-proof, but far less likely to
                // abandon a device code over one slow poll.
                let device_client =
                    DefaultHyperClientBuilder::default().with_timeout(TOKEN_REFRESH_TIMEOUT);
                let mut builder = DeviceFlowAuthenticator::with_client(secret, device_client)
                    .with_storage(Box::new(storage.clone()));
                if let Some(url) = device_code_url {
                    builder = builder.device_code_url(url);
                }
                if let Some(delegate) = device {
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
            }
        }

        self.use_cached_auth(Box::new(storage), cached_secret)
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

    pub(crate) fn use_cached_auth(
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
