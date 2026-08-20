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
use mxr_core::id::{LabelId, MessageId, ThreadId};
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

impl SyncUpsert {
    /// Point this message's in-memory ids at `stored_id`.
    ///
    /// The store resolves an envelope by `(account_id, provider_id)` and keeps
    /// the id already on disk, so a message whose id derivation changed
    /// between releases comes back under its old id. Everything written after
    /// the envelope — body, attachments, labels, keywords — and everything the
    /// caller does with the envelope afterwards (search index, reply pairs,
    /// the upserted-id list) has to use that id, or it writes against a
    /// `messages` row that does not exist.
    fn retarget(&mut self, stored_id: MessageId) {
        if self.envelope.id == stored_id {
            return;
        }
        self.envelope.id = stored_id.clone();
        self.body.set_message_id(stored_id);
    }
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
    ///
    /// Takes the page by mutable reference because the store decides each
    /// message's id: an envelope that matches an existing row by
    /// `(account_id, provider_id)` keeps the stored id, and the entry is
    /// rewritten in place so the caller's own follow-up work — search index,
    /// reply pairs, semantic ingest — uses the same id the rows are under.
    ///
    /// Returns the threads this page emptied out of: a message that moved to
    /// another thread leaves its old one behind, and only the row's prior
    /// `thread_id` names it. Callers report those alongside the threads the
    /// page landed in, so clients drop metadata they cached for a thread that
    /// no longer holds the message.
    pub async fn apply_sync_upserts(
        &self,
        upserts: &mut [SyncUpsert],
    ) -> Result<Vec<ThreadId>, sqlx::Error> {
        let mut vacated_threads = Vec::new();
        for chunk in upserts.chunks_mut(UPSERT_CHUNK) {
            let mut tx = self.writer().begin().await?;
            for upsert in chunk {
                let stored =
                    upsert_envelope_tx(&mut tx, &upsert.envelope, upsert.direction).await?;
                vacated_threads.extend(stored.vacated_thread_id);
                upsert.retarget(stored.id);
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
        Ok(vacated_threads)
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

        let mut upserts: Vec<SyncUpsert> = (0..3)
            .map(|index| upsert(&account, index, vec![work.id.clone()]))
            .collect();

        batched.apply_sync_upserts(&mut upserts).await.unwrap();
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

        let mut upserts = vec![upsert(&account, 0, Vec::new())];
        store.apply_sync_upserts(&mut upserts).await.unwrap();

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

        for pass in [&mut first, &mut second] {
            batched
                .apply_sync_upserts(std::slice::from_mut(pass))
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

    /// A row written before 0.4.52 lives under an unscoped id, so a re-sync
    /// computes a different `MessageId` for the same
    /// `(account_id, provider_id)`. The natural key is UNIQUE, so the stored
    /// id wins: the page must land under the id already on disk, with every
    /// dependent row attached to it, and nothing left under the incoming id.
    #[tokio::test]
    async fn a_legacy_row_under_a_different_id_keeps_its_id_and_takes_the_new_page() {
        let account = test_account();
        let work = label(&account, "work");
        let store = Store::in_memory().await.unwrap();
        store.insert_account(&account).await.unwrap();
        store.upsert_label(&work).await.unwrap();

        let mut legacy = vec![upsert(&account, 0, Vec::new())];
        let legacy_id = MessageId::from_provider_id("fake", &legacy[0].envelope.provider_id);
        legacy[0].envelope.id = legacy_id.clone();
        legacy[0].body.message_id = legacy_id.clone();
        legacy[0].body.attachments.clear();
        store.apply_sync_upserts(&mut legacy).await.unwrap();

        // Same provider id, id derived the way the current code derives it.
        let mut current = vec![upsert(&account, 0, vec![work.id.clone()])];
        let incoming_id = current[0].envelope.id.clone();
        assert_ne!(incoming_id, legacy_id, "the two derivations must differ");
        current[0].envelope.subject = "Subject 0 resynced".to_string();
        store.apply_sync_upserts(&mut current).await.unwrap();

        assert_eq!(
            store.count_messages_by_account(&account.id).await.unwrap(),
            1,
            "the re-sync must update the legacy row, not add a second one"
        );
        // The caller reads the ids back off the page it handed in — that is
        // how sync learns which id its search index and reply pairs must use.
        assert_eq!(current[0].envelope.id, legacy_id);
        assert_eq!(current[0].body.message_id, legacy_id);
        assert!(current[0]
            .body
            .attachments
            .iter()
            .all(|attachment| attachment.message_id == legacy_id));

        assert!(store.get_envelope(&incoming_id).await.unwrap().is_none());
        let envelope = store
            .get_envelope(&legacy_id)
            .await
            .unwrap()
            .expect("legacy row");
        assert_eq!(envelope.subject, "Subject 0 resynced");
        let body = store.get_body(&legacy_id).await.unwrap().expect("body");
        assert_eq!(body.text_plain.as_deref(), Some("Body 0"));
        assert_eq!(body.attachments.len(), 1);
        assert_eq!(body.attachments[0].message_id, legacy_id);
        assert_eq!(store.count_message_labels().await.unwrap(), 1);
        assert!(!store
            .get_message_keywords(&legacy_id)
            .await
            .unwrap()
            .is_empty());
        assert!(store.get_body(&incoming_id).await.unwrap().is_none());
    }

    /// A provider that re-threads a message overwrites `messages.thread_id`,
    /// so after the write nothing else can name the thread it left. The page
    /// reports it, and callers pass it on as a changed thread.
    #[tokio::test]
    async fn moving_a_message_between_threads_reports_the_thread_it_left() {
        let account = test_account();
        let store = Store::in_memory().await.unwrap();
        store.insert_account(&account).await.unwrap();

        let mut first = vec![upsert(&account, 0, Vec::new())];
        let original_thread = first[0].envelope.thread_id.clone();
        let vacated = store.apply_sync_upserts(&mut first).await.unwrap();
        assert!(
            vacated.is_empty(),
            "a message that is new to the store leaves no thread behind"
        );

        let mut moved = vec![upsert(&account, 0, Vec::new())];
        let new_thread = mxr_core::id::ThreadId::from_scoped_provider_id(
            &account.id,
            "fake",
            "thread-after-merge",
        );
        moved[0].envelope.thread_id = new_thread.clone();
        assert_ne!(original_thread, new_thread);

        let vacated = store.apply_sync_upserts(&mut moved).await.unwrap();
        assert_eq!(vacated, vec![original_thread]);
        let envelope = store
            .get_envelope(&moved[0].envelope.id)
            .await
            .unwrap()
            .expect("envelope");
        assert_eq!(envelope.thread_id, new_thread);

        // Re-applying the same page changes nothing, so nothing is vacated.
        let vacated = store.apply_sync_upserts(&mut moved).await.unwrap();
        assert!(vacated.is_empty());
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

        let mut upserts: Vec<SyncUpsert> = (0..2)
            .map(|index| upsert(&account, index, vec![work.id.clone()]))
            .collect();
        store.apply_sync_upserts(&mut upserts).await.unwrap();
        store.apply_sync_upserts(&mut upserts).await.unwrap();

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
