use crate::fixtures;
use chrono::Utc;
use mxr_core::{AccountId, MailSendProvider, MailSyncProvider, SyncCursor};

/// Run reusable sync-provider conformance checks.
///
/// These assertions validate baseline mxr contract behavior through the public
/// provider traits. Adapter authors can call this from their own async tests.
pub async fn run_sync_conformance<P>(provider: &P)
where
    P: MailSyncProvider + ?Sized,
{
    let labels = provider
        .sync_labels()
        .await
        .expect("sync_labels should succeed");
    assert!(
        labels.iter().all(|label| !label.name.trim().is_empty()),
        "labels must have visible names"
    );

    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("initial sync should succeed");
    assert!(
        !batch.upserted.is_empty(),
        "initial sync should return at least one message"
    );
    assert!(
        !matches!(batch.next_cursor, SyncCursor::Initial),
        "initial sync should advance the cursor"
    );

    for synced in &batch.upserted {
        assert!(
            !synced.envelope.provider_id.is_empty(),
            "provider ids must be populated"
        );
        assert!(
            !synced.envelope.subject.trim().is_empty(),
            "subjects must be populated"
        );
        assert_eq!(
            synced.envelope.id, synced.body.message_id,
            "body message ids must match envelope ids"
        );
        assert!(
            synced.envelope.date <= Utc::now() + chrono::Duration::minutes(1),
            "message dates must be parseable"
        );
        for attachment in &synced.body.attachments {
            assert!(
                !attachment.provider_id.is_empty(),
                "attachment provider ids must be populated"
            );
            assert_eq!(
                attachment.message_id, synced.envelope.id,
                "attachment message ids must match envelope ids"
            );
        }
    }

    if let Some(with_attachment) = batch
        .upserted
        .iter()
        .find(|synced| !synced.body.attachments.is_empty())
    {
        let attachment = &with_attachment.body.attachments[0];
        let bytes = provider
            .fetch_attachment(
                &with_attachment.envelope.provider_id,
                &attachment.provider_id,
            )
            .await
            .expect("attachment fetch should succeed");
        assert!(!bytes.is_empty(), "attachment bytes should not be empty");
    }

    let first = &batch.upserted[0].envelope.provider_id;
    provider
        .set_read(first, true)
        .await
        .expect("set_read should succeed");
    provider
        .set_starred(first, true)
        .await
        .expect("set_starred should succeed");
    provider
        .modify_labels(first, &["INBOX".to_string()], &[])
        .await
        .expect("modify_labels should succeed");
    provider.trash(first).await.expect("trash should succeed");

    if provider.capabilities().labels {
        let created = provider
            .create_label("Conformance Label", Some("#3366ff"))
            .await
            .expect("create_label should succeed when labels are supported");
        let renamed = provider
            .rename_label(&created.provider_id, "Conformance Label Renamed")
            .await
            .expect("rename_label should succeed when labels are supported");
        assert_eq!(renamed.name, "Conformance Label Renamed");
        provider
            .delete_label(&renamed.provider_id)
            .await
            .expect("delete_label should succeed when labels are supported");
    }

    let delta = provider
        .sync_messages(&batch.next_cursor)
        .await
        .expect("delta sync should succeed");
    assert!(
        !matches!(delta.next_cursor, SyncCursor::Initial),
        "delta sync should preserve a non-initial cursor"
    );
    assert!(
        delta
            .upserted
            .iter()
            .all(|synced| !synced.envelope.provider_id.is_empty()),
        "delta sync results must preserve provider ids"
    );
}

/// Run reusable send-provider conformance checks.
pub async fn run_send_conformance<P>(provider: &P)
where
    P: MailSendProvider + ?Sized,
{
    let draft = fixtures::sample_draft(AccountId::new());
    let from = fixtures::sample_from_address();
    let now = Utc::now();

    let receipt = provider
        .send(&draft, &from)
        .await
        .expect("send should succeed");
    assert!(
        receipt.sent_at >= now - chrono::Duration::minutes(1),
        "receipt timestamp should be recent"
    );

    let _saved = provider
        .save_draft(&draft, &from)
        .await
        .expect("save_draft should not fail");
    if provider.name() != "smtp" {
        let saved = provider
            .save_draft(&draft, &from)
            .await
            .expect("save_draft should stay stable across calls");
        assert!(
            saved.is_some(),
            "non-SMTP providers should return a provider draft id"
        );
    }
}
