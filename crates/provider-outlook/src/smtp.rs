use mxr_compose::email::build_message;
use mxr_core::error::MxrError;
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft, SendReceipt};
use async_trait::async_trait;
use futures::future::BoxFuture;
use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, Tokio1Executor,
};
#[cfg(not(test))]
use lettre::AsyncTransport;
use std::sync::Arc;

type TokenFn =
    Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<String>> + Send + Sync>;

/// SMTP send provider using XOAUTH2 for Microsoft Outlook/Exchange.
pub struct OutlookSmtpSendProvider {
    host: String,
    port: u16,
    username: String,
    token_fn: TokenFn,
}

impl OutlookSmtpSendProvider {
    pub fn new(host: String, port: u16, username: String, token_fn: TokenFn) -> Self {
        Self {
            host,
            port,
            username,
            token_fn,
        }
    }

    async fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
        let token = (self.token_fn)()
            .await
            .map_err(|e| format!("failed to get access token: {e}"))?;

        let creds = Credentials::new(self.username.clone(), token);

        // Port 465 = implicit TLS, port 587 = STARTTLS
        let transport = if self.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host)
                .map_err(|e| e.to_string())?
                .port(self.port)
                .authentication(vec![Mechanism::Xoauth2])
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)
                .map_err(|e| e.to_string())?
                .port(self.port)
                .authentication(vec![Mechanism::Xoauth2])
                .credentials(creds)
                .build()
        };

        Ok(transport)
    }

    pub async fn test_connection(&self) -> Result<(), String> {
        let transport = self.build_transport().await?;
        transport
            .test_connection()
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[async_trait]
impl MailSendProvider for OutlookSmtpSendProvider {
    fn name(&self) -> &str {
        "outlook-smtp"
    }

    async fn send(&self, draft: &Draft, from: &Address) -> Result<SendReceipt, MxrError> {
        let _message = build_message(draft, from, false)
            .map_err(|e| MxrError::Provider(format!("failed to build message: {e}")))?;

        #[cfg(not(test))]
        {
            let transport = self
                .build_transport()
                .await
                .map_err(|e| MxrError::Provider(e))?;
            transport
                .send(_message)
                .await
                .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;
        }

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: String::new(),
        })
    }
}
