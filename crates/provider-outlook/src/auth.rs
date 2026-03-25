use crate::error::OutlookError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bundled Azure app client ID for mxr.
/// Build with `OUTLOOK_CLIENT_ID` env var set, or users provide their own in config.
pub const BUNDLED_CLIENT_ID: Option<&str> = option_env!("OUTLOOK_CLIENT_ID");

/// Whether this Outlook account is a personal Microsoft account or a work/org account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlookTenant {
    /// Personal @outlook.com / @hotmail.com / @live.com — uses /consumers endpoint.
    Personal,
    /// Work / Microsoft 365 / Exchange Online — uses /organizations endpoint.
    Work,
}

impl OutlookTenant {
    fn device_code_url(&self) -> &'static str {
        match self {
            Self::Personal => {
                "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
            }
            Self::Work => {
                "https://login.microsoftonline.com/organizations/oauth2/v2.0/devicecode"
            }
        }
    }

    fn token_url(&self) -> &'static str {
        match self {
            Self::Personal => "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
            Self::Work => "https://login.microsoftonline.com/organizations/oauth2/v2.0/token",
        }
    }
}

/// Scopes needed for IMAP sync and SMTP send.
const SCOPES: &str =
    "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access";

/// Seconds before expiry at which we proactively refresh the token.
const REFRESH_MARGIN_SECS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlookTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: i64,
}

impl OutlookTokens {
    pub fn is_near_expiry(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.expires_at - now < REFRESH_MARGIN_SECS
    }
}

#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Full URL with user_code pre-filled (open this directly — no manual code entry needed).
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

pub struct OutlookAuth {
    client_id: String,
    token_ref: String,
    tenant: OutlookTenant,
}

impl OutlookAuth {
    pub fn new(client_id: String, token_ref: String, tenant: OutlookTenant) -> Self {
        Self {
            client_id,
            token_ref,
            tenant,
        }
    }

    pub fn bundled_client_id() -> Option<&'static str> {
        BUNDLED_CLIENT_ID
    }

    pub fn tenant_kind(&self) -> OutlookTenant {
        self.tenant
    }

    fn token_path(&self) -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mxr")
            .join("tokens")
            .join(format!("{}.json", self.token_ref))
    }

    pub fn load_tokens(&self) -> Result<Option<OutlookTokens>, OutlookError> {
        let path = self.token_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let tokens: OutlookTokens = serde_json::from_str(&content)?;
        Ok(Some(tokens))
    }

    pub fn save_tokens(&self, tokens: &OutlookTokens) -> Result<(), OutlookError> {
        let path = self.token_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(tokens)?;
        std::fs::write(&path, content)?;
        // Set 0600 permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Initiate the device code flow. Returns the response with `user_code` and `verification_uri`
    /// for the user to complete in a browser.
    pub async fn start_device_flow(&self) -> Result<DeviceCodeResponse, OutlookError> {
        let client = reqwest::Client::new();
        let response = client
            .post(self.tenant.device_code_url())
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", SCOPES),
            ])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(OutlookError::OAuth2(format!(
                "device code request failed ({status}): {text}"
            )));
        }

        let device_code_resp: DeviceCodeResponse = response.json().await?;
        Ok(device_code_resp)
    }

    /// Poll the token endpoint until auth completes, user declines, or the code expires.
    /// `interval` is the polling interval in seconds (from `start_device_flow` response).
    pub async fn poll_for_token(
        &self,
        device_code: &str,
        interval: u64,
    ) -> Result<OutlookTokens, OutlookError> {
        let client = reqwest::Client::new();
        let poll_interval = std::time::Duration::from_secs(interval.max(5));

        loop {
            tokio::time::sleep(poll_interval).await;

            let resp = client
                .post(self.tenant.token_url())
                .form(&[
                    ("client_id", self.client_id.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device_code),
                ])
                .send()
                .await?
                .json::<TokenResponse>()
                .await?;

            match resp.error.as_deref() {
                None => {
                    let access_token = resp
                        .access_token
                        .ok_or_else(|| OutlookError::OAuth2("no access_token in response".into()))?;
                    let refresh_token = resp
                        .refresh_token
                        .ok_or_else(|| OutlookError::OAuth2("no refresh_token in response".into()))?;
                    let expires_in = resp.expires_in.unwrap_or(3600);
                    let expires_at = chrono::Utc::now().timestamp() + expires_in as i64;
                    return Ok(OutlookTokens {
                        access_token,
                        refresh_token,
                        expires_at,
                    });
                }
                Some("authorization_pending") => {
                    // Normal — user hasn't completed auth yet, keep polling
                    continue;
                }
                Some("authorization_declined") => {
                    return Err(OutlookError::AuthDeclined);
                }
                Some("expired_token") | Some("bad_verification_code") => {
                    return Err(OutlookError::DeviceCodeExpired);
                }
                Some(other) => {
                    let desc = resp.error_description.unwrap_or_default();
                    return Err(OutlookError::OAuth2(format!("{other}: {desc}")));
                }
            }
        }
    }

    /// Refresh an expired access token using the stored refresh token.
    pub async fn refresh_access_token(
        &self,
        tokens: &OutlookTokens,
    ) -> Result<OutlookTokens, OutlookError> {
        let client = reqwest::Client::new();
        let resp = client
            .post(self.tenant.token_url())
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", tokens.refresh_token.as_str()),
                ("scope", SCOPES),
            ])
            .send()
            .await?
            .json::<TokenResponse>()
            .await?;

        if let Some(err) = resp.error {
            let desc = resp.error_description.unwrap_or_default();
            return Err(OutlookError::OAuth2(format!("{err}: {desc}")));
        }

        let access_token = resp
            .access_token
            .ok_or_else(|| OutlookError::OAuth2("no access_token in refresh response".into()))?;
        // Use new refresh_token if provided, otherwise keep the existing one
        let refresh_token = resp
            .refresh_token
            .unwrap_or_else(|| tokens.refresh_token.clone());
        let expires_in = resp.expires_in.unwrap_or(3600);
        let expires_at = chrono::Utc::now().timestamp() + expires_in as i64;

        Ok(OutlookTokens {
            access_token,
            refresh_token,
            expires_at,
        })
    }

    /// Returns a valid access token, refreshing if near expiry.
    /// Saves refreshed tokens back to disk.
    pub async fn get_valid_access_token(&self) -> Result<String, OutlookError> {
        let tokens = self
            .load_tokens()?
            .ok_or(OutlookError::TokenExpired)?;

        if tokens.is_near_expiry() {
            let refreshed = self.refresh_access_token(&tokens).await?;
            self.save_tokens(&refreshed)?;
            Ok(refreshed.access_token)
        } else {
            Ok(tokens.access_token)
        }
    }
}
