//! End-to-end Gmail integration smoke that exercises the regression class
//! that shipped the 0.4.52 label-id bug:
//!
//!   real OAuth → real Gmail API → sync_engine → store → message_labels
//!   junction → query by label
//!
//! Env-gated so it does not run on normal `cargo test`. To run locally or
//! in CI, set:
//!
//!   MXR_GMAIL_TEST_CLIENT_ID
//!   MXR_GMAIL_TEST_CLIENT_SECRET
//!   MXR_GMAIL_TEST_REFRESH_TOKEN
//!
//! Use a dedicated throwaway Google account — the test only reads, but it
//! does authenticate as the live user.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use mxr_core::id::AccountId;
use mxr_provider_gmail::auth::GmailAuth;
use mxr_provider_gmail::client::GmailClient;
use mxr_provider_gmail::GmailProvider;
use mxr_search::{SearchIndex, SearchServiceHandle};
use mxr_store::Store;
use mxr_sync::SyncEngine;

const ENV_CLIENT_ID: &str = "MXR_GMAIL_TEST_CLIENT_ID";
const ENV_CLIENT_SECRET: &str = "MXR_GMAIL_TEST_CLIENT_SECRET";
const ENV_REFRESH_TOKEN: &str = "MXR_GMAIL_TEST_REFRESH_TOKEN";

fn live_creds() -> Option<(String, String, String)> {
    let id = std::env::var(ENV_CLIENT_ID).ok()?;
    let secret = std::env::var(ENV_CLIENT_SECRET).ok()?;
    let refresh = std::env::var(ENV_REFRESH_TOKEN).ok()?;
    if id.is_empty() || secret.is_empty() || refresh.is_empty() {
        return None;
    }
    Some((id, secret, refresh))
}

#[tokio::test]
#[ignore = "requires live Gmail credentials in env (see file header)"]
async fn live_gmail_sync_populates_store_and_label_junction() {
    let Some((client_id, client_secret, refresh_token)) = live_creds() else {
        panic!(
            "missing live credentials. Set {ENV_CLIENT_ID}, {ENV_CLIENT_SECRET}, {ENV_REFRESH_TOKEN}.",
        );
    };

    // 1. Build a Gmail provider authenticated via the long-lived refresh token.
    let auth = GmailAuth::with_refresh_token(client_id, client_secret, refresh_token);
    let account_id = AccountId::new();
    let client = GmailClient::new(auth);
    let provider = GmailProvider::new(account_id.clone(), client);

    // 2. Stand up an in-memory store + search.
    let store = Arc::new(Store::in_memory().await.unwrap());
    let (search, _ingest) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());
    let engine = SyncEngine::new(store.clone(), search);

    // 3. Insert the account so FK-constrained writes succeed.
    let account = mxr_core::Account {
        id: account_id.clone(),
        name: "live-gmail-test".to_string(),
        email: "live-gmail-test@example.invalid".to_string(),
        sync_backend: Some(mxr_core::BackendRef {
            provider_kind: mxr_core::ProviderKind::Gmail,
            config_key: "live-gmail-test".to_string(),
        }),
        send_backend: Some(mxr_core::BackendRef {
            provider_kind: mxr_core::ProviderKind::Gmail,
            config_key: "live-gmail-test".to_string(),
        }),
        enabled: true,
    };
    store.insert_account(&account).await.unwrap();

    // 4. Run one sync cycle. Limited blast radius: this is a single delta
    //    iteration against the live API, not a continuous loop.
    let outcome = engine
        .sync_account_with_outcome(&provider)
        .await
        .expect("real Gmail sync must succeed end-to-end");

    // 5. Labels — the regression class. Provider must hand at least the
    //    standard system labels and they must land in the store under their
    //    own ids without UNIQUE-constraint failure.
    let labels = store
        .list_labels_by_account(&account_id)
        .await
        .expect("labels must persist");
    assert!(
        labels.iter().any(|l| l.provider_id == "INBOX"),
        "Gmail must report at least the INBOX system label; got {:?}",
        labels.iter().map(|l| &l.provider_id).collect::<Vec<_>>()
    );

    // 6. Re-run the sync. The 0.4.52 bug surfaces on the SECOND label
    //    upsert (the first run inserted the rows, the second hits UNIQUE).
    //    This iteration would fail with `UNIQUE constraint failed:
    //    labels.account_id, labels.provider_id` on the buggy code path.
    engine
        .sync_account_with_outcome(&provider)
        .await
        .expect("second sync must not regress on label upsert");

    // 7. Junction sanity — at least one INBOX message should exist with a
    //    junction row pointing at the INBOX label. This rejects the silent
    //    junction-loss class of bug (CASCADE on re-upsert) the engine has
    //    backfill logic for.
    if outcome.synced_count > 0 {
        let inbox = labels
            .iter()
            .find(|l| l.provider_id == "INBOX")
            .expect("checked above");
        let envelopes = store
            .list_envelopes_by_label(&inbox.id, 1, 0)
            .await
            .expect("query by INBOX label must succeed");
        assert!(
            !envelopes.is_empty(),
            "INBOX has at least one synced message but the junction is empty"
        );
    }
}
