use crate::{decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

type CachedBodyTexts = std::collections::HashMap<MessageId, (Option<String>, Option<String>)>;

pub(crate) fn evidence_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<mxr_core::mail_evidence::MailEvidence, sqlx::Error> {
    let metadata: MessageMetadata = row
        .try_get::<Option<String>, _>("evidence_metadata")?
        .as_deref()
        .map(decode_json)
        .transpose()?
        .unwrap_or_default();
    let mut evidence =
        mxr_core::mail_evidence::MailEvidence::cached(&metadata, row.try_get("evidence_text")?);
    evidence.is_reply |= row
        .try_get::<Option<String>, _>("evidence_reply")?
        .is_some_and(|id| mxr_core::mail_evidence::has_reply_reference(&id));
    evidence.thread_correspondence = row.try_get("evidence_thread_correspondence")?;
    evidence.previous_text = row.try_get("evidence_previous_text")?;
    Ok(evidence)
}

impl super::Store {
    /// Body text for a bounded set of trusted message IDs. Previews need no
    /// attachment metadata or parsed message headers; missing bodies stay absent.
    pub async fn body_texts(&self, ids: &[MessageId]) -> Result<CachedBodyTexts, sqlx::Error> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT b.message_id, b.text_plain, b.text_html FROM (SELECT DISTINCT value FROM json_each(?1)) wanted
             CROSS JOIN bodies b ON b.message_id = wanted.value"
        ).bind(encode_json(&ids)?).fetch_all(self.reader()).await?;
        rows.into_iter()
            .map(|(id, plain, html)| Ok((decode_id(&id)?, (plain, html))))
            .collect()
    }

    /// Update only the downloaded attachment row so a stale message body
    /// cannot replace sibling attachments or calendar metadata.
    pub async fn set_attachment_cache(
        &self,
        message_id: &MessageId,
        attachment_id: &AttachmentId,
        path: &std::path::Path,
        size_bytes: u64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE attachments SET local_path = ?, size_bytes = ? WHERE message_id = ? AND id = ?",
        )
        .bind(path.to_string_lossy().to_string())
        .bind(i64::try_from(size_bytes).unwrap_or(i64::MAX))
        .bind(message_id.as_str())
        .bind(attachment_id.as_str())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn get_body(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<MessageBody>, sqlx::Error> {
        let mid = message_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT message_id, text_plain, text_html, fetched_at, metadata_json FROM bodies WHERE message_id = ?"#,
        )
        .bind(&mid)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("body.get_body", started_at, row.is_some());

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let att_mid = message_id.as_str();
        let started_at = std::time::Instant::now();
        let attachments_rows = sqlx::query(
            r#"SELECT id, message_id, filename, mime_type, disposition, content_id, content_location, size_bytes, local_path, provider_id FROM attachments WHERE message_id = ?"#,
        )
        .bind(att_mid)
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "body.get_body.attachments",
            started_at,
            attachments_rows.len(),
        );

        let attachments: Vec<AttachmentMeta> = attachments_rows
            .into_iter()
            .map(|r| {
                Ok(AttachmentMeta {
                    id: decode_id(r.try_get::<&str, _>("id")?)?,
                    message_id: decode_id(r.try_get::<&str, _>("message_id")?)?,
                    filename: r.try_get("filename")?,
                    mime_type: r.try_get("mime_type")?,
                    disposition: decode_attachment_disposition(
                        r.try_get::<&str, _>("disposition")?,
                    )?,
                    content_id: r.try_get("content_id")?,
                    content_location: r.try_get("content_location")?,
                    size_bytes: r.try_get::<i64, _>("size_bytes")? as u64,
                    local_path: r
                        .try_get::<Option<String>, _>("local_path")?
                        .map(std::path::PathBuf::from),
                    provider_id: r.try_get("provider_id")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?;

        let metadata_json: String = row.try_get("metadata_json")?;
        Ok(Some(MessageBody {
            message_id: decode_id(row.try_get::<&str, _>("message_id")?)?,
            text_plain: row.try_get("text_plain")?,
            text_html: row.try_get("text_html")?,
            attachments,
            fetched_at: decode_timestamp(row.try_get("fetched_at")?)?,
            metadata: decode_json(&metadata_json)?,
        }))
    }

    pub async fn insert_body(&self, body: &MessageBody) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        insert_body_tx(&mut tx, body).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Body, attachment, and calendar writes against an open connection so a
/// page of synced messages can share one transaction. Reads go through the
/// same connection: inside an uncommitted transaction the reader pool
/// cannot see the message row this body belongs to.
pub(crate) async fn insert_body_tx(
    conn: &mut sqlx::SqliteConnection,
    body: &MessageBody,
) -> Result<(), sqlx::Error> {
    let fetched_at = body.fetched_at.timestamp();
    let mid = body.message_id.as_str();
    let mut metadata = body.metadata.clone();
    metadata.list_id = mxr_core::mail_evidence::MailEvidence::cached(&metadata, None).list_id;
    let metadata_json = encode_json(&metadata)?;

    sqlx::query(
        "INSERT OR REPLACE INTO bodies (message_id, text_plain, text_html, fetched_at, metadata_json) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&mid)
    .bind(&body.text_plain)
    .bind(&body.text_html)
    .bind(fetched_at)
    .bind(metadata_json)
    .execute(&mut *conn)
    .await?;

    // Promote List-Id from body metadata to the indexed `messages.list_id`
    // column. Cheap upsert; runs once per body and is overwritten if the
    // sender's headers change. Powers `mxr unsub --rank` grouping.
    if let Some(list_id) = metadata.list_id.as_ref() {
        sqlx::query("UPDATE messages SET list_id = ? WHERE id = ?")
            .bind(list_id)
            .bind(&mid)
            .execute(&mut *conn)
            .await?;
    }

    sqlx::query("DELETE FROM attachments WHERE message_id = ?")
        .bind(&mid)
        .execute(&mut *conn)
        .await?;

    for att in &body.attachments {
        let att_id = att.id.as_str();
        let att_mid = att.message_id.as_str();
        let local_path = att
            .local_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let size_bytes = att.size_bytes as i64;
        let disposition = encode_attachment_disposition(att.disposition);
        sqlx::query(
            "INSERT OR REPLACE INTO attachments (id, message_id, filename, mime_type, disposition, content_id, content_location, size_bytes, local_path, provider_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(att_id)
        .bind(att_mid)
        .bind(&att.filename)
        .bind(&att.mime_type)
        .bind(disposition)
        .bind(&att.content_id)
        .bind(&att.content_location)
        .bind(size_bytes)
        .bind(local_path)
        .bind(&att.provider_id)
        .execute(&mut *conn)
        .await?;
    }

    crate::calendar::replace_calendar_invite_for_body_tx(
        conn,
        &body.message_id,
        body.metadata.calendar.as_ref(),
    )
    .await?;

    Ok(())
}

fn encode_attachment_disposition(disposition: AttachmentDisposition) -> &'static str {
    match disposition {
        AttachmentDisposition::Attachment => "attachment",
        AttachmentDisposition::Inline => "inline",
        AttachmentDisposition::Unspecified => "unspecified",
    }
}

fn decode_attachment_disposition(value: &str) -> Result<AttachmentDisposition, sqlx::Error> {
    match value {
        "attachment" => Ok(AttachmentDisposition::Attachment),
        "inline" => Ok(AttachmentDisposition::Inline),
        "unspecified" => Ok(AttachmentDisposition::Unspecified),
        other => Err(sqlx::Error::Protocol(format!(
            "invalid attachment disposition: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::super::Store;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use mxr_core::{
        AttachmentDisposition, AttachmentId, AttachmentMeta, CalendarAttendee, CalendarMetadata,
        CalendarPerson, MessageBody, MessageMetadata,
    };

    fn attachment(message_id: mxr_core::MessageId, provider_id: &str) -> AttachmentMeta {
        AttachmentMeta {
            id: AttachmentId::from_provider_id("test", provider_id),
            message_id,
            filename: format!("{provider_id}.txt"),
            mime_type: "text/plain".into(),
            disposition: AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 10,
            local_path: None,
            provider_id: provider_id.into(),
        }
    }

    fn body(message_id: mxr_core::MessageId, attachments: Vec<AttachmentMeta>) -> MessageBody {
        MessageBody {
            message_id,
            text_plain: Some("body".into()),
            text_html: None,
            attachments,
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[tokio::test]
    async fn preview_body_texts_preserve_empty_and_html_bodies_without_loading_attachments() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mut mails = Vec::new();
        for _ in 0..3 {
            let mut mail = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            mail.provider_id = mail.id.to_string();
            mail.message_id_header = None;
            store.upsert_envelope(&mail).await.unwrap();
            mails.push(mail);
        }
        let mut first = body(
            mails[0].id.clone(),
            vec![attachment(mails[0].id.clone(), "file")],
        );
        first.text_plain = Some(String::new());
        first.text_html = Some("<p>HTML alternative</p>".into());
        store.insert_body(&first).await.unwrap();
        store
            .insert_body(&body(mails[2].id.clone(), vec![]))
            .await
            .unwrap();
        let texts = store
            .body_texts(&[
                mails[0].id.clone(),
                mails[0].id.clone(),
                mails[1].id.clone(),
                mxr_core::MessageId::new(),
            ])
            .await
            .unwrap();
        assert_eq!(
            texts.len(),
            1,
            "missing, duplicate and unrequested IDs do not add rows"
        );
        assert_eq!(
            texts[&mails[0].id],
            (Some(String::new()), Some("<p>HTML alternative</p>".into()))
        );
        assert_eq!(
            store
                .get_body(&mails[0].id)
                .await
                .unwrap()
                .unwrap()
                .attachments
                .len(),
            1,
            "the detail path still returns complete attachments"
        );
        assert!(store.body_texts(&[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn desk_and_place_share_context_in_both_import_orders_with_or_without_a_reference() {
        for sent_direction in [
            mxr_core::MessageDirection::Outbound,
            mxr_core::MessageDirection::Unknown,
            mxr_core::MessageDirection::Inbound,
        ] {
            for (reversed, referenced) in
                [(false, false), (true, false), (false, true), (true, true)]
            {
                let store = Store::in_memory().await.unwrap();
                let account = test_account();
                store.insert_account(&account).await.unwrap();
                let mut sent = TestEnvelopeBuilder::new()
                    .account_id(account.id.clone())
                    .build();
                sent.provider_id = "sent".into();
                sent.from.email.clone_from(&account.email);
                sent.message_id_header = Some("<earlier@example.org>".into());
                sent.date = chrono::Utc::now() - chrono::Duration::hours(2);
                let mut reply = TestEnvelopeBuilder::new()
                    .account_id(account.id.clone())
                    .build();
                reply.provider_id = "reply".into();
                reply.thread_id = sent.thread_id.clone();
                reply.message_id_header = Some("<reply@example.org>".into());
                reply.date = sent.date + chrono::Duration::hours(1);
                reply.in_reply_to = referenced.then(|| sent.message_id_header.clone()).flatten();
                let order = if reversed {
                    [
                        (&reply, mxr_core::MessageDirection::Inbound),
                        (&sent, sent_direction),
                    ]
                } else {
                    [
                        (&sent, sent_direction),
                        (&reply, mxr_core::MessageDirection::Inbound),
                    ]
                };
                for (envelope, direction) in order {
                    store
                        .upsert_envelope_with_direction(envelope, direction)
                        .await
                        .unwrap();
                }
                let previous = body(sent.id.clone(), vec![]);
                store.insert_body(&previous).await.unwrap();
                store
                    .insert_body(&body(reply.id.clone(), vec![]))
                    .await
                    .unwrap();
                let messages = store
                    .desk_messages_in_threads(&account.id, &[sent.thread_id])
                    .await
                    .unwrap();
                let desk = messages
                    .iter()
                    .find(|message| message.id == reply.id)
                    .unwrap();
                let place = store.place_message(&reply.id).await.unwrap().unwrap();
                assert_eq!(desk.evidence.thread_correspondence, sent_direction != mxr_core::MessageDirection::Inbound,
                "known inbound direction must not be reinterpreted, but an owned unknown-direction sender establishes correspondence");
                assert_eq!(desk.evidence.previous_text, previous.text_plain);
                assert_eq!(desk.evidence, place.evidence);
            }
        }
    }

    #[tokio::test]
    async fn cached_list_id_repair_preserves_body_attachments_and_unknown_metadata() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();
        let cached = body(
            envelope.id.clone(),
            vec![attachment(envelope.id.clone(), "kept")],
        );
        store.insert_body(&cached).await.unwrap();
        // Emulate an old cache, including a field this binary does not own.
        sqlx::query("UPDATE bodies SET metadata_json = ? WHERE message_id = ?")
            .bind(r#"{"list_id":null,"raw_headers":"List-Id: Weekly <WEEKLY.EXAMPLE.ORG>\r\n","future_field":{"keep":true}}"#)
            .bind(envelope.id.as_str()).execute(store.writer()).await.unwrap();
        assert_eq!(store.backfill_message_list_ids().await.unwrap(), 1);
        assert_eq!(store.backfill_message_list_ids().await.unwrap(), 0);
        let restored = store.get_body(&envelope.id).await.unwrap().unwrap();
        assert_eq!(restored.text_plain, cached.text_plain);
        assert_eq!(
            serde_json::to_value(&restored.attachments).unwrap(),
            serde_json::to_value(&cached.attachments).unwrap()
        );
        assert_eq!(
            restored.metadata.list_id.as_deref(),
            Some("weekly.example.org")
        );
        let (json, indexed): (String, String) = sqlx::query_as("SELECT b.metadata_json, m.list_id FROM bodies b JOIN messages m ON m.id=b.message_id WHERE m.id=?")
            .bind(envelope.id.as_str()).fetch_one(store.reader()).await.unwrap();
        assert!(json.contains("future_field"));
        assert_eq!(indexed, "weekly.example.org");
    }

    #[tokio::test]
    async fn body_refresh_removes_attachments_no_longer_returned_by_provider() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();

        store
            .insert_body(&body(
                envelope.id.clone(),
                vec![
                    attachment(envelope.id.clone(), "first"),
                    attachment(envelope.id.clone(), "removed"),
                ],
            ))
            .await
            .unwrap();

        store
            .insert_body(&body(
                envelope.id.clone(),
                vec![attachment(envelope.id.clone(), "first")],
            ))
            .await
            .unwrap();

        let refreshed = store.get_body(&envelope.id).await.unwrap().unwrap();
        let provider_ids = refreshed
            .attachments
            .iter()
            .map(|attachment| attachment.provider_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(provider_ids, vec!["first"]);
    }

    #[tokio::test]
    async fn attachment_cache_update_changes_only_the_selected_attachment() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();
        let first = attachment(envelope.id.clone(), "first");
        let second = attachment(envelope.id.clone(), "second");
        store
            .insert_body(&body(
                envelope.id.clone(),
                vec![first.clone(), second.clone()],
            ))
            .await
            .unwrap();

        let path = std::path::Path::new("/private/first.bin");
        assert!(store
            .set_attachment_cache(&envelope.id, &first.id, path, 12)
            .await
            .unwrap());
        assert!(!store
            .set_attachment_cache(&envelope.id, &mxr_core::AttachmentId::new(), path, 12,)
            .await
            .unwrap());

        let refreshed = store.get_body(&envelope.id).await.unwrap().unwrap();
        assert_eq!(refreshed.attachments.len(), 2);
        let cached = refreshed
            .attachments
            .iter()
            .find(|attachment| attachment.id == first.id)
            .unwrap();
        assert_eq!(cached.local_path.as_deref(), Some(path));
        assert_eq!(cached.size_bytes, 12);
        let sibling = refreshed
            .attachments
            .iter()
            .find(|attachment| attachment.id == second.id)
            .unwrap();
        assert_eq!(sibling.local_path, None);
        assert_eq!(sibling.size_bytes, second.size_bytes);
    }

    #[tokio::test]
    async fn insert_body_persists_calendar_invite_for_message() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();

        let mut message_body = body(envelope.id.clone(), Vec::new());
        message_body.metadata.calendar = Some(CalendarMetadata {
            method: Some("REQUEST".into()),
            summary: Some("Planning meeting".into()),
            component_kind: Some("VEVENT".into()),
            uid: Some("planning-123@example.com".into()),
            sequence: Some(2),
            recurrence_id: None,
            dtstamp: Some("20240515T120000Z".into()),
            starts_at: Some("20240520T090000Z".into()),
            ends_at: Some("20240520T093000Z".into()),
            description: None,
            location: Some("Room 4".into()),
            status: None,
            rrule: None,
            organizer: Some(CalendarPerson {
                email: "alice@example.com".into(),
                name: Some("Alice Smith".into()),
                uri: Some("mailto:alice@example.com".into()),
            }),
            attendees: vec![CalendarAttendee {
                email: "bob@example.com".into(),
                name: Some("Bob Example".into()),
                uri: Some("mailto:bob@example.com".into()),
                partstat: Some("NEEDS-ACTION".into()),
                role: None,
                rsvp: Some(true),
            }],
            rsvp_requested: true,
            raw_ics: Some("BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nEND:VCALENDAR\r\n".into()),
            warnings: Vec::new(),
            viewer_partstat: None,
            viewer_attendee_email: None,
            is_update: false,
        });

        store.insert_body(&message_body).await.unwrap();

        let invite = store
            .get_calendar_invite_for_message(&envelope.id)
            .await
            .unwrap()
            .expect("calendar invite row");
        assert_eq!(invite.account_id, account.id);
        assert_eq!(invite.message_id, envelope.id);
        assert_eq!(invite.metadata.summary.as_deref(), Some("Planning meeting"));
        assert_eq!(
            invite.metadata.raw_ics.as_deref(),
            Some("BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nEND:VCALENDAR\r\n")
        );
        assert_eq!(invite.metadata.attendees[0].email, "bob@example.com");
    }

    #[tokio::test]
    async fn backfill_calendar_invites_restores_rows_from_existing_body_metadata() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();

        let mut message_body = body(envelope.id.clone(), Vec::new());
        message_body.metadata.calendar = Some(CalendarMetadata {
            method: Some("REQUEST".into()),
            summary: Some("Planning meeting".into()),
            component_kind: Some("VEVENT".into()),
            uid: Some("planning-123@example.com".into()),
            sequence: Some(2),
            starts_at: Some("20240520T090000Z".into()),
            organizer: Some(CalendarPerson {
                email: "alice@example.com".into(),
                name: Some("Alice Smith".into()),
                uri: Some("mailto:alice@example.com".into()),
            }),
            attendees: vec![CalendarAttendee {
                email: "bob@example.com".into(),
                name: Some("Bob Example".into()),
                uri: Some("mailto:bob@example.com".into()),
                partstat: Some("NEEDS-ACTION".into()),
                role: None,
                rsvp: Some(true),
            }],
            raw_ics: Some("BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nEND:VCALENDAR\r\n".into()),
            ..Default::default()
        });

        store.insert_body(&message_body).await.unwrap();
        sqlx::query("DELETE FROM calendar_invites WHERE message_id = ?")
            .bind(envelope.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        assert!(store
            .get_calendar_invite_for_message(&envelope.id)
            .await
            .unwrap()
            .is_none());

        let backfilled = store
            .backfill_calendar_invites_from_bodies(None)
            .await
            .unwrap();

        assert_eq!(backfilled, 1);
        assert!(store
            .get_calendar_invite_for_message(&envelope.id)
            .await
            .unwrap()
            .is_some());
    }
}
