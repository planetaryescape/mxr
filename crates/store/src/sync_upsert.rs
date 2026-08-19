//! Batched persistence for a page of synced messages.
//!
//! The per-message path (`upsert_envelope_with_direction` → `insert_body`
//! → `set_message_labels` → `set_message_keywords`) opens and commits
//! around eight transactions per message. Every one of those goes through
//! the single writer connection, so an initial sync of tens of thousands
//! of messages spends most of its time committing. This module runs the
//! same statements, in the same per-message order, sharing one
//! transaction across a chunk of messages.

use crate::body::insert_body_tx;
use crate::keywords::replace_message_keywords_tx;
use crate::message::{replace_message_labels_tx, upsert_envelope_tx};
use mxr_core::id::LabelId;
use mxr_core::types::{Envelope, MessageBody, MessageDirection};

/// One synced message, with everything the store needs to persist it.
///
/// Deliberately not `Clone`: sync builds one of these per message in a
/// page, and copying a page's worth of bodies is what the batched path
/// exists to avoid.
#[derive(Debug)]
pub struct SyncUpsert {
    pub envelope: Envelope,
    pub direction: MessageDirection,
    pub body: MessageBody,
    pub label_ids: Vec<LabelId>,
}

/// Messages per transaction. Bounds how long one commit holds the writer
/// lock (other writers — mutations, contacts refresh — queue behind it)
/// while still amortising the commit cost over hundreds of messages.
const UPSERT_CHUNK: usize = 500;

impl super::Store {
    /// Persist a page of synced messages.
    ///
    /// Equivalent to calling the per-message store functions in order, but
    /// batched. A chunk is all-or-nothing: a failure leaves the sync cursor
    /// unadvanced and the page is re-synced.
    pub async fn apply_sync_upserts(&self, upserts: &[SyncUpsert]) -> Result<(), sqlx::Error> {
        for chunk in upserts.chunks(UPSERT_CHUNK) {
            let mut tx = self.writer().begin().await?;
            for upsert in chunk {
                upsert_envelope_tx(&mut tx, &upsert.envelope, upsert.direction).await?;
                insert_body_tx(&mut tx, &upsert.body).await?;
                replace_message_labels_tx(&mut tx, &upsert.envelope.id, &upsert.label_ids).await?;
                replace_message_keywords_tx(
                    &mut tx,
                    &upsert.envelope.id,
                    &upsert.envelope.keywords,
                )
                .await?;
            }
            tx.commit().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use mxr_core::id::{AttachmentId, MessageId};
    use mxr_core::types::{
        Account, AttachmentDisposition, AttachmentMeta, CalendarMetadata, EventSource, Label,
        LabelKind, MessageFlags, MessageMetadata,
    };
    use std::collections::BTreeSet;

    fn label(account: &Account, name: &str) -> Label {
        Label {
            id: mxr_core::id::LabelId::from_scoped_provider_id(&account.id, "fake", name),
            account_id: account.id.clone(),
            name: name.to_string(),
            kind: LabelKind::User,
            color: None,
            provider_id: name.to_string(),
            unread_count: 0,
            total_count: 0,
            role: None,
        }
    }

    /// A message that exercises every write the batched path performs:
    /// attachments, a List-Id promoted onto `messages`, a calendar invite,
    /// labels, and keywords.
    fn upsert(
        account: &Account,
        index: usize,
        label_ids: Vec<mxr_core::id::LabelId>,
    ) -> SyncUpsert {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .flags(MessageFlags::READ)
            .build();
        envelope.id =
            MessageId::from_scoped_provider_id(&account.id, "fake", &format!("m-{index}"));
        envelope.provider_id = format!("m-{index}");
        envelope.subject = format!("Subject {index}");
        envelope.label_provider_ids = vec!["work".to_string()];
        envelope.keywords = BTreeSet::from([format!("$Keyword{index}"), "$Forwarded".to_string()]);

        let metadata = MessageMetadata {
            list_id: Some(format!("list-{index}.example")),
            calendar: Some(CalendarMetadata {
                uid: Some(format!("invite-{index}")),
                method: Some("REQUEST".to_string()),
                summary: Some("Standup".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let body = MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(format!("Body {index}")),
            text_html: Some(format!("<p>Body {index}</p>")),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::from_provider_id("fake", &format!("att-{index}")),
                message_id: envelope.id.clone(),
                filename: format!("file-{index}.pdf"),
                mime_type: "application/pdf".to_string(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 42,
                local_path: None,
                provider_id: format!("att-{index}"),
            }],
            fetched_at: chrono::Utc::now(),
            metadata,
        };

        SyncUpsert {
            envelope,
            direction: MessageDirection::Inbound,
            body,
            label_ids,
        }
    }

    /// The batched path must leave exactly the rows the per-message store
    /// calls leave — that equivalence is what lets sync use it for every
    /// provider.
    #[tokio::test]
    async fn batched_upserts_match_the_per_message_path() {
        let account = test_account();
        let batched = Store::in_memory().await.unwrap();
        let per_message = Store::in_memory().await.unwrap();
        let work = label(&account, "work");

        for store in [&batched, &per_message] {
            store.insert_account(&account).await.unwrap();
            store.upsert_label(&work).await.unwrap();
        }

        let upserts: Vec<SyncUpsert> = (0..3)
            .map(|index| upsert(&account, index, vec![work.id.clone()]))
            .collect();

        batched.apply_sync_upserts(&upserts).await.unwrap();
        for item in &upserts {
            per_message
                .upsert_envelope_with_direction(&item.envelope, item.direction)
                .await
                .unwrap();
            per_message.insert_body(&item.body).await.unwrap();
            per_message
                .set_message_labels(&item.envelope.id, &item.label_ids, EventSource::Sync)
                .await
                .unwrap();
            per_message
                .set_message_keywords(&item.envelope.id, &item.envelope.keywords)
                .await
                .unwrap();
        }

        for item in &upserts {
            let id = &item.envelope.id;
            assert_eq!(
                format!("{:?}", batched.get_envelope(id).await.unwrap()),
                format!("{:?}", per_message.get_envelope(id).await.unwrap()),
                "envelope rows differ"
            );
            assert_eq!(
                format!("{:?}", batched.get_body(id).await.unwrap()),
                format!("{:?}", per_message.get_body(id).await.unwrap()),
                "body/attachment rows differ"
            );
            assert_eq!(
                batched.get_message_keywords(id).await.unwrap(),
                per_message.get_message_keywords(id).await.unwrap(),
                "keyword rows differ"
            );
            assert_eq!(
                batched
                    .get_calendar_invite_for_message(id)
                    .await
                    .unwrap()
                    .map(|invite| invite.metadata),
                per_message
                    .get_calendar_invite_for_message(id)
                    .await
                    .unwrap()
                    .map(|invite| invite.metadata),
                "calendar invite rows differ"
            );
        }
        assert_eq!(
            batched.count_message_labels().await.unwrap(),
            per_message.count_message_labels().await.unwrap(),
        );
    }

    /// The calendar invite's account lookup runs on the transaction's own
    /// connection because the reader pool cannot see an uncommitted message
    /// row. `Store::in_memory` shares one connection between reader and
    /// writer, so only a file-backed store proves it.
    #[tokio::test]
    async fn calendar_invites_land_on_a_file_backed_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(&dir.path().join("store.db")).await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let upserts = vec![upsert(&account, 0, Vec::new())];
        store.apply_sync_upserts(&upserts).await.unwrap();

        let invite = store
            .get_calendar_invite_for_message(&upserts[0].envelope.id)
            .await
            .unwrap()
            .expect("calendar invite must be written inside the batch transaction");
        assert_eq!(invite.metadata.uid.as_deref(), Some("invite-0"));
    }

    /// A re-sync that changes flags and shrinks the label and keyword sets
    /// must leave the same rows as the per-message path: both are
    /// delete-then-insert, so the removed entries have to disappear.
    #[tokio::test]
    async fn batched_upserts_match_the_per_message_path_on_a_shrinking_re_sync() {
        let account = test_account();
        let batched = Store::in_memory().await.unwrap();
        let per_message = Store::in_memory().await.unwrap();
        let work = label(&account, "work");
        let travel = label(&account, "travel");

        for store in [&batched, &per_message] {
            store.insert_account(&account).await.unwrap();
            store.upsert_label(&work).await.unwrap();
            store.upsert_label(&travel).await.unwrap();
        }

        let mut first = upsert(&account, 0, vec![work.id.clone(), travel.id.clone()]);
        first.envelope.keywords = BTreeSet::from(["$Forwarded".to_string(), "$Label1".to_string()]);

        // Second pass: read flag set, one label dropped, keywords emptied.
        let mut second = upsert(&account, 0, vec![work.id.clone()]);
        second.envelope.flags = MessageFlags::READ | MessageFlags::STARRED;
        second.envelope.subject = "Subject 0 edited".to_string();
        second.envelope.keywords = BTreeSet::new();
        second.body.attachments.clear();

        for pass in [&first, &second] {
            batched
                .apply_sync_upserts(std::slice::from_ref(pass))
                .await
                .unwrap();
            per_message
                .upsert_envelope_with_direction(&pass.envelope, pass.direction)
                .await
                .unwrap();
            per_message.insert_body(&pass.body).await.unwrap();
            per_message
                .set_message_labels(&pass.envelope.id, &pass.label_ids, EventSource::Sync)
                .await
                .unwrap();
            per_message
                .set_message_keywords(&pass.envelope.id, &pass.envelope.keywords)
                .await
                .unwrap();
        }

        let id = &second.envelope.id;
        assert_eq!(
            format!("{:?}", batched.get_envelope(id).await.unwrap()),
            format!("{:?}", per_message.get_envelope(id).await.unwrap()),
            "envelope rows differ after a re-sync"
        );
        assert_eq!(
            format!("{:?}", batched.get_body(id).await.unwrap()),
            format!("{:?}", per_message.get_body(id).await.unwrap()),
            "body/attachment rows differ after a re-sync"
        );
        assert!(
            batched.get_message_keywords(id).await.unwrap().is_empty(),
            "an empty keyword set must clear the previous keywords"
        );
        assert_eq!(
            batched.count_message_labels().await.unwrap(),
            per_message.count_message_labels().await.unwrap(),
        );
        assert_eq!(batched.count_message_labels().await.unwrap(), 1);
        let envelope = batched.get_envelope(id).await.unwrap().expect("envelope");
        assert!(envelope.flags.contains(MessageFlags::STARRED));
        assert_eq!(envelope.subject, "Subject 0 edited");
    }

    /// Documents a pre-existing hazard rather than a behaviour we want: a
    /// row written before 0.4.52 lives under an unscoped id, so a re-sync
    /// computes a different `MessageId` for the same
    /// `(account_id, provider_id)`. `upsert_envelope_tx` keeps the stored
    /// id (the natural key is UNIQUE), but the body is written against the
    /// incoming id, which no `messages` row has — so the page fails on the
    /// foreign key instead of silently splitting the message in two. The
    /// per-message path fails the same way; the fix (resolve dependents
    /// against the stored id) is deliberately out of scope here.
    #[tokio::test]
    async fn a_legacy_row_under_a_different_id_fails_the_page_rather_than_splitting_it() {
        let account = test_account();
        let store = Store::in_memory().await.unwrap();
        store.insert_account(&account).await.unwrap();

        let mut legacy = upsert(&account, 0, Vec::new());
        legacy.envelope.id = MessageId::from_provider_id("fake", &legacy.envelope.provider_id);
        legacy.body.message_id = legacy.envelope.id.clone();
        legacy.body.attachments.clear();
        store.apply_sync_upserts(&[legacy]).await.unwrap();

        // Same provider id, id derived the way the current code derives it.
        let current = upsert(&account, 0, Vec::new());
        let error = store
            .apply_sync_upserts(&[current])
            .await
            .expect_err("dependent writes use the incoming id, which has no message row");
        assert!(
            error
                .to_string()
                .to_ascii_lowercase()
                .contains("foreign key"),
            "expected a foreign-key failure, got: {error}"
        );
        assert_eq!(
            store.count_messages_by_account(&account.id).await.unwrap(),
            1
        );
    }

    /// Re-running a page (the provider resends it after a failed chunk) must
    /// replace rather than duplicate.
    #[tokio::test]
    async fn re_applying_a_page_is_idempotent() {
        let account = test_account();
        let store = Store::in_memory().await.unwrap();
        store.insert_account(&account).await.unwrap();
        let work = label(&account, "work");
        store.upsert_label(&work).await.unwrap();

        let upserts: Vec<SyncUpsert> = (0..2)
            .map(|index| upsert(&account, index, vec![work.id.clone()]))
            .collect();
        store.apply_sync_upserts(&upserts).await.unwrap();
        store.apply_sync_upserts(&upserts).await.unwrap();

        assert_eq!(
            store.count_messages_by_account(&account.id).await.unwrap(),
            2
        );
        assert_eq!(store.count_message_labels().await.unwrap(), 2);
        let body = store
            .get_body(&upserts[0].envelope.id)
            .await
            .unwrap()
            .expect("body");
        assert_eq!(body.attachments.len(), 1);
    }
}
