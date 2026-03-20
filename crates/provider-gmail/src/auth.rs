use thiserror::Error;
use yup_oauth2::InstalledFlowAuthenticator;
use yup_oauth2::InstalledFlowReturnMethod;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    #[error("Token expired or missing")]
    TokenExpired,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

const GMAIL_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/gmail.labels",
];

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
            token_fn: None,
        }
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
                let response = reqwest::Client::new()
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
                        body.error_description
                            .or(body.error)
                            .unwrap_or_else(|| format!("token refresh failed with status {status}")),
                    ));
                }

                body.access_token.ok_or(AuthError::TokenExpired)
            }) as TokenFuture
        });

        Self {
            client_id,
            client_secret,
            token_ref: "refresh-token".into(),
            token_fn: Some(token_fn),
        }
    }

    fn token_path(&self) -> std::path::PathBuf {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("mxr")
            .join("tokens");
        data_dir.join(format!("{}.json", self.token_ref))
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
        let secret = self.make_secret();
        let token_path = self.token_path();

        if let Some(parent) = token_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let auth =
            InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .persist_tokens_to_disk(token_path)
                .build()
                .await
                .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        // Force token fetch for the interactive flow
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
                    .map(|t| t.to_string())
                    .ok_or(AuthError::TokenExpired)
            })
        }));

        Ok(())
    }

    pub async fn load_existing(&mut self) -> Result<(), AuthError> {
        let token_path = self.token_path();
        if !token_path.exists() {
            return Err(AuthError::TokenExpired);
        }

        let secret = self.make_secret();
        let auth =
            InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .persist_tokens_to_disk(token_path)
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
                    .map(|t| t.to_string())
                    .ok_or(AuthError::TokenExpired)
            })
        }));

        Ok(())
    }

    pub async fn access_token(&self) -> Result<String, AuthError> {
        let token_fn = self.token_fn.as_ref().ok_or(AuthError::TokenExpired)?;
        (token_fn)().await
    }
}
