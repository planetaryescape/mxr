use mxr_core::id::{AccountId, DraftId};
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft};
use mxr_provider_smtp::{config::SmtpConfig, SmtpSendProvider};

fn invalid_draft() -> Draft {
    Draft {
        id: DraftId::new(),
        account_id: AccountId::new(),
        reply_headers: None,
        to: vec![Address {
            name: None,
            email: "invalid-email-without-at".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "fixture".into(),
        body_markdown: "body".into(),
        attachments: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn provider() -> SmtpSendProvider {
    SmtpSendProvider::new(SmtpConfig::new(
        "smtp.example.com".into(),
        587,
        "me@example.com".into(),
        "mxr/test-smtp".into(),
        true,
        true,
    ))
}

#[tokio::test]
async fn provider_offline_smoke_smtp_send_rejects_invalid_recipient() {
    let provider = provider();
    let from = Address {
        name: None,
        email: "sender@example.com".into(),
    };

    let err = provider
        .send(&invalid_draft(), &from)
        .await
        .expect_err("invalid recipient must fail");
    assert!(err.to_string().contains("Failed to build message"));
}
