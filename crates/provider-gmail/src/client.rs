#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic and unwrap for direct fixture failures"
    )
)]

use crate::auth::{AuthError, GmailAuth};
use crate::error::GmailError;
use crate::error_response::{read_error_response, ErrorResponse, RateLimitRefusal};
use crate::rate_limit_gate::RateLimitGate;
use crate::request_log::{RequestLog, RequestSummary};
use crate::types::*;
use async_trait::async_trait;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{TimeDelta, Utc};
use mxr_core::{AccountId, ServerDraftSnapshot};
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;
use tracing::{debug, warn};

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const GMAIL_HTTP_TIMEOUT: Duration = Duration::from_secs(60);
/// Connection establishment (TCP + TLS) should never take long even on a
/// slow link — only data transfer needs the 60s ceiling. A short connect
/// timeout makes dead/half-open networks fail the sync attempt in seconds
/// instead of holding `sync_in_progress` for the full request timeout.
const GMAIL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub enum MessageFormat {
    Metadata,
    Full,
    Minimal,
}

impl MessageFormat {
    fn as_str(&self) -> &str {
        match self {
            Self::Metadata => "metadata",
            Self::Full => "full",
            Self::Minimal => "minimal",
        }
    }
}

pub struct GmailClient {
    http: reqwest::Client,
    auth: GmailAuth,
    base_url: String,
    /// Named in the rate-limit warning. Set by `GmailProvider::new`.
    account: String,
    requests: RequestLog,
    gate: RateLimitGate,
}

#[async_trait]
pub trait GmailApi: Send + Sync {
    async fn list_messages(
        &self,
        query: Option<&str>,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<GmailListResponse, GmailError>;
    async fn get_message(
        &self,
        message_id: &str,
        format: MessageFormat,
    ) -> Result<GmailMessage, GmailError>;
    async fn batch_get_messages(
        &self,
        message_ids: &[String],
        format: MessageFormat,
    ) -> Result<Vec<GmailMessage>, GmailError>;
    async fn list_history(
        &self,
        start_history_id: u64,
        page_token: Option<&str>,
    ) -> Result<GmailHistoryResponse, GmailError>;
    async fn modify_message(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<(), GmailError>;
    async fn trash_message(&self, message_id: &str) -> Result<(), GmailError>;
    async fn send_message(
        &self,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<serde_json::Value, GmailError>;
    async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, GmailError>;
    async fn create_draft(
        &self,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<String, GmailError>;
    async fn update_draft(
        &self,
        draft_id: &str,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<(), GmailError>;
    async fn fetch_draft(&self, draft_id: &str) -> Result<Option<ServerDraftSnapshot>, GmailError>;
    async fn delete_draft(&self, draft_id: &str) -> Result<(), GmailError>;
    async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError>;
    async fn create_label(&self, name: &str, color: Option<&str>)
        -> Result<GmailLabel, GmailError>;
    async fn rename_label(&self, label_id: &str, new_name: &str) -> Result<GmailLabel, GmailError>;
    async fn delete_label(&self, label_id: &str) -> Result<(), GmailError>;
}

impl GmailClient {
    pub fn new(auth: GmailAuth) -> Self {
        let http = reqwest::Client::builder()
            .timeout(GMAIL_HTTP_TIMEOUT)
            .connect_timeout(GMAIL_CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|error| {
                warn!(%error, "failed to configure Gmail HTTP timeout; using default client");
                reqwest::Client::new()
            });
        Self {
            http,
            auth,
            base_url: GMAIL_API_BASE.to_string(),
            account: "unknown".to_string(),
            requests: RequestLog::default(),
            gate: RateLimitGate::default(),
        }
    }

    pub(crate) fn with_account(mut self, account_id: &AccountId) -> Self {
        self.account = account_id.as_str();
        self
    }

    /// Override base URL (used for testing with wiremock).
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    async fn auth_header(&self) -> Result<String, GmailError> {
        let token = self
            .auth
            .access_token()
            .await
            .map_err(|error| match error {
                // Typed so the daemon can size its backoff from the hint. It is
                // reported before any mail request exists, so a send is never
                // retried because of it.
                AuthError::RefreshRateLimited { retry_after_secs } => GmailError::RateLimited {
                    retry_after_secs,
                    detail: None,
                },
                AuthError::RefreshUnavailable(_) => GmailError::AuthTemporary,
                // Not an authorization failure: keep it out of the "Auth error"
                // label so sync health does not prescribe signing in again.
                error @ AuthError::TokenCacheWrite(_) => GmailError::Other(error.into()),
                other => GmailError::Auth(other.to_string()),
            })?;
        Ok(format!("Bearer {token}"))
    }

    /// Sends one Gmail API request for this account. While Gmail's rate-limit
    /// wait is running, this returns that wait without touching the network,
    /// the token endpoint or the request count. The endpoint label is a fixed
    /// name such as `messages.get`: no ids, no query.
    async fn send(
        &self,
        endpoint: &'static str,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, GmailError> {
        let now = tokio::time::Instant::now().into_std();
        if let Some(held) = self.gate.held(now) {
            return Err(send_preparation_error(
                endpoint,
                mxr_core::SendSubmissionPhase::Cooldown,
                self.held_error(held),
            ));
        }
        let authorization = self.auth_header().await.map_err(|error| {
            send_preparation_error(
                endpoint,
                mxr_core::SendSubmissionPhase::Authentication,
                error,
            )
        })?;
        let request = request.header("Authorization", authorization);
        self.requests.record(endpoint, now);
        Ok(request.send().await?)
    }

    /// The gate's error. Its retry is whole seconds, at least 1, so a caller
    /// that sleeps on it never wakes before Gmail's wait has passed.
    fn held_error(&self, held: crate::rate_limit_gate::Held) -> GmailError {
        let remaining = held.remaining;
        let secs = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
        let resume = TimeDelta::try_seconds(i64::try_from(secs).unwrap_or(i64::MAX))
            .and_then(|delta| Utc::now().checked_add_signed(delta));
        let when = resume.map_or_else(
            || "a later time".to_string(),
            |at| format!("{} UTC", at.format("%H:%M:%S")),
        );
        let original = held
            .detail
            .unwrap_or_else(|| "Gmail rate limit".to_string());
        GmailError::RateLimited {
            retry_after_secs: secs.max(1),
            detail: Some(format!("Gmail asked mxr to wait until {when}; {original}")),
        }
    }

    async fn handle_error(&self, endpoint: &'static str, resp: reqwest::Response) -> GmailError {
        match read_error_response(resp).await {
            ErrorResponse::RateLimited(refusal) => self.rate_limited(endpoint, refusal),
            other => other.into(),
        }
    }

    /// Builds the typed rate limit and writes the one WARN line for it. The
    /// detail (endpoint, Gmail's reason, the count) goes into the error text too,
    /// so sync status shows it without a log search.
    fn rate_limited(&self, endpoint: &'static str, refusal: RateLimitRefusal) -> GmailError {
        let now = tokio::time::Instant::now().into_std();
        let summary = self.requests.summary(now);
        let detail = refused_detail(endpoint, &refusal, &summary);
        self.gate
            .refused(now, refusal.retry_after_secs, Some(detail.clone()));
        warn!(
            account = %self.account,
            endpoint,
            reason = refusal.reason.as_deref().unwrap_or("none"),
            retry_after_secs = refusal.retry_after_secs,
            requests_last_minute = summary.last_minute,
            requests_last_hour = summary.last_hour,
            requests_by_endpoint_last_hour = %summary.endpoint_counts(),
            "Gmail refused a request with a rate limit"
        );
        GmailError::RateLimited {
            retry_after_secs: refusal.retry_after_secs,
            detail: Some(detail),
        }
    }

    pub async fn list_messages(
        &self,
        query: Option<&str>,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<GmailListResponse, GmailError> {
        let mut url = format!("{}/messages?maxResults={max_results}", self.base_url);
        if let Some(q) = query {
            url.push_str(&format!("&q={}", urlencoding::encode(q)));
        }
        if let Some(pt) = page_token {
            url.push_str(&format!("&pageToken={pt}"));
        }

        debug!(url = %url, "listing messages");

        let resp = self.send("messages.list", self.http.get(&url)).await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.list", resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn get_message(
        &self,
        message_id: &str,
        format: MessageFormat,
    ) -> Result<GmailMessage, GmailError> {
        let url = format!(
            "{}/messages/{message_id}?format={}",
            self.base_url,
            format.as_str()
        );

        let resp = self.send("messages.get", self.http.get(&url)).await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.get", resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn batch_get_messages(
        &self,
        message_ids: &[String],
        format: MessageFormat,
    ) -> Result<Vec<GmailMessage>, GmailError> {
        let mut messages = Vec::with_capacity(message_ids.len());

        // Fetch in small chunks to avoid rate limits.
        // 10 concurrent requests per chunk is conservative.
        for chunk in message_ids.chunks(10) {
            let futs: Vec<_> = chunk
                .iter()
                .map(|id| self.get_message(id, format))
                .collect();
            let results = futures::future::join_all(futs).await;
            for result in results {
                match result {
                    Ok(message) => messages.push(message),
                    Err(GmailError::NotFound(body)) => {
                        warn!(
                            error = %body,
                            "gmail message vanished before fetch during sync; skipping"
                        );
                    }
                    Err(error) => return Err(error),
                }
            }
        }

        Ok(messages)
    }

    pub async fn list_history(
        &self,
        start_history_id: u64,
        page_token: Option<&str>,
    ) -> Result<GmailHistoryResponse, GmailError> {
        let mut url = format!(
            "{}/history?startHistoryId={start_history_id}",
            self.base_url
        );
        if let Some(pt) = page_token {
            url.push_str(&format!("&pageToken={pt}"));
        }

        let resp = self.send("history.list", self.http.get(&url)).await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("history.list", resp).await);
        }

        Ok(resp.json().await?)
    }

    /// Modify labels on a single message.
    pub async fn modify_message(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<(), GmailError> {
        let url = format!("{}/messages/{message_id}/modify", self.base_url);

        let body = serde_json::json!({
            "addLabelIds": add_labels,
            "removeLabelIds": remove_labels,
        });

        let resp = self
            .send("messages.modify", self.http.post(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.modify", resp).await);
        }

        Ok(())
    }

    /// Batch modify labels on multiple messages.
    pub async fn batch_modify_messages(
        &self,
        message_ids: &[String],
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<(), GmailError> {
        let url = format!("{}/messages/batchModify", self.base_url);

        let body = serde_json::json!({
            "ids": message_ids,
            "addLabelIds": add_labels,
            "removeLabelIds": remove_labels,
        });

        let resp = self
            .send("messages.batchModify", self.http.post(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.batchModify", resp).await);
        }

        Ok(())
    }

    /// Trash a message.
    pub async fn trash_message(&self, message_id: &str) -> Result<(), GmailError> {
        let url = format!("{}/messages/{message_id}/trash", self.base_url);

        let resp = self
            .send(
                "messages.trash",
                self.http
                    .post(&url)
                    .header(reqwest::header::CONTENT_LENGTH, "0")
                    .body(Vec::new()),
            )
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.trash", resp).await);
        }

        Ok(())
    }

    /// Send a message via Gmail API. `thread_id` is Gmail's native thread id;
    /// when set, the sent message is filed on that conversation.
    pub async fn send_message(
        &self,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<serde_json::Value, GmailError> {
        let url = format!("{}/messages/send", self.base_url);

        let body = message_body(raw_base64url, thread_id);

        let resp = self
            .send("messages.send", self.http.post(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.send", resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, GmailError> {
        let url = format!(
            "{}/messages/{}/attachments/{}",
            self.base_url, message_id, attachment_id
        );

        let resp = self
            .send("messages.attachments.get", self.http.get(&url))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("messages.attachments.get", resp).await);
        }

        let json: serde_json::Value = resp.json().await?;
        let data = json["data"]
            .as_str()
            .ok_or_else(|| GmailError::Parse("Missing attachment data field".into()))?;

        decode_attachment_data(data)
    }

    /// Create a draft in Gmail. Returns the draft ID.
    ///
    /// Gmail files drafts on a conversation only by `threadId`; unlike sends,
    /// the In-Reply-To/References headers inside `raw` are not enough.
    pub async fn create_draft(
        &self,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<String, GmailError> {
        let url = format!("{}/drafts", self.base_url);

        let body = draft_body(raw_base64url, thread_id);

        let resp = self
            .send("drafts.create", self.http.post(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("drafts.create", resp).await);
        }

        let json: serde_json::Value = resp.json().await?;
        let draft_id = json["id"].as_str().unwrap_or("unknown").to_string();
        Ok(draft_id)
    }

    /// Replace a Gmail draft's message while preserving the stable draft ID.
    pub async fn update_draft(
        &self,
        draft_id: &str,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<(), GmailError> {
        let url = format!("{}/drafts/{draft_id}", self.base_url);
        let body = draft_body(raw_base64url, thread_id);

        let resp = self
            .send("drafts.update", self.http.put(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("drafts.update", resp).await);
        }

        Ok(())
    }

    pub async fn fetch_draft(
        &self,
        draft_id: &str,
    ) -> Result<Option<ServerDraftSnapshot>, GmailError> {
        let url = format!("{}/drafts/{draft_id}?format=raw", self.base_url);
        let resp = self.send("drafts.get", self.http.get(&url)).await?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(self.handle_error("drafts.get", resp).await);
        }

        let json: serde_json::Value = resp.json().await?;
        let revision = json["message"]["id"]
            .as_str()
            .ok_or_else(|| GmailError::Parse("draft response missing message.id".into()))?;
        let encoded = json["message"]["raw"]
            .as_str()
            .ok_or_else(|| GmailError::Parse("draft response missing message.raw".into()))?;
        let raw_rfc822 = URL_SAFE_NO_PAD
            .decode(encoded)
            .or_else(|_| URL_SAFE.decode(encoded))
            .map_err(|error| GmailError::Parse(format!("invalid draft MIME encoding: {error}")))?;
        Ok(Some(ServerDraftSnapshot {
            revision: revision.to_string(),
            raw_rfc822,
        }))
    }

    pub async fn delete_draft(&self, draft_id: &str) -> Result<(), GmailError> {
        let url = format!("{}/drafts/{draft_id}", self.base_url);
        let resp = self.send("drafts.delete", self.http.delete(&url)).await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("drafts.delete", resp).await);
        }
        Ok(())
    }

    pub async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
        let url = format!("{}/labels", self.base_url);

        let resp = self.send("labels.list", self.http.get(&url)).await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("labels.list", resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn create_label(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<GmailLabel, GmailError> {
        let url = format!("{}/labels", self.base_url);
        let mut body = serde_json::json!({
            "name": name,
            "labelListVisibility": "labelShow",
            "messageListVisibility": "show",
        });
        if let Some(color) = color {
            body["color"] = serde_json::json!({
                "backgroundColor": color,
                "textColor": "#000000",
            });
        }

        let resp = self
            .send("labels.create", self.http.post(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("labels.create", resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn rename_label(
        &self,
        label_id: &str,
        new_name: &str,
    ) -> Result<GmailLabel, GmailError> {
        let url = format!("{}/labels/{label_id}", self.base_url);
        let body = serde_json::json!({
            "name": new_name,
        });

        let resp = self
            .send("labels.patch", self.http.patch(&url).json(&body))
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("labels.patch", resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn delete_label(&self, label_id: &str) -> Result<(), GmailError> {
        let url = format!("{}/labels/{label_id}", self.base_url);

        let resp = self.send("labels.delete", self.http.delete(&url)).await?;

        if !resp.status().is_success() {
            return Err(self.handle_error("labels.delete", resp).await);
        }

        Ok(())
    }
}

fn send_preparation_error(
    endpoint: &str,
    phase: mxr_core::SendSubmissionPhase,
    error: GmailError,
) -> GmailError {
    if endpoint == "messages.send" {
        GmailError::NotSubmitted {
            phase,
            source: Box::new(error),
        }
    } else {
        error
    }
}

/// Body for `messages.send`: `{"raw": ..}` plus `threadId` when known.
/// `messages.get (rateLimitExceeded: User-rate limit exceeded; 412 requests in the last hour)`.
fn refused_detail(endpoint: &str, refusal: &RateLimitRefusal, summary: &RequestSummary) -> String {
    let requests = format!("{} requests in the last hour", summary.last_hour);
    let inside = match refusal.describe() {
        Some(what) => format!("{what}; {requests}"),
        None => requests,
    };
    format!("Gmail {endpoint} ({inside})")
}

fn message_body(raw_base64url: &str, thread_id: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({ "raw": raw_base64url });
    if let Some(thread_id) = thread_id {
        body["threadId"] = serde_json::Value::String(thread_id.to_string());
    }
    body
}

/// Body for `drafts.create` / `drafts.update`: the message nested under
/// `"message"`, with `threadId` alongside `raw` when known.
fn draft_body(raw_base64url: &str, thread_id: Option<&str>) -> serde_json::Value {
    serde_json::json!({ "message": message_body(raw_base64url, thread_id) })
}

fn decode_attachment_data(data: &str) -> Result<Vec<u8>, GmailError> {
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    use base64::Engine;

    URL_SAFE_NO_PAD
        .decode(data)
        .or_else(|_| URL_SAFE.decode(data))
        .map_err(|e| GmailError::Parse(format!("Base64 decode error: {e}")))
}

#[async_trait]
impl GmailApi for GmailClient {
    async fn list_messages(
        &self,
        query: Option<&str>,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<GmailListResponse, GmailError> {
        Self::list_messages(self, query, page_token, max_results).await
    }

    async fn get_message(
        &self,
        message_id: &str,
        format: MessageFormat,
    ) -> Result<GmailMessage, GmailError> {
        Self::get_message(self, message_id, format).await
    }

    async fn batch_get_messages(
        &self,
        message_ids: &[String],
        format: MessageFormat,
    ) -> Result<Vec<GmailMessage>, GmailError> {
        Self::batch_get_messages(self, message_ids, format).await
    }

    async fn list_history(
        &self,
        start_history_id: u64,
        page_token: Option<&str>,
    ) -> Result<GmailHistoryResponse, GmailError> {
        Self::list_history(self, start_history_id, page_token).await
    }

    async fn modify_message(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<(), GmailError> {
        Self::modify_message(self, message_id, add_labels, remove_labels).await
    }

    async fn trash_message(&self, message_id: &str) -> Result<(), GmailError> {
        Self::trash_message(self, message_id).await
    }

    async fn send_message(
        &self,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<serde_json::Value, GmailError> {
        Self::send_message(self, raw_base64url, thread_id).await
    }

    async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, GmailError> {
        Self::get_attachment(self, message_id, attachment_id).await
    }

    async fn create_draft(
        &self,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<String, GmailError> {
        Self::create_draft(self, raw_base64url, thread_id).await
    }

    async fn update_draft(
        &self,
        draft_id: &str,
        raw_base64url: &str,
        thread_id: Option<&str>,
    ) -> Result<(), GmailError> {
        Self::update_draft(self, draft_id, raw_base64url, thread_id).await
    }

    async fn fetch_draft(&self, draft_id: &str) -> Result<Option<ServerDraftSnapshot>, GmailError> {
        Self::fetch_draft(self, draft_id).await
    }

    async fn delete_draft(&self, draft_id: &str) -> Result<(), GmailError> {
        Self::delete_draft(self, draft_id).await
    }

    async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
        Self::list_labels(self).await
    }

    async fn create_label(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<GmailLabel, GmailError> {
        Self::create_label(self, name, color).await
    }

    async fn rename_label(&self, label_id: &str, new_name: &str) -> Result<GmailLabel, GmailError> {
        Self::rename_label(self, label_id, new_name).await
    }

    async fn delete_label(&self, label_id: &str) -> Result<(), GmailError> {
        Self::delete_label(self, label_id).await
    }
}

/// URL encoding helper — minimal, just for query params.
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut encoded = String::with_capacity(input.len());
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char);
                }
                _ => {
                    encoded.push_str(&format!("%{byte:02X}"));
                }
            }
        }
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use mxr_core::MxrError;
    use std::any::Any;
    use std::panic::AssertUnwindSafe;
    use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // For tests, we need a GmailClient that doesn't need real OAuth.
    // We'll intercept at the HTTP level via wiremock.
    // The auth will fail, but wiremock won't check the Authorization header
    // unless we tell it to. However, the auth.access_token() call in the client
    // will fail because there's no authenticator set up.
    //
    // Solution: Create a special test client that bypasses auth.
    struct TestGmailClient {
        http: reqwest::Client,
        base_url: String,
        token: String,
    }

    impl TestGmailClient {
        fn new(base_url: String) -> Self {
            Self {
                http: reqwest::Client::new(),
                base_url,
                token: "test-token-12345".to_string(),
            }
        }

        fn auth_header(&self) -> String {
            format!("Bearer {}", self.token)
        }

        async fn handle_error(&self, resp: reqwest::Response) -> GmailError {
            read_error_response(resp).await.into()
        }

        async fn list_messages(
            &self,
            query: Option<&str>,
            page_token: Option<&str>,
            max_results: u32,
        ) -> Result<GmailListResponse, GmailError> {
            let mut url = format!("{}/messages?maxResults={max_results}", self.base_url);
            if let Some(q) = query {
                url.push_str(&format!("&q={}", urlencoding::encode(q)));
            }
            if let Some(pt) = page_token {
                url.push_str(&format!("&pageToken={pt}"));
            }

            let resp = self
                .http
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(self.handle_error(resp).await);
            }

            Ok(resp.json().await?)
        }
    }

    async fn start_mock_server() -> Option<MockServer> {
        match AssertUnwindSafe(MockServer::start()).catch_unwind().await {
            Ok(server) => Some(server),
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                if message.contains("Failed to bind an OS port")
                    || message.contains("Operation not permitted")
                    || message.contains("PermissionDenied")
                {
                    eprintln!("skipping wiremock test: {message}");
                    None
                } else {
                    std::panic::resume_unwind(payload);
                }
            }
        }
    }

    fn panic_message(payload: &(dyn Any + Send)) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }

        if let Some(message) = payload.downcast_ref::<&str>() {
            return (*message).to_string();
        }

        "unknown panic payload".to_string()
    }

    #[tokio::test]
    async fn client_error_handling() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        // 401 Unauthorized
        Mock::given(method("GET"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .expect(1)
            .named("401")
            .mount(&server)
            .await;

        let client = TestGmailClient::new(server.uri());
        let err = client.list_messages(None, None, 10).await.unwrap_err();
        assert!(matches!(err, GmailError::AuthExpired));

        server.reset().await;

        // 404 Not Found
        Mock::given(method("GET"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(404).set_body_string("message not found"))
            .expect(1)
            .mount(&server)
            .await;

        let err = client.list_messages(None, None, 10).await.unwrap_err();
        assert!(matches!(err, GmailError::NotFound(_)));

        server.reset().await;

        // 429 Rate Limited
        Mock::given(method("GET"))
            .and(path("/messages"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "30")
                    .set_body_string("rate limited"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client.list_messages(None, None, 10).await.unwrap_err();
        match err {
            GmailError::RateLimited {
                retry_after_secs,
                detail,
            } => {
                assert_eq!(retry_after_secs, 30);
                assert_eq!(detail, None, "a plain-text body names no one");
            }
            other => panic!("Expected RateLimited, got {other:?}"),
        }
    }

    impl TestGmailClient {
        async fn get_message(
            &self,
            message_id: &str,
            format: MessageFormat,
        ) -> Result<GmailMessage, GmailError> {
            let url = format!(
                "{}/messages/{message_id}?format={}",
                self.base_url,
                format.as_str()
            );

            let resp = self
                .http
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(self.handle_error(resp).await);
            }

            Ok(resp.json().await?)
        }

        async fn list_history(
            &self,
            start_history_id: u64,
            page_token: Option<&str>,
        ) -> Result<GmailHistoryResponse, GmailError> {
            let mut url = format!(
                "{}/history?startHistoryId={start_history_id}",
                self.base_url
            );
            if let Some(pt) = page_token {
                url.push_str(&format!("&pageToken={pt}"));
            }

            let resp = self
                .http
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(self.handle_error(resp).await);
            }

            Ok(resp.json().await?)
        }

        async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
            let url = format!("{}/labels", self.base_url);

            let resp = self
                .http
                .get(&url)
                .header("Authorization", self.auth_header())
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(self.handle_error(resp).await);
            }

            Ok(resp.json().await?)
        }
    }

    #[test]
    fn send_and_draft_bodies_carry_thread_id_only_when_known() {
        assert_eq!(
            message_body("cmF3", None),
            serde_json::json!({ "raw": "cmF3" })
        );
        assert_eq!(
            message_body("cmF3", Some("t-1")),
            serde_json::json!({ "raw": "cmF3", "threadId": "t-1" })
        );
        assert_eq!(
            draft_body("cmF3", None),
            serde_json::json!({ "message": { "raw": "cmF3" } })
        );
        assert_eq!(
            draft_body("cmF3", Some("t-1")),
            serde_json::json!({ "message": { "raw": "cmF3", "threadId": "t-1" } })
        );
    }

    #[tokio::test]
    async fn list_messages_single_page() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        Mock::given(method("GET"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [
                    {"id": "msg1", "threadId": "t1"},
                    {"id": "msg2", "threadId": "t2"}
                ],
                "resultSizeEstimate": 2
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = TestGmailClient::new(server.uri());
        let resp = client.list_messages(None, None, 10).await.unwrap();

        let msgs = resp.messages.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id, "msg1");
        assert_eq!(msgs[1].id, "msg2");
        assert!(resp.next_page_token.is_none());
    }

    #[tokio::test]
    async fn get_attachment_accepts_padded_base64url_data() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        Mock::given(method("GET"))
            .and(path("/messages/msg-123/attachments/att-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": "JVBERg=="
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client =
            GmailClient::new(GmailAuth::for_test_token("test-token")).with_base_url(server.uri());

        let bytes = client.get_attachment("msg-123", "att-1").await.unwrap();

        assert_eq!(bytes, b"%PDF");
    }

    #[tokio::test]
    async fn trash_message_sends_empty_post_with_content_length() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        Mock::given(method("POST"))
            .and(path("/messages/msg-123/trash"))
            .and(header("content-length", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg-123",
                "threadId": "thread-1"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client =
            GmailClient::new(GmailAuth::for_test_token("test-token")).with_base_url(server.uri());

        client.trash_message("msg-123").await.unwrap();
    }

    /// Storage holding an expired token; `fail_writes` makes saving a refreshed
    /// one fail.
    #[derive(Clone)]
    struct ExpiredTokens {
        fail_writes: bool,
    }

    #[async_trait::async_trait]
    impl yup_oauth2::storage::TokenStorage for ExpiredTokens {
        async fn set(
            &self,
            _: &[&str],
            _: yup_oauth2::storage::TokenInfo,
        ) -> Result<(), yup_oauth2::storage::TokenStorageError> {
            if self.fail_writes {
                return Err(yup_oauth2::storage::TokenStorageError::Other(
                    "disk full".into(),
                ));
            }
            Ok(())
        }

        async fn get(&self, _: &[&str]) -> Option<yup_oauth2::storage::TokenInfo> {
            Some(yup_oauth2::storage::TokenInfo {
                access_token: Some("expired-access".into()),
                refresh_token: Some("synthetic-refresh".into()),
                expires_at: Some(std::time::SystemTime::UNIX_EPOCH.into()),
                id_token: None,
            })
        }
    }

    #[tokio::test]
    async fn token_endpoint_throttle_becomes_a_typed_rate_limit_before_any_mail_request() {
        let Some(token_server) = start_mock_server().await else {
            return;
        };
        let Some(mail_server) = start_mock_server().await else {
            return;
        };
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
            .expect(3)
            .mount(&token_server)
            .await;
        let auth = GmailAuth::for_test_storage(
            Box::new(ExpiredTokens { fail_writes: false }),
            format!("{}/token", token_server.uri()),
        );
        let client = GmailClient::new(auth).with_base_url(mail_server.uri());

        let read = client.get_attachment("msg-1", "att-1").await.unwrap_err();
        let write = client.trash_message("msg-1").await.unwrap_err();
        let send = client.send_message("cmF3", None).await.unwrap_err();

        for error in [read, write, send] {
            let error = match error {
                GmailError::NotSubmitted { source, .. } => *source,
                error => error,
            };
            assert!(
                matches!(
                    error,
                    GmailError::RateLimited {
                        retry_after_secs: 120,
                        ..
                    }
                ),
                "{error:?}"
            );
            assert!(matches!(
                MxrError::from(error),
                MxrError::RateLimited {
                    retry_after_secs: 120,
                    ..
                }
            ));
        }
        // The throttle is reported, never retried: three calls, three token
        // requests, and no mail request (a send included) ever left the client.
        assert_eq!(token_server.received_requests().await.unwrap().len(), 3);
        assert!(mail_server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn token_cache_write_failure_is_not_labelled_an_auth_error() {
        let Some(token_server) = start_mock_server().await else {
            return;
        };
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new-access", "token_type": "Bearer", "expires_in": 3600
            })))
            .mount(&token_server)
            .await;
        let auth = GmailAuth::for_test_storage(
            Box::new(ExpiredTokens { fail_writes: true }),
            format!("{}/token", token_server.uri()),
        );
        let client = GmailClient::new(auth).with_base_url("http://127.0.0.1:9".into());

        let message = MxrError::from(client.trash_message("msg-1").await.unwrap_err()).to_string();

        assert!(
            message.contains("Local token cache write failed"),
            "{message}"
        );
        assert!(!message.to_ascii_lowercase().contains("auth"), "{message}");
    }

    #[tokio::test]
    async fn get_message_metadata() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        Mock::given(method("GET"))
            .and(path("/messages/msg-123"))
            .and(query_param("format", "metadata"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "msg-123",
                "threadId": "thread-1",
                "labelIds": ["INBOX", "UNREAD"],
                "snippet": "Hello world",
                "historyId": "99999",
                "internalDate": "1700000000000",
                "sizeEstimate": 2048,
                "payload": {
                    "mimeType": "text/plain",
                    "headers": [
                        {"name": "From", "value": "Alice <alice@example.com>"},
                        {"name": "Subject", "value": "Test"}
                    ]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = TestGmailClient::new(server.uri());
        let msg = client
            .get_message("msg-123", MessageFormat::Metadata)
            .await
            .unwrap();

        assert_eq!(msg.id, "msg-123");
        assert_eq!(msg.thread_id, "thread-1");
        assert_eq!(msg.label_ids.as_ref().unwrap().len(), 2);
        assert_eq!(msg.snippet, Some("Hello world".to_string()));
        assert_eq!(msg.size_estimate, Some(2048));
    }

    #[tokio::test]
    async fn list_history_delta() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        Mock::given(method("GET"))
            .and(path("/history"))
            .and(query_param("startHistoryId", "12345"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "history": [
                    {
                        "id": "12346",
                        "messagesAdded": [
                            {"message": {"id": "new-msg-1", "threadId": "t1"}}
                        ]
                    },
                    {
                        "id": "12347",
                        "messagesDeleted": [
                            {"message": {"id": "old-msg-1", "threadId": "t2"}}
                        ]
                    },
                    {
                        "id": "12348",
                        "labelsAdded": [
                            {
                                "message": {"id": "msg-3", "threadId": "t3"},
                                "labelIds": ["STARRED"]
                            }
                        ],
                        "labelsRemoved": [
                            {
                                "message": {"id": "msg-3", "threadId": "t3"},
                                "labelIds": ["UNREAD"]
                            }
                        ]
                    }
                ],
                "historyId": "12348"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = TestGmailClient::new(server.uri());
        let resp = client.list_history(12345, None).await.unwrap();

        let history = resp.history.unwrap();
        assert_eq!(history.len(), 3);

        // Verify added
        let added = history[0].messages_added.as_ref().unwrap();
        assert_eq!(added[0].message.id, "new-msg-1");

        // Verify deleted
        let deleted = history[1].messages_deleted.as_ref().unwrap();
        assert_eq!(deleted[0].message.id, "old-msg-1");

        // Verify label changes
        let labels_added = history[2].labels_added.as_ref().unwrap();
        assert_eq!(labels_added[0].label_ids.as_ref().unwrap()[0], "STARRED");
        let labels_removed = history[2].labels_removed.as_ref().unwrap();
        assert_eq!(labels_removed[0].label_ids.as_ref().unwrap()[0], "UNREAD");

        assert_eq!(resp.history_id, Some("12348".to_string()));
    }

    #[tokio::test]
    async fn list_labels_response() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        Mock::given(method("GET"))
            .and(path("/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "labels": [
                    {
                        "id": "INBOX",
                        "name": "INBOX",
                        "type": "system",
                        "messagesTotal": 100,
                        "messagesUnread": 5
                    },
                    {
                        "id": "Label_1",
                        "name": "Work",
                        "type": "user",
                        "messagesTotal": 42,
                        "messagesUnread": 3,
                        "color": {
                            "textColor": "#000000",
                            "backgroundColor": "#16a765"
                        }
                    }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = TestGmailClient::new(server.uri());
        let resp = client.list_labels().await.unwrap();

        let labels = resp.labels.unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].id, "INBOX");
        assert_eq!(labels[0].messages_total, Some(100));
        assert_eq!(labels[0].messages_unread, Some(5));
        assert_eq!(labels[1].name, "Work");
        assert!(labels[1].color.is_some());
    }

    #[tokio::test]
    async fn client_pagination() {
        let Some(server) = start_mock_server().await else {
            return;
        };

        // Page 1 (no pageToken param)
        Mock::given(method("GET"))
            .and(path("/messages"))
            .and(query_param("maxResults", "2"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [
                    {"id": "msg1", "threadId": "t1"},
                    {"id": "msg2", "threadId": "t2"}
                ],
                "nextPageToken": "page2token",
                "resultSizeEstimate": 6
            })))
            .expect(1)
            .mount(&server)
            .await;

        // Page 2
        Mock::given(method("GET"))
            .and(path("/messages"))
            .and(query_param("pageToken", "page2token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [
                    {"id": "msg3", "threadId": "t3"},
                    {"id": "msg4", "threadId": "t4"}
                ],
                "nextPageToken": "page3token",
                "resultSizeEstimate": 6
            })))
            .expect(1)
            .mount(&server)
            .await;

        // Page 3 (last)
        Mock::given(method("GET"))
            .and(path("/messages"))
            .and(query_param("pageToken", "page3token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [
                    {"id": "msg5", "threadId": "t5"},
                    {"id": "msg6", "threadId": "t6"}
                ],
                "resultSizeEstimate": 6
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = TestGmailClient::new(server.uri());

        // Paginate through all pages
        let mut all_ids = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let resp = client
                .list_messages(None, page_token.as_deref(), 2)
                .await
                .unwrap();

            if let Some(msgs) = resp.messages {
                for m in &msgs {
                    all_ids.push(m.id.clone());
                }
            }

            match resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        assert_eq!(
            all_ids,
            vec!["msg1", "msg2", "msg3", "msg4", "msg5", "msg6"]
        );
    }

    /// A client on the real request path, with a fixed token. The test double
    /// above skips `send()`, so the gate is only exercised through this.
    fn gated_client(server: &MockServer) -> GmailClient {
        GmailClient::new(crate::auth::GmailAuth::for_test_token("test-token"))
            .with_base_url(server.uri())
    }

    async fn labels_refusal(server: &MockServer, retry_after: &str) {
        Mock::given(method("GET"))
            .and(path("/labels"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", retry_after)
                    .set_body_string(
                        r#"{"error":{"code":429,"message":"User-rate limit exceeded.","errors":[{"domain":"usageLimits","reason":"rateLimitExceeded"}]}}"#,
                    ),
            )
            .up_to_n_times(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn a_refusal_holds_every_later_call_without_a_request() {
        let Some(server) = start_mock_server().await else {
            return;
        };
        labels_refusal(&server, "900").await;
        // Mounted only so a request that slips through is a 404 NotFound, not
        // a silent pass. None of these may be reached.
        Mock::given(method("GET"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(0)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/messages/m1/trash"))
            .respond_with(ResponseTemplate::new(204))
            .expect(0)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/messages/send"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(0)
            .mount(&server)
            .await;

        let client = gated_client(&server);
        let first = client.list_labels().await.unwrap_err();
        assert!(matches!(
            first,
            GmailError::RateLimited {
                retry_after_secs: 900,
                ..
            }
        ));

        let held = client.list_messages(None, None, 10).await.unwrap_err();
        let GmailError::RateLimited {
            retry_after_secs,
            detail,
        } = held
        else {
            panic!("expected the gate's RateLimited, got {held:?}");
        };
        assert!(
            (899..=900).contains(&retry_after_secs),
            "{retry_after_secs}"
        );
        let detail = detail.unwrap();
        assert!(
            detail.contains("Gmail asked mxr to wait until "),
            "{detail}"
        );
        assert!(detail.contains(" UTC; Gmail labels.list"), "{detail}");

        let trash = client.trash_message("m1").await.unwrap_err();
        assert!(matches!(trash, GmailError::RateLimited { .. }), "{trash:?}");
        let send = client.send_message("ZmFrZQ", None).await.unwrap_err();
        assert!(
            matches!(&send, GmailError::NotSubmitted {
            phase: mxr_core::SendSubmissionPhase::Cooldown, source,
        } if matches!(source.as_ref(), GmailError::RateLimited { .. })),
            "{send:?}"
        );

        assert_eq!(
            client.requests.summary(Instant::now()).last_hour,
            1,
            "held calls are not counted as requests"
        );
    }

    #[tokio::test]
    async fn a_call_after_the_wait_goes_out_and_a_new_refusal_extends_it() {
        let Some(server) = start_mock_server().await else {
            return;
        };
        let client = gated_client(&server);
        // A wait that ended a second ago, as if Gmail had asked for it long ago.
        let long_ago = Instant::now() - Duration::from_secs(60);
        client.gate.refused(long_ago, 1, Some("old".into()));

        Mock::given(method("GET"))
            .and(path("/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"labels":[]}"#))
            .expect(1)
            .mount(&server)
            .await;
        client.list_labels().await.unwrap();

        server.reset().await;
        labels_refusal(&server, "1200").await;
        let refused = client.list_labels().await.unwrap_err();
        assert!(matches!(refused, GmailError::RateLimited { .. }));

        let held = client
            .gate
            .held(Instant::now())
            .expect("the new refusal holds");
        assert!(
            held.remaining > Duration::from_secs(1100),
            "{:?}",
            held.remaining
        );
    }
}
