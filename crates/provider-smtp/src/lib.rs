pub mod config;

use async_trait::async_trait;
use config::{SmtpConfig, SmtpError};
#[cfg(not(test))]
use lettre::AsyncTransport;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use mxr_core::error::MxrError;
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft, SendReceipt};
use mxr_outbound::email::build_message;

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

    async fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, SmtpError> {
        let transport = if self.config.use_tls {
            if self.config.port == 465 {
                let builder = AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
                    .port(self.config.port);
                if self.config.auth_required {
                    let password = self.config.resolve_password()?;
                    let creds = Credentials::new(self.config.username.clone(), password);
                    builder.credentials(creds).build()
                } else {
                    builder.build()
                }
            } else {
                let builder =
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.host)
                        .map_err(|e| SmtpError::Transport(e.to_string()))?
                        .port(self.config.port);
                if self.config.auth_required {
                    let password = self.config.resolve_password()?;
                    let creds = Credentials::new(self.config.username.clone(), password);
                    builder.credentials(creds).build()
                } else {
                    builder.build()
                }
            }
        } else {
            let builder =
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                    .port(self.config.port);
            if self.config.auth_required {
                let password = self.config.resolve_password()?;
                let creds = Credentials::new(self.config.username.clone(), password);
                builder.credentials(creds).build()
            } else {
                builder.build()
            }
        };

        Ok(transport)
    }
}

#[async_trait]
impl MailSendProvider for SmtpSendProvider {
    fn name(&self) -> &str {
        "smtp"
    }

    async fn send(&self, draft: &Draft, from: &Address) -> Result<SendReceipt, MxrError> {
        let message = build_message(draft, from, false)
            .map_err(|e| MxrError::Provider(format!("Failed to build message: {e}")))?;

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
                .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;
        }

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
        })
    }
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
    use std::sync::{Arc, Mutex};

    fn test_draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: mxr_core::id::AccountId::new(),
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
            SmtpConfig {
                host: "smtp.example.com".into(),
                port: 587,
                username: "me@example.com".into(),
                password_ref: "mxr/test".into(),
                auth_required: true,
                use_tls: true,
            },
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
            SmtpConfig {
                host: "smtp.example.com".into(),
                port: 587,
                username: "me@example.com".into(),
                password_ref: "mxr/test".into(),
                auth_required: true,
                use_tls: true,
            },
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

        provider.send(&draft, &from).await.unwrap();

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
}
