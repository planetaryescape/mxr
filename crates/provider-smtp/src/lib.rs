pub mod config;

use async_trait::async_trait;
use config::{SmtpConfig, SmtpError};
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use mxr_compose::render::render_markdown;
use mxr_core::error::MxrError;
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft, SendReceipt};

pub struct SmtpSendProvider {
    config: SmtpConfig,
}

impl SmtpSendProvider {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    async fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, SmtpError> {
        let password = self.config.resolve_password()?;
        let creds = Credentials::new(self.config.username.clone(), password);

        let transport = if self.config.use_tls {
            if self.config.port == 465 {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
                    .port(self.config.port)
                    .credentials(creds)
                    .build()
            } else {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
                    .port(self.config.port)
                    .credentials(creds)
                    .build()
            }
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                .port(self.config.port)
                .credentials(creds)
                .build()
        };

        Ok(transport)
    }
}

/// Build a lettre Message from a Draft.
fn build_message(draft: &Draft, from: &Address) -> Result<Message, MxrError> {
    let from_mailbox: Mailbox =
        from.email
            .parse()
            .map_err(|e: lettre::address::AddressError| {
                MxrError::Provider(format!("Invalid from address: {e}"))
            })?;

    let mut builder = Message::builder().from(from_mailbox);

    for addr in &draft.to {
        let mailbox: Mailbox = addr
            .email
            .parse()
            .map_err(|e: lettre::address::AddressError| {
                MxrError::Provider(format!("Invalid to address: {e}"))
            })?;
        builder = builder.to(mailbox);
    }

    for addr in &draft.cc {
        let mailbox: Mailbox = addr
            .email
            .parse()
            .map_err(|e: lettre::address::AddressError| {
                MxrError::Provider(format!("Invalid cc address: {e}"))
            })?;
        builder = builder.cc(mailbox);
    }

    for addr in &draft.bcc {
        let mailbox: Mailbox = addr
            .email
            .parse()
            .map_err(|e: lettre::address::AddressError| {
                MxrError::Provider(format!("Invalid bcc address: {e}"))
            })?;
        builder = builder.bcc(mailbox);
    }

    builder = builder.subject(&draft.subject);

    let rendered = render_markdown(&draft.body_markdown);
    let multipart = MultiPart::alternative()
        .singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(rendered.plain),
        )
        .singlepart(
            SinglePart::builder()
                .header(ContentType::TEXT_HTML)
                .body(rendered.html),
        );

    builder
        .multipart(multipart)
        .map_err(|e| MxrError::Provider(format!("Failed to build message: {e}")))
}

#[async_trait]
impl MailSendProvider for SmtpSendProvider {
    fn name(&self) -> &str {
        "smtp"
    }

    async fn send(&self, draft: &Draft, from: &Address) -> Result<SendReceipt, MxrError> {
        let transport = self
            .build_transport()
            .await
            .map_err(|e| MxrError::Provider(e.to_string()))?;

        let message = build_message(draft, from)?;

        transport
            .send(message)
            .await
            .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::DraftId;

    fn test_draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: mxr_core::id::AccountId::new(),
            in_reply_to: None,
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

    #[test]
    fn build_message_basic() {
        let draft = test_draft();
        let from = Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        };
        let msg = build_message(&draft, &from).unwrap();
        // Verify the message was built successfully by formatting it
        let bytes = msg.formatted();
        let formatted = String::from_utf8_lossy(&bytes);
        assert!(formatted.contains("Test Subject"));
        assert!(formatted.contains("alice@example.com"));
        assert!(formatted.contains("text/plain"));
        assert!(formatted.contains("text/html"));
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
        assert!(build_message(&draft, &from).is_err());
    }
}
