use mxr_core::provider::MailSyncProvider;
use mxr_core::types::SyncCursor;
use mxr_provider_gmail::{auth::GmailAuth, client::GmailClient, GmailProvider};

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing env var {name}"))
}

#[tokio::test]
#[ignore = "live smoke"]
async fn live_smoke_gmail_sync_labels_and_messages() {
    let auth = GmailAuth::with_refresh_token(
        required_env("MXR_GMAIL_CLIENT_ID"),
        required_env("MXR_GMAIL_CLIENT_SECRET"),
        required_env("MXR_GMAIL_REFRESH_TOKEN"),
    );
    let client = GmailClient::new(auth);
    let provider = GmailProvider::new(mxr_core::AccountId::new(), client);

    let labels = provider.sync_labels().await.unwrap();
    assert!(!labels.is_empty(), "gmail live smoke should list labels");

    let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
    assert!(
        !batch.upserted.is_empty() || matches!(batch.next_cursor, SyncCursor::GmailBackfill { .. }),
        "gmail live smoke should return messages or a backfill cursor"
    );
}
