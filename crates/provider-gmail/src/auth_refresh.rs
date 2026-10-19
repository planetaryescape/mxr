//! Background token acquisition must never fall back to a browser flow.
//! yup-oauth2's installed/device authenticators do so on *any* refresh error,
//! hiding transient failures behind a wait for user authorization.

use std::time::{Duration, SystemTime};

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

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u32>,
    token_type: Option<String>,
    id_token: Option<String>,
    error: Option<String>,
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
            .map_err(|error| AuthError::OAuth2(format!("token cache write failed: {error}")))?;
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
        let status = response.status();
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(AuthError::RefreshUnavailable(format!("HTTP {status}")));
        }
        let body: TokenResponse = response.json().await.map_err(refresh_http_error)?;
        if !status.is_success() {
            // Keep the OAuth error code (invalid_grant, invalid_client, ...),
            // not a response body that could contain credential material.
            return Err(AuthError::OAuth2(body.error.unwrap_or_else(|| {
                format!("token endpoint rejected request: HTTP {status}")
            })));
        }
        let access_token = body
            .access_token
            .filter(|token| !token.is_empty())
            .ok_or_else(|| AuthError::RefreshUnavailable("missing access token".into()))?;
        let expires_in = body
            .expires_in
            .filter(|seconds| *seconds > 0)
            .ok_or_else(|| {
                AuthError::RefreshUnavailable("missing or invalid token lifetime".into())
            })?;
        if body
            .token_type
            .is_some_and(|kind| !kind.eq_ignore_ascii_case("bearer"))
        {
            return Err(AuthError::RefreshUnavailable(
                "unexpected token type".into(),
            ));
        }
        Ok(TokenInfo {
            access_token: Some(access_token),
            refresh_token: Some(
                body.refresh_token
                    .unwrap_or_else(|| refresh_token.to_string()),
            ),
            expires_at: Some(
                (SystemTime::now() + Duration::from_secs(u64::from(expires_in))).into(),
            ),
            id_token: body.id_token,
        })
    }
}

pub(crate) fn refresh_http_error(error: reqwest::Error) -> AuthError {
    let message = if error.is_timeout() {
        "token endpoint request timed out".to_string()
    } else {
        error.without_url().to_string()
    };
    AuthError::RefreshUnavailable(message)
}
