use keyring::Entry;
use mxr_core::provider::MailSyncProvider;
use mxr_core::types::SyncCursor;
use mxr_provider_imap::{config::ImapConfig, ImapProvider};

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing env var {name}"))
}

#[tokio::test]
#[ignore = "live smoke"]
async fn live_smoke_imap_sync_labels_and_messages() {
    let username = required_env("MXR_IMAP_USERNAME");
    let password = required_env("MXR_IMAP_PASSWORD");
    let password_ref = "mxr/live-smoke-imap";
    let _ = Entry::new(password_ref, &username)
        .unwrap()
        .set_password(&password);

    let config = ImapConfig {
        host: required_env("MXR_IMAP_HOST"),
        port: required_env("MXR_IMAP_PORT").parse().unwrap(),
        username,
        password_ref: password_ref.into(),
        use_tls: std::env::var("MXR_IMAP_USE_TLS")
            .ok()
            .map(|value| value != "false")
            .unwrap_or(true),
    };
    let mut provider = ImapProvider::new(mxr_core::AccountId::new(), config);

    provider.authenticate().await.unwrap();
    let labels = provider.sync_labels().await.unwrap();
    assert!(!labels.is_empty(), "imap live smoke should list folders");

    let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
    assert!(
        !batch.upserted.is_empty(),
        "imap live smoke should fetch at least one message"
    );
}
