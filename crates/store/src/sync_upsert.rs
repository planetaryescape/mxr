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
