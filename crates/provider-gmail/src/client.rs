use crate::auth::GmailAuth;
use crate::error::GmailError;
use crate::types::*;
use async_trait::async_trait;
use tracing::{debug, warn};

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

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
}

#[async_trait]
pub trait GmailApi: Send + Sync {
    async fn list_messages(
        &self,
        query: Option<&str>,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<GmailListResponse, GmailError>;
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
    async fn send_message(&self, raw_base64url: &str) -> Result<serde_json::Value, GmailError>;
    async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, GmailError>;
    async fn create_draft(&self, raw_base64url: &str) -> Result<String, GmailError>;
    async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError>;
    async fn create_label(&self, name: &str, color: Option<&str>)
        -> Result<GmailLabel, GmailError>;
    async fn rename_label(&self, label_id: &str, new_name: &str) -> Result<GmailLabel, GmailError>;
    async fn delete_label(&self, label_id: &str) -> Result<(), GmailError>;
}

impl GmailClient {
    pub fn new(auth: GmailAuth) -> Self {
        Self {
            http: reqwest::Client::new(),
            auth,
            base_url: GMAIL_API_BASE.to_string(),
        }
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
            .map_err(|e| GmailError::Auth(e.to_string()))?;
        Ok(format!("Bearer {token}"))
    }

    async fn handle_error(&self, resp: reqwest::Response) -> GmailError {
        let status = resp.status().as_u16();
        match status {
            401 => GmailError::AuthExpired,
            404 => {
                let body = resp.text().await.unwrap_or_default();
                GmailError::NotFound(body)
            }
            429 => {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60);
                GmailError::RateLimited {
                    retry_after_secs: retry_after,
                }
            }
            _ => {
                let body = resp.text().await.unwrap_or_default();
                GmailError::Api { status, body }
            }
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

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(())
    }

    /// Trash a message.
    pub async fn trash_message(&self, message_id: &str) -> Result<(), GmailError> {
        let url = format!("{}/messages/{message_id}/trash", self.base_url);

        let resp = self
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(())
    }

    /// Send a message via Gmail API.
    pub async fn send_message(&self, raw_base64url: &str) -> Result<serde_json::Value, GmailError> {
        let url = format!("{}/messages/send", self.base_url);

        let body = serde_json::json!({ "raw": raw_base64url });

        let resp = self
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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
            .http
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        let json: serde_json::Value = resp.json().await?;
        let data = json["data"]
            .as_str()
            .ok_or_else(|| GmailError::Parse("Missing attachment data field".into()))?;

        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let bytes = URL_SAFE_NO_PAD
            .decode(data)
            .map_err(|e| GmailError::Parse(format!("Base64 decode error: {e}")))?;
        Ok(bytes)
    }

    /// Create a draft in Gmail. Returns the draft ID.
    pub async fn create_draft(&self, raw_base64url: &str) -> Result<String, GmailError> {
        let url = format!("{}/drafts", self.base_url);

        let body = serde_json::json!({
            "message": {
                "raw": raw_base64url
            }
        });

        let resp = self
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        let json: serde_json::Value = resp.json().await?;
        let draft_id = json["id"].as_str().unwrap_or("unknown").to_string();
        Ok(draft_id)
    }

    pub async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
        let url = format!("{}/labels", self.base_url);

        let resp = self
            .http
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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
            .http
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
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
            .http
            .patch(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(resp.json().await?)
    }

    pub async fn delete_label(&self, label_id: &str) -> Result<(), GmailError> {
        let url = format!("{}/labels/{label_id}", self.base_url);

        let resp = self
            .http
            .delete(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(self.handle_error(resp).await);
        }

        Ok(())
    }
}

#[async_trait]
impl GmailApi for GmailClient {
    async fn list_messages(
        &self,
        query: Option<&str>,
        page_token: Option<&str>,
        max_results: u32,
    ) -> Result<GmailListResponse, GmailError> {
        GmailClient::list_messages(self, query, page_token, max_results).await
    }

    async fn batch_get_messages(
        &self,
        message_ids: &[String],
        format: MessageFormat,
    ) -> Result<Vec<GmailMessage>, GmailError> {
        GmailClient::batch_get_messages(self, message_ids, format).await
    }

    async fn list_history(
        &self,
        start_history_id: u64,
        page_token: Option<&str>,
    ) -> Result<GmailHistoryResponse, GmailError> {
        GmailClient::list_history(self, start_history_id, page_token).await
    }

    async fn modify_message(
        &self,
        message_id: &str,
        add_labels: &[&str],
        remove_labels: &[&str],
    ) -> Result<(), GmailError> {
        GmailClient::modify_message(self, message_id, add_labels, remove_labels).await
    }

    async fn trash_message(&self, message_id: &str) -> Result<(), GmailError> {
        GmailClient::trash_message(self, message_id).await
    }

    async fn send_message(&self, raw_base64url: &str) -> Result<serde_json::Value, GmailError> {
        GmailClient::send_message(self, raw_base64url).await
    }

    async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<Vec<u8>, GmailError> {
        GmailClient::get_attachment(self, message_id, attachment_id).await
    }

    async fn create_draft(&self, raw_base64url: &str) -> Result<String, GmailError> {
        GmailClient::create_draft(self, raw_base64url).await
    }

    async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
        GmailClient::list_labels(self).await
    }

    async fn create_label(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<GmailLabel, GmailError> {
        GmailClient::create_label(self, name, color).await
    }

    async fn rename_label(&self, label_id: &str, new_name: &str) -> Result<GmailLabel, GmailError> {
        GmailClient::rename_label(self, label_id, new_name).await
    }

    async fn delete_label(&self, label_id: &str) -> Result<(), GmailError> {
        GmailClient::delete_label(self, label_id).await
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
                    encoded.push_str(&format!("%{:02X}", byte));
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
    use std::any::Any;
    use std::panic::AssertUnwindSafe;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
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
            let status = resp.status().as_u16();
            match status {
                401 => GmailError::AuthExpired,
                404 => {
                    let body = resp.text().await.unwrap_or_default();
                    GmailError::NotFound(body)
                }
                429 => {
                    let retry_after = resp
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(60);
                    GmailError::RateLimited {
                        retry_after_secs: retry_after,
                    }
                }
                _ => {
                    let body = resp.text().await.unwrap_or_default();
                    GmailError::Api { status, body }
                }
            }
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
            GmailError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 30);
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
}
