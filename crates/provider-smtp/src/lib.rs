#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

pub mod config;

use async_trait::async_trait;
use config::{SmtpConfig, SmtpError};
#[cfg(not(test))]
use lettre::AsyncTransport;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use mxr_core::error::MxrError;
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft, SendReceipt};
use mxr_outbound::attachments::{load_attachment_paths_async, LoadedAttachment};
#[cfg(test)]
use mxr_outbound::email::build_message;
use std::path::PathBuf;
use std::time::Instant;

pub struct SmtpSendProvider {
    config: SmtpConfig,
    #[cfg(test)]
    test_sender: Option<std::sync::Arc<dyn TestSender>>,
}

impl SmtpSendProvider {
    pub fn new(config: SmtpConfig) -> Self {
        Self {
            config,
            #[cfg(test)]
            test_sender: None,
        }
    }

    #[cfg(test)]
    fn with_test_sender(config: SmtpConfig, test_sender: std::sync::Arc<dyn TestSender>) -> Self {
        Self {
            config,
            test_sender: Some(test_sender),
        }
    }

    pub async fn test_connection(&self) -> Result<(), SmtpError> {
        #[cfg(test)]
        if let Some(sender) = &self.test_sender {
            return sender.test_connection().await.map_err(SmtpError::Transport);
        }

        let transport = self.build_transport().await?;
        transport
            .test_connection()
            .await
            .map_err(|e| SmtpError::Transport(e.to_string()))?;
        Ok(())
    }

    /// Resolve SMTP credentials, or `None` when the server needs no auth.
    fn resolve_credentials(&self) -> Result<Option<Credentials>, SmtpError> {
        if self.config.auth_required {
            let password = self.config.resolve_password()?;
            Ok(Some(Credentials::new(
                self.config.username.clone(),
                password,
            )))
        } else {
            Ok(None)
        }
    }

    async fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, SmtpError> {
        // Defense-in-depth: lettre does not gate AUTH on encryption, so a
        // plaintext transport would happily send AUTH PLAIN/LOGIN in the clear.
        // Refuse before we ever attach credentials. `validate()` also enforces
        // this at config-load time.
        if self.config.auth_required && !self.config.use_tls {
            return Err(SmtpError::Transport(
                "refusing to send SMTP credentials over an unencrypted connection (auth_required with use_tls=false)".into(),
            ));
        }

        let builder = if self.config.use_tls {
            if self.config.port == 465 {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
            } else {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
            }
            .port(self.config.port)
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                .port(self.config.port)
        };

        let transport = match self.resolve_credentials()? {
            Some(creds) => builder.credentials(creds).build(),
            None => builder.build(),
        };

        Ok(transport)
    }
}

#[async_trait]
impl MailSendProvider for SmtpSendProvider {
    fn name(&self) -> &str {
        "smtp"
    }

    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt, MxrError> {
        let attachments = load_attachments(&draft.attachments).await?;
        let started_at = Instant::now();
        let message = mxr_outbound::email::build_message_with_id(
            draft,
            from,
            false,
            &attachments,
            rfc2822_message_id,
        )
        .map_err(|e| MxrError::Provider(format!("Failed to build message: {e}")))?;
        tracing::trace!(
            attachment_count = attachments.len(),
            elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
            "smtp message build completed"
        );

        #[cfg(test)]
        if let Some(sender) = &self.test_sender {
            sender
                .send(message)
                .await
                .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;
        }

        #[cfg(not(test))]
        {
            let transport = self
                .build_transport()
                .await
                .map_err(|e| MxrError::Provider(e.to_string()))?;

            transport
                .send(message)
                .await
                .map_err(|e| classify_smtp_send_error(&e))?;
        }

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }

    async fn send_calendar_reply(
        &self,
        reply: &mxr_core::CalendarReplyMessage,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt, MxrError> {
        let message = mxr_outbound::email::build_calendar_reply_message_with_id(
            reply,
            from,
            rfc2822_message_id,
        )
        .map_err(|e| MxrError::Provider(format!("Failed to build calendar reply: {e}")))?;

        #[cfg(test)]
        if let Some(sender) = &self.test_sender {
            sender
                .send(message)
                .await
                .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;
        }

        #[cfg(not(test))]
        {
            let transport = self
                .build_transport()
                .await
                .map_err(|e| MxrError::Provider(e.to_string()))?;
            transport
                .send(message)
                .await
                .map_err(|e| classify_smtp_send_error(&e))?;
        }

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }
}

/// Transient (4xx) responses and timeouts are worth retrying; permanent (5xx)
/// responses and everything else are hard failures.
fn is_retryable(is_transient: bool, is_timeout: bool) -> bool {
    is_transient || is_timeout
}

/// Map a send failure onto the core error taxonomy. Split out from
/// `classify_smtp_send_error` so the decision and message can be unit-tested
/// without a `lettre` error (which has no public constructor).
fn map_send_error(retryable: bool, error: impl std::fmt::Display) -> MxrError {
    if retryable {
        // SMTP carries no Retry-After hint; 60s is a sane default backoff so the
        // daemon can reschedule the send instead of hard-failing it.
        MxrError::RateLimited {
            retry_after_secs: 60,
        }
    } else {
        MxrError::Provider(format!("SMTP send failed: {error}"))
    }
}

/// Classify a lettre SMTP send error so the daemon backs off on transient
/// failures (4xx / timeout) and hard-fails on permanent ones (5xx).
#[cfg(not(test))]
fn classify_smtp_send_error(e: &lettre::transport::smtp::Error) -> MxrError {
    map_send_error(is_retryable(e.is_transient(), e.is_timeout()), e)
}

async fn load_attachments(paths: &[PathBuf]) -> Result<Vec<LoadedAttachment>, MxrError> {
    let started_at = Instant::now();
    let attachments = load_attachment_paths_async(paths)
        .await
        .map_err(|error| MxrError::Provider(format!("Failed to load attachments: {error}")))?;

    tracing::trace!(
        attachment_count = attachments.len(),
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "smtp attachment load completed"
    );

    Ok(attachments)
}

#[cfg(test)]
#[async_trait]
trait TestSender: Send + Sync {
    async fn send(&self, message: lettre::Message) -> Result<(), String>;
    async fn test_connection(&self) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::DraftId;
    use mxr_core::types::DraftIntent;
    use std::sync::{Arc, Mutex};

    fn test_draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: mxr_core::id::AccountId::new(),
            intent: DraftIntent::New,
            reply_headers: None,
            to: vec![Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test Subject".into(),
            body_markdown: "Hello **world**!".into(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[derive(Debug, Clone)]
    struct RecordedMessage {
        formatted: String,
        envelope_from: Option<String>,
        envelope_to: Vec<String>,
    }

    #[derive(Default)]
    struct RecordedSender {
        messages: Mutex<Vec<RecordedMessage>>,
    }

    #[async_trait]
    impl TestSender for RecordedSender {
        async fn send(&self, message: lettre::Message) -> Result<(), String> {
            let envelope = message.envelope();
            self.messages.lock().unwrap().push(RecordedMessage {
                formatted: String::from_utf8(message.formatted()).unwrap(),
                envelope_from: envelope.from().map(ToString::to_string),
                envelope_to: envelope.to().iter().map(ToString::to_string).collect(),
            });
            Ok(())
        }

        async fn test_connection(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn resolve_credentials_present_only_when_auth_required() {
        let with_auth = SmtpConfig::new(
            "smtp.example.com".into(),
            587,
            "user".into(),
            "mxr/test".into(),
            true,
            true,
        )
        .with_password("pw".into());
        assert!(SmtpSendProvider::new(with_auth)
            .resolve_credentials()
            .unwrap()
            .is_some());

        let no_auth = SmtpConfig::new(
            "smtp.example.com".into(),
            587,
            "user".into(),
            "mxr/test".into(),
            false,
            true,
        );
        assert!(SmtpSendProvider::new(no_auth)
            .resolve_credentials()
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn build_transport_refuses_cleartext_auth() {
        let config = SmtpConfig::new(
            "smtp.example.com".into(),
            587,
            "user".into(),
            "mxr/test".into(),
            true,
            false,
        );
        let provider = SmtpSendProvider::new(config);
        let err = provider.build_transport().await.unwrap_err();
        assert!(matches!(err, SmtpError::Transport(msg) if msg.contains("unencrypted")));
    }

    #[test]
    fn is_retryable_covers_transient_timeout_and_permanent() {
        assert!(is_retryable(true, false));
        assert!(is_retryable(false, true));
        assert!(is_retryable(true, true));
        assert!(!is_retryable(false, false));
    }

    #[test]
    fn map_send_error_maps_transient_to_rate_limited_and_permanent_to_provider() {
        assert!(matches!(
            map_send_error(true, "transient boom"),
            MxrError::RateLimited {
                retry_after_secs: 60
            }
        ));
        assert!(matches!(
            map_send_error(false, "permanent boom"),
            MxrError::Provider(msg) if msg == "SMTP send failed: permanent boom"
        ));
    }

    #[test]
    fn build_message_basic() {
        let draft = test_draft();
        let from = Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        };
        let msg = build_message(&draft, &from, false).unwrap();
        let bytes = msg.formatted();
        let formatted = String::from_utf8_lossy(&bytes);
        assert!(formatted.contains("From: Me <me@example.com>\r\n"));
        assert!(formatted.contains("To: Alice <alice@example.com>\r\n"));
        assert!(formatted.contains("Subject: Test Subject\r\n"));
        assert!(formatted.contains("Content-Type: multipart/alternative"));
        assert!(formatted.contains("text/plain; charset=utf-8"));
        assert!(formatted.contains("text/html; charset=utf-8"));
        assert!(formatted.contains("Hello **world**!"));
        assert!(formatted.contains("<strong>world</strong>"));
    }

    #[test]
    fn build_message_invalid_email() {
        let mut draft = test_draft();
        draft.to = vec![Address {
            name: None,
            email: "not-valid".into(),
        }];
        let from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        assert!(build_message(&draft, &from, false).is_err());
    }

    #[tokio::test]
    async fn smtp_provider_passes_send_conformance() {
        let sender = Arc::new(RecordedSender::default());
        let provider = SmtpSendProvider::with_test_sender(
            SmtpConfig::new(
                "smtp.example.com".into(),
                587,
                "me@example.com".into(),
                "mxr/test".into(),
                true,
                true,
            ),
            sender.clone(),
        );
        mxr_provider_fake::conformance::run_send_conformance(&provider).await;
        let messages = sender.messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0]
            .formatted
            .contains("Subject: Conformance test draft"));
    }

    #[tokio::test]
    async fn smtp_send_preserves_envelope_recipients_and_strips_bcc_header() {
        let sender = Arc::new(RecordedSender::default());
        let provider = SmtpSendProvider::with_test_sender(
            SmtpConfig::new(
                "smtp.example.com".into(),
                587,
                "me@example.com".into(),
                "mxr/test".into(),
                true,
                true,
            ),
            sender.clone(),
        );

        let mut draft = test_draft();
        draft.cc = vec![Address {
            name: Some("Carol".into()),
            email: "carol@example.com".into(),
        }];
        draft.bcc = vec![Address {
            name: Some("Hidden".into()),
            email: "hidden@example.com".into(),
        }];

        let from = Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        };

        provider
            .send(&draft, &from, "<test-message@example.com>")
            .await
            .unwrap();

        let messages = sender.messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        let message = &messages[0];

        assert_eq!(message.envelope_from.as_deref(), Some("me@example.com"));
        assert_eq!(
            message.envelope_to,
            vec![
                "alice@example.com".to_string(),
                "carol@example.com".to_string(),
                "hidden@example.com".to_string(),
            ]
        );
        assert!(message
            .formatted
            .contains("To: Alice <alice@example.com>\r\n"));
        assert!(message
            .formatted
            .contains("Cc: Carol <carol@example.com>\r\n"));
        assert!(!message.formatted.contains("\r\nBcc:"));
        assert!(message
            .formatted
            .contains("Content-Type: multipart/alternative"));
        assert!(message.formatted.contains("Hello **world**!"));
        assert!(message.formatted.contains("<strong>world</strong>"));
    }

    #[tokio::test]
    async fn smtp_send_loads_attachments_on_async_path() {
        let sender = Arc::new(RecordedSender::default());
        let provider = SmtpSendProvider::with_test_sender(
            SmtpConfig::new(
                "smtp.example.com".into(),
                587,
                "me@example.com".into(),
                "mxr/test".into(),
                true,
                true,
            ),
            sender.clone(),
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello attachment").unwrap();

        let mut draft = test_draft();
        draft.attachments = vec![path];

        let from = Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        };

        provider
            .send(&draft, &from, "<test-attachment@example.com>")
            .await
            .unwrap();

        let messages = sender.messages.lock().unwrap();
        let message = &messages[0].formatted;
        assert!(message.contains("multipart/mixed"));
        assert!(message.contains("note.txt"));
    }
}
