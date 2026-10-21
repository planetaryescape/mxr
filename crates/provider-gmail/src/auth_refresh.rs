//! Background token acquisition must never fall back to a browser flow.
//! yup-oauth2's installed/device authenticators do so on *any* refresh error,
//! hiding transient failures behind a wait for user authorization.

use std::time::{Duration, SystemTime};

use reqwest::header::HeaderMap;
use serde::Deserialize;
use tokio::sync::Mutex;
use yup_oauth2::storage::{TokenInfo, TokenStorage};

use crate::auth::{AuthError, GMAIL_SCOPES, TOKEN_HTTP_TIMEOUT};

pub(crate) struct CachedTokenSource {
    client: reqwest::Client,
    secret: yup_oauth2::ApplicationSecret,
    // Serialize cache lookup + refresh + persistence so simultaneous API calls
    // reuse the new token instead of refreshing the same grant in parallel.
    storage: Mutex<Box<dyn TokenStorage>>,
}

/// Longest `Retry-After` the provider carries upward. The daemon clamps its
/// own backoff tighter; this only stops an absurd header from travelling.
const MAX_RETRY_AFTER_SECS: u64 = 3600;
/// Used when a 429 carries no usable `Retry-After`, matching Gmail API 429s.
const DEFAULT_RETRY_AFTER_SECS: u64 = 60;

/// OAuth error codes that describe the endpoint, not the grant (RFC 6749
/// section 4.1.2.1), so they must not prescribe reauthorization.
const TRANSIENT_OAUTH_CODES: &[&str] = &["temporarily_unavailable", "server_error"];

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u32>,
    token_type: Option<String>,
    id_token: Option<String>,
    error: Option<String>,
}

/// A successful token-endpoint response that has passed validation.
pub(crate) struct RefreshedToken {
    pub(crate) access_token: String,
    pub(crate) expires_in: u32,
    pub(crate) refresh_token: Option<String>,
    pub(crate) id_token: Option<String>,
}

impl CachedTokenSource {
    pub(crate) fn new(
        storage: Box<dyn TokenStorage>,
        secret: yup_oauth2::ApplicationSecret,
    ) -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            // Unlike yup-oauth2's HTTP timeout, this covers the response body
            // as well as DNS, connection establishment and response headers.
            .timeout(TOKEN_HTTP_TIMEOUT)
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| AuthError::RefreshUnavailable(error.to_string()))?;
        Ok(Self {
            client,
            secret,
            storage: Mutex::new(storage),
        })
    }

    pub(crate) async fn access_token(&self) -> Result<String, AuthError> {
        let storage = self.storage.lock().await;
        let cached = storage
            .get(GMAIL_SCOPES)
            .await
            .ok_or(AuthError::TokenExpired)?;
        if !cached.is_expired() {
            if let Some(token) = cached.access_token.filter(|token| !token.is_empty()) {
                return Ok(token);
            }
        }
        let refresh_token = cached
            .refresh_token
            .filter(|token| !token.is_empty())
            .ok_or(AuthError::TokenExpired)?;
        let started = std::time::Instant::now();
        let result = self.refresh(&refresh_token).await;
        tracing::debug!(
            elapsed_ms = started.elapsed().as_millis(),
            succeeded = result.is_ok(),
            "background Gmail token refresh finished"
        );
        let refreshed = result?;
        let access_token = refreshed
            .access_token
            .clone()
            .ok_or(AuthError::TokenExpired)?;
        storage
            .set(GMAIL_SCOPES, refreshed)
            .await
            // The grant is intact and the new token already sits in the live
            // cache; only persistence failed, which no sign-in can fix.
            .map_err(|error| AuthError::TokenCacheWrite(error.to_string()))?;
        Ok(access_token)
    }

    async fn refresh(&self, refresh_token: &str) -> Result<TokenInfo, AuthError> {
        let response = self
            .client
            .post(&self.secret.token_uri)
            .form(&[
                ("client_id", self.secret.client_id.as_str()),
                ("client_secret", self.secret.client_secret.as_str()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(refresh_http_error)?;
        let refreshed = read_refreshed_token(response).await?;
        Ok(TokenInfo {
            access_token: Some(refreshed.access_token),
            refresh_token: Some(
                refreshed
                    .refresh_token
                    .unwrap_or_else(|| refresh_token.to_string()),
            ),
            expires_at: Some(
                (SystemTime::now() + Duration::from_secs(u64::from(refreshed.expires_in))).into(),
            ),
            id_token: refreshed.id_token,
        })
    }
}

/// Turn a token-endpoint response into a validated token or a classified
/// error. Shared by the background and explicit refresh helpers so both report
/// the same sanitized causes. Response bodies are parsed, never echoed.
pub(crate) async fn read_refreshed_token(
    response: reqwest::Response,
) -> Result<RefreshedToken, AuthError> {
    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AuthError::RefreshRateLimited {
            retry_after_secs: retry_after_secs(response.headers()),
        });
    }
    if status.is_server_error() {
        return Err(AuthError::RefreshUnavailable(format!("HTTP {status}")));
    }
    let body = match response.json::<TokenResponse>().await {
        Ok(body) => body,
        Err(error) if error.is_decode() => {
            // Neither a revoked grant nor a usable token: say what arrived.
            return Err(AuthError::RefreshUnavailable(format!(
                "token endpoint returned an unreadable HTTP {status} response"
            )));
        }
        Err(error) => return Err(refresh_http_error(error)),
    };
    if !status.is_success() {
        return Err(match body.error.as_deref().and_then(oauth_error_code) {
            Some(code) if TRANSIENT_OAUTH_CODES.contains(&code) => {
                AuthError::RefreshUnavailable(format!("{code} (HTTP {status})"))
            }
            // Only an OAuth error code is proof the endpoint rejected the grant.
            Some(code) => AuthError::OAuth2(code.to_string()),
            None => AuthError::RefreshUnavailable(format!(
                "token endpoint returned HTTP {status} without an OAuth error code"
            )),
        });
    }
    let access_token = body
        .access_token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AuthError::RefreshUnavailable("missing access token".into()))?;
    let expires_in = body
        .expires_in
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| AuthError::RefreshUnavailable("missing or invalid token lifetime".into()))?;
    if body
        .token_type
        .is_some_and(|kind| !kind.eq_ignore_ascii_case("bearer"))
    {
        return Err(AuthError::RefreshUnavailable(
            "unexpected token type".into(),
        ));
    }
    Ok(RefreshedToken {
        access_token,
        expires_in,
        refresh_token: body.refresh_token,
        id_token: body.id_token,
    })
}

/// An OAuth error code as the RFC defines them (`invalid_grant`, ...). Anything
/// else in that field is untrusted text and is not carried into diagnostics.
fn oauth_error_code(code: &str) -> Option<&str> {
    let well_formed = !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    well_formed.then_some(code)
}

/// `Retry-After` in delta-seconds, bounded. HTTP-date values are treated as
/// absent: the daemon clamps its backoff anyway, so a wrong default costs one
/// early retry.
fn retry_after_secs(headers: &HeaderMap) -> u64 {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
        .min(MAX_RETRY_AFTER_SECS)
}

/// A transport failure reduced to a fixed category. The reqwest error text can
/// carry the request URL and resolver or socket details, so none of it is kept.
pub(crate) fn refresh_http_error(error: reqwest::Error) -> AuthError {
    let message = if error.is_timeout() {
        "token endpoint request timed out"
    } else if has_tls_cause(&error) {
        "token endpoint TLS handshake failed"
    } else if error.is_connect() {
        "token endpoint connection failed"
    } else if error.is_decode() || error.is_body() {
        "token endpoint response could not be read"
    } else {
        "token endpoint request failed"
    };
    AuthError::RefreshUnavailable(message.to_string())
}

fn has_tls_cause(error: &reqwest::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    while let Some(current) = cause {
        let text = current.to_string().to_ascii_lowercase();
        // rustls reports a peer that is not speaking TLS as a "corrupt message".
        if [
            "tls",
            "ssl",
            "certificate",
            "handshake",
            "corrupt message",
            "fatal alert",
        ]
        .iter()
        .any(|marker| text.contains(marker))
        {
            return true;
        }
        cause = current.source();
    }
    false
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use yup_oauth2::storage::TokenStorageError;

    const BODY_MARKER: &str = "body-marker-must-not-leak";

    #[derive(Clone, Default)]
    struct MemoryTokens {
        token: Arc<StdMutex<Option<TokenInfo>>>,
        fail_writes: bool,
    }

    impl MemoryTokens {
        fn expired(refresh_token: &str) -> Self {
            let tokens = Self::default();
            *tokens.token.lock().unwrap() = Some(TokenInfo {
                access_token: Some("expired-access".into()),
                refresh_token: Some(refresh_token.into()),
                expires_at: Some(SystemTime::UNIX_EPOCH.into()),
                id_token: None,
            });
            tokens
        }

        fn refresh_token(&self) -> Option<String> {
            self.token
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|token| token.refresh_token.clone())
        }
    }

    #[async_trait::async_trait]
    impl TokenStorage for MemoryTokens {
        async fn set(&self, _scopes: &[&str], token: TokenInfo) -> Result<(), TokenStorageError> {
            if self.fail_writes {
                return Err(TokenStorageError::Other("disk full".into()));
            }
            *self.token.lock().unwrap() = Some(token);
            Ok(())
        }

        async fn get(&self, _scopes: &[&str]) -> Option<TokenInfo> {
            self.token.lock().unwrap().clone()
        }
    }

    fn source_for(token_uri: String, tokens: &MemoryTokens) -> CachedTokenSource {
        let secret = yup_oauth2::ApplicationSecret {
            client_id: "client".into(),
            client_secret: "secret".into(),
            token_uri,
            ..Default::default()
        };
        CachedTokenSource::new(Box::new(tokens.clone()), secret).unwrap()
    }

    async fn endpoint_responding(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        server
    }

    async fn refresh_error(response: ResponseTemplate) -> (AuthError, MemoryTokens) {
        let server = endpoint_responding(response).await;
        let tokens = MemoryTokens::expired("synthetic-refresh");
        let source = source_for(format!("{}/token", server.uri()), &tokens);
        let error = source.access_token().await.unwrap_err();
        (error, tokens)
    }

    #[tokio::test]
    async fn non_json_client_error_keeps_its_status_and_is_not_a_revocation() {
        let (error, tokens) =
            refresh_error(ResponseTemplate::new(400).set_body_string(BODY_MARKER)).await;

        let AuthError::RefreshUnavailable(message) = &error else {
            panic!("an unreadable rejection is not proof of revocation: {error:?}");
        };
        assert!(message.contains("HTTP 400"), "{message}");
        assert!(!error.to_string().contains(BODY_MARKER), "{error}");
        assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));
    }

    #[tokio::test]
    async fn json_rejection_without_an_oauth_code_is_not_a_revocation() {
        let (error, _) = refresh_error(
            ResponseTemplate::new(403).set_body_json(serde_json::json!({"detail": BODY_MARKER})),
        )
        .await;

        let AuthError::RefreshUnavailable(message) = &error else {
            panic!("{error:?}");
        };
        assert!(message.contains("HTTP 403"), "{message}");
        assert!(!message.contains(BODY_MARKER), "{message}");
    }

    #[tokio::test]
    async fn only_a_well_formed_oauth_code_is_carried_into_the_error() {
        let (rejected, _) = refresh_error(
            ResponseTemplate::new(400).set_body_json(serde_json::json!({"error": "invalid_grant"})),
        )
        .await;
        assert!(matches!(rejected, AuthError::OAuth2(ref code) if code == "invalid_grant"));

        // The field is remote text; anything but a code stays out of diagnostics.
        let (untrusted, _) = refresh_error(
            ResponseTemplate::new(400)
                .set_body_json(serde_json::json!({"error": format!("Bad {BODY_MARKER}")})),
        )
        .await;
        assert!(matches!(untrusted, AuthError::RefreshUnavailable(_)));
        assert!(!untrusted.to_string().contains(BODY_MARKER));
    }

    #[tokio::test]
    async fn wrong_typed_fields_are_an_unreadable_response_not_a_rejection() {
        for body in [
            serde_json::json!({"error": 42}),
            serde_json::json!({"error": {"code": "invalid_grant"}}),
        ] {
            let (error, _) = refresh_error(ResponseTemplate::new(400).set_body_json(body)).await;
            assert!(
                matches!(&error, AuthError::RefreshUnavailable(m) if m.contains("unreadable HTTP 400")),
                "{error:?}"
            );
        }
        let (lifetime, _) = refresh_error(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"access_token": "a", "token_type": "Bearer", "expires_in": "3600"}),
        ))
        .await;
        assert!(matches!(lifetime, AuthError::RefreshUnavailable(_)));
    }

    #[tokio::test]
    async fn a_throttled_refresh_recovers_on_the_next_attempt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "30"))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new-access", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&server)
            .await;
        let tokens = MemoryTokens::expired("synthetic-refresh");
        let source = source_for(format!("{}/token", server.uri()), &tokens);

        assert!(matches!(
            source.access_token().await,
            Err(AuthError::RefreshRateLimited {
                retry_after_secs: 30
            })
        ));
        assert_eq!(source.access_token().await.unwrap(), "new-access");
        assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));
    }

    #[tokio::test]
    async fn transient_oauth_codes_are_retryable_not_revocations() {
        for code in ["temporarily_unavailable", "server_error"] {
            let (error, tokens) = refresh_error(
                ResponseTemplate::new(400).set_body_json(serde_json::json!({ "error": code })),
            )
            .await;
            assert!(matches!(error, AuthError::RefreshUnavailable(_)), "{code}");
            assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));
        }
    }

    #[tokio::test]
    async fn malformed_success_responses_keep_the_grant_and_report_the_status() {
        let (not_json, tokens) =
            refresh_error(ResponseTemplate::new(200).set_body_string(BODY_MARKER)).await;
        let AuthError::RefreshUnavailable(message) = &not_json else {
            panic!("{not_json:?}");
        };
        assert!(message.contains("HTTP 200"), "{message}");
        assert!(!message.contains(BODY_MARKER), "{message}");
        assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));

        let (no_token, _) = refresh_error(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"token_type": "Bearer", "expires_in": 3600})),
        )
        .await;
        assert!(matches!(no_token, AuthError::RefreshUnavailable(_)));
    }

    #[tokio::test]
    async fn redirects_are_reported_not_followed() {
        let (error, _) = refresh_error(
            ResponseTemplate::new(302).insert_header("location", "http://127.0.0.1:9/elsewhere"),
        )
        .await;
        let AuthError::RefreshUnavailable(message) = &error else {
            panic!("{error:?}");
        };
        assert!(message.contains("HTTP 302"), "{message}");
        assert!(!message.contains("127.0.0.1"), "{message}");
    }

    #[tokio::test]
    async fn rate_limit_carries_a_bounded_retry_after_and_keeps_the_grant() {
        for (header, expected) in [
            (Some("120"), 120),
            (Some(" 7 "), 7),
            (Some("999999"), MAX_RETRY_AFTER_SECS),
            (
                Some("Wed, 21 Oct 2026 07:28:00 GMT"),
                DEFAULT_RETRY_AFTER_SECS,
            ),
            (Some("soon"), DEFAULT_RETRY_AFTER_SECS),
            (None, DEFAULT_RETRY_AFTER_SECS),
        ] {
            let mut response = ResponseTemplate::new(429).set_body_string(BODY_MARKER);
            if let Some(header) = header {
                response = response.insert_header("retry-after", header);
            }
            let (error, tokens) = refresh_error(response).await;
            assert!(
                matches!(error, AuthError::RefreshRateLimited { retry_after_secs } if retry_after_secs == expected),
                "Retry-After {header:?} -> {error:?}"
            );
            assert!(!error.to_string().contains(BODY_MARKER));
            assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));
        }
    }

    #[tokio::test]
    async fn cache_write_failure_after_a_good_refresh_is_not_an_authorization_error() {
        let server = endpoint_responding(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"access_token": "new-access", "token_type": "Bearer", "expires_in": 3600}),
        ))
        .await;
        let tokens = MemoryTokens {
            fail_writes: true,
            ..MemoryTokens::expired("synthetic-refresh")
        };
        let source = source_for(format!("{}/token", server.uri()), &tokens);

        let error = source.access_token().await.unwrap_err();

        assert!(matches!(error, AuthError::TokenCacheWrite(_)), "{error:?}");
        assert!(error.to_string().contains("Local token cache write failed"));
        assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));
    }

    #[tokio::test]
    async fn transport_failures_report_a_category_without_the_url() {
        // Connection refused: bind to learn a free port, then release it.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let tokens = MemoryTokens::expired("synthetic-refresh");
        let refused = source_for(format!("http://127.0.0.1:{port}/token"), &tokens)
            .access_token()
            .await
            .unwrap_err();
        assert!(
            matches!(&refused, AuthError::RefreshUnavailable(m) if m.contains("connection failed")),
            "{refused:?}"
        );

        // A peer that is not speaking TLS behind an https URL.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        });
        let tls = source_for(format!("https://127.0.0.1:{port}/token"), &tokens)
            .access_token()
            .await
            .unwrap_err();
        peer.abort();
        assert!(
            matches!(&tls, AuthError::RefreshUnavailable(m) if m.contains("TLS")),
            "{tls:?}"
        );

        for error in [&refused, &tls] {
            let text = error.to_string();
            assert!(
                !text.contains("127.0.0.1") && !text.contains(&port.to_string()),
                "{text}"
            );
        }
        assert_eq!(tokens.refresh_token().as_deref(), Some("synthetic-refresh"));
    }

    /// Storage whose `set` blocks its thread like a stuck keychain or disk call,
    /// until the test releases it.
    #[derive(Clone)]
    struct BlockedSet {
        inner: MemoryTokens,
        entered: Arc<StdMutex<std::sync::mpsc::Sender<()>>>,
        release: Arc<StdMutex<std::sync::mpsc::Receiver<()>>>,
    }

    #[async_trait::async_trait]
    impl TokenStorage for BlockedSet {
        async fn set(&self, scopes: &[&str], token: TokenInfo) -> Result<(), TokenStorageError> {
            self.entered.lock().unwrap().send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("the test releases the blocked write");
            self.inner.set(scopes, token).await
        }

        async fn get(&self, scopes: &[&str]) -> Option<TokenInfo> {
            self.inner.get(scopes).await
        }
    }

    // Measured 2026-10-21 with a 600ms blocked `set` and a 10ms heartbeat task:
    // a current-thread runtime stalled for 610ms; a two-worker multi-thread
    // runtime (the daemon's flavor) kept a 13ms maximum gap. The outer
    // acquisition guard did not preempt the blocked write on either: a 100ms
    // `timeout` around `access_token()` returned the token after 602ms. This
    // pins the part that holds: other tasks keep running while one write is
    // stuck, so a hung keychain call costs one worker, not the daemon.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_blocked_cache_write_leaves_the_rest_of_the_runtime_running() {
        let server = endpoint_responding(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"access_token": "new-access", "token_type": "Bearer", "expires_in": 3600}),
        ))
        .await;
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let storage = BlockedSet {
            inner: MemoryTokens::expired("synthetic-refresh"),
            entered: Arc::new(StdMutex::new(entered_tx)),
            release: Arc::new(StdMutex::new(release_rx)),
        };
        let secret = yup_oauth2::ApplicationSecret {
            token_uri: format!("{}/token", server.uri()),
            ..Default::default()
        };
        let source = CachedTokenSource::new(Box::new(storage), secret).unwrap();
        let refresh = tokio::spawn(async move { source.access_token().await });
        tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(10)))
            .await
            .unwrap()
            .expect("the refresh reaches the blocked write");

        let ticks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = ticks.clone();
        let heartbeat = tokio::spawn(async move {
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_millis(5)).await;
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        tokio::time::timeout(Duration::from_secs(5), heartbeat)
            .await
            .expect("other tasks run while a cache write is blocked")
            .unwrap();
        assert_eq!(ticks.load(std::sync::atomic::Ordering::SeqCst), 5);

        release_tx.send(()).unwrap();
        assert_eq!(refresh.await.unwrap().unwrap(), "new-access");
    }
}
