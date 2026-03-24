use keyring::Entry;
use mxr_core::provider::MailSendProvider;
use mxr_core::types::{Address, Draft};
use mxr_provider_smtp::{config::SmtpConfig, SmtpSendProvider};

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing env var {name}"))
}

#[tokio::test]
#[ignore = "live smoke"]
async fn live_smoke_smtp_test_connection_and_send() {
    let username = required_env("MXR_SMTP_USERNAME");
    let password = required_env("MXR_SMTP_PASSWORD");
    let password_ref = "mxr/live-smoke-smtp";
    let _ = Entry::new(password_ref, &username)
        .unwrap()
        .set_password(&password);

    let config = SmtpConfig {
        host: required_env("MXR_SMTP_HOST"),
        port: required_env("MXR_SMTP_PORT").parse().unwrap(),
        username: username.clone(),
        password_ref: password_ref.into(),
        auth_required: true,
        use_tls: std::env::var("MXR_SMTP_USE_TLS")
            .ok()
            .map(|value| value != "false")
            .unwrap_or(true),
    };
    let provider = SmtpSendProvider::new(config);
    provider.test_connection().await.unwrap();

    let to = std::env::var("MXR_SMTP_TO").unwrap_or_else(|_| username.clone());
    let draft = Draft {
        id: mxr_core::DraftId::new(),
        account_id: mxr_core::AccountId::new(),
        reply_headers: None,
        to: vec![Address {
            name: None,
            email: to,
        }],
        cc: vec![],
        bcc: vec![],
        subject: format!("mxr live smoke {}", chrono::Utc::now().timestamp()),
        body_markdown: "SMTP live smoke message".into(),
        attachments: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let from = Address {
        name: None,
        email: username,
    };

    provider.send(&draft, &from).await.unwrap();
}
