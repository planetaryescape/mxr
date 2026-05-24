use async_trait::async_trait;
use futures::future::BoxFuture;
#[cfg(not(test))]
use lettre::AsyncTransport;
use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, Tokio1Executor,
};
use mxr_core::error::MxrError;
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft, SendReceipt};
use mxr_outbound::attachments::load_attachment_paths_sync;
use mxr_outbound::email::build_message_with_id;
use std::sync::Arc;

type TokenFn = Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<String>> + Send + Sync>;

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

        // Port 465 = implicit TLS, port 587 = STARTTLS.
        let builder = if self.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host).map_err(|e| e.to_string())?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host)
                .map_err(|e| e.to_string())?
        };

        Ok(builder
            .port(self.port)
            .authentication(vec![Mechanism::Xoauth2])
            .credentials(creds)
            .build())
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

    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt, MxrError> {
        let attachments = load_attachment_paths_sync(&draft.attachments)
            .map_err(|e| MxrError::Provider(format!("failed to load attachments: {e}")))?;
        let _message = build_message_with_id(draft, from, false, &attachments, rfc2822_message_id)
            .map_err(|e| MxrError::Provider(format!("failed to build message: {e}")))?;

        #[cfg(not(test))]
        {
            let transport = self.build_transport().await.map_err(MxrError::Provider)?;
            transport
                .send(_message)
                .await
                .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;
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
        let _message = mxr_outbound::email::build_calendar_reply_message_with_id(
            reply,
            from,
            rfc2822_message_id,
        )
        .map_err(|e| MxrError::Provider(format!("failed to build calendar reply: {e}")))?;

        #[cfg(not(test))]
        {
            let transport = self.build_transport().await.map_err(MxrError::Provider)?;
            transport
                .send(_message)
                .await
                .map_err(|e| MxrError::Provider(format!("SMTP send failed: {e}")))?;
        }

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }
}
