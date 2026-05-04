use crate::{decode_id, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use std::time::Instant;

impl super::Store {
    /// Append a state-transition event for a message. Callers must have
    /// already detected a real transition; this method writes unconditionally.
    pub async fn insert_message_event(
        &self,
        event: &MessageEvent,
    ) -> Result<(), sqlx::Error> {
        let message_id = event.message_id.as_str();
        let account_id = event.account_id.as_str();
        let event_type = event.event_type.as_db_str();
        let source = event.source.as_db_str();
        let label_id = event.label_id.as_ref().map(|l| l.as_str().to_string());
        let occurred_at = event.occurred_at;
        let metadata_json = event.metadata_json.as_deref();
        sqlx::query!(
            r#"
            INSERT INTO message_events (
                message_id, account_id, event_type, source,
                label_id, occurred_at, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            message_id,
            account_id,
            event_type,
            source,
            label_id,
            occurred_at,
            metadata_json,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// List all events for a message, ordered by occurrence ascending.
    pub async fn list_message_events(
        &self,
        message_id: &MessageId,
    ) -> Result<Vec<MessageEvent>, sqlx::Error> {
        let started_at = Instant::now();
        let mid = message_id.as_str();
        let rows = sqlx::query!(
            r#"
            SELECT
                message_id  as "message_id!",
                account_id  as "account_id!",
                event_type  as "event_type!",
                source      as "source!",
                label_id,
                occurred_at as "occurred_at!: i64",
                metadata_json
            FROM message_events
            WHERE message_id = ?
            ORDER BY occurred_at ASC, id ASC
            "#,
            mid,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("message_events.list_for_message", started_at, rows.len());

        let mut events = Vec::with_capacity(rows.len());
        for r in rows {
            let event_type = MessageEventType::from_db_str(&r.event_type).ok_or_else(|| {
                sqlx::Error::Decode(
                    format!("unknown message_events.event_type: {}", r.event_type).into(),
                )
            })?;
            let source = EventSource::from_db_str(&r.source).ok_or_else(|| {
                sqlx::Error::Decode(
                    format!("unknown message_events.source: {}", r.source).into(),
                )
            })?;
            events.push(MessageEvent {
                message_id: decode_id(&r.message_id)?,
                account_id: decode_id(&r.account_id)?,
                event_type,
                source,
                label_id: r.label_id.as_deref().map(decode_id).transpose()?,
                occurred_at: r.occurred_at,
                metadata_json: r.metadata_json,
            });
        }
        Ok(events)
    }
}
