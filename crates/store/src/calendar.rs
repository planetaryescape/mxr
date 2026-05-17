use crate::{decode_id, decode_json, encode_json};
use chrono::Utc;
use mxr_core::id::{AccountId, CalendarInviteId, MessageId};
use mxr_core::types::{CalendarMetadata, MessageMetadata};
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct CalendarInviteRecord {
    pub id: CalendarInviteId,
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub metadata: CalendarMetadata,
    pub created_at: i64,
    pub updated_at: i64,
}

impl super::Store {
    pub async fn get_calendar_invite_for_message(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<CalendarInviteRecord>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, account_id, message_id, metadata_json, created_at, updated_at
             FROM calendar_invites
             WHERE message_id = ?
             ORDER BY updated_at DESC
             LIMIT 1",
        )
        .bind(message_id.as_str())
        .fetch_optional(self.reader())
        .await?;

        row.map(row_to_calendar_invite_record).transpose()
    }

    pub async fn list_calendar_invites(
        &self,
        limit: u32,
    ) -> Result<Vec<CalendarInviteRecord>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, account_id, message_id, metadata_json, created_at, updated_at
             FROM calendar_invites
             ORDER BY COALESCE(starts_at, updated_at) DESC
             LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(row_to_calendar_invite_record)
            .collect()
    }

    pub async fn update_calendar_invite_partstat(
        &self,
        message_id: &MessageId,
        attendee_email: &str,
        partstat: &str,
    ) -> Result<(), sqlx::Error> {
        let Some(mut invite) = self.get_calendar_invite_for_message(message_id).await? else {
            return Ok(());
        };
        for attendee in &mut invite.metadata.attendees {
            if attendee.email.eq_ignore_ascii_case(attendee_email) {
                attendee.partstat = Some(partstat.to_string());
            }
        }
        let metadata_json = encode_json(&invite.metadata)?;
        let updated_at = Utc::now().timestamp();
        sqlx::query(
            "UPDATE calendar_invites
             SET metadata_json = ?, current_partstat = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(metadata_json)
        .bind(partstat)
        .bind(updated_at)
        .bind(invite.id.as_str())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn calendar_invite_has_newer_sequence(
        &self,
        account_id: &AccountId,
        message_id: &MessageId,
        uid: &str,
        recurrence_id: Option<&str>,
        sequence: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let current_sequence = sequence.unwrap_or(0);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM calendar_invites
             WHERE account_id = ?
               AND message_id != ?
               AND uid = ?
               AND COALESCE(recurrence_id, '') = COALESCE(?, '')
               AND COALESCE(sequence, 0) > ?",
        )
        .bind(account_id.as_str())
        .bind(message_id.as_str())
        .bind(uid)
        .bind(recurrence_id)
        .bind(current_sequence)
        .fetch_one(self.reader())
        .await?;
        Ok(count > 0)
    }

    /// True iff a stored invite exists for the same `(account_id, uid,
    /// recurrence_id)` with a *lower* sequence than the one given — i.e.
    /// this invite is an update/reschedule of a prior REQUEST. Used to fill
    /// `CalendarMetadata.is_update`.
    pub async fn calendar_invite_has_earlier_sequence(
        &self,
        account_id: &AccountId,
        message_id: &MessageId,
        uid: &str,
        recurrence_id: Option<&str>,
        sequence: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let current_sequence = sequence.unwrap_or(0);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM calendar_invites
             WHERE account_id = ?
               AND message_id != ?
               AND uid = ?
               AND COALESCE(recurrence_id, '') = COALESCE(?, '')
               AND COALESCE(sequence, 0) < ?",
        )
        .bind(account_id.as_str())
        .bind(message_id.as_str())
        .bind(uid)
        .bind(recurrence_id)
        .bind(current_sequence)
        .fetch_one(self.reader())
        .await?;
        Ok(count > 0)
    }

    pub async fn calendar_invite_has_different_organizer(
        &self,
        account_id: &AccountId,
        message_id: &MessageId,
        uid: &str,
        recurrence_id: Option<&str>,
        organizer_email: &str,
    ) -> Result<bool, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM calendar_invites
             WHERE account_id = ?
               AND message_id != ?
               AND uid = ?
               AND COALESCE(recurrence_id, '') = COALESCE(?, '')
               AND organizer_email IS NOT NULL
               AND LOWER(organizer_email) != LOWER(?)",
        )
        .bind(account_id.as_str())
        .bind(message_id.as_str())
        .bind(uid)
        .bind(recurrence_id)
        .bind(organizer_email)
        .fetch_one(self.reader())
        .await?;
        Ok(count > 0)
    }

    pub async fn backfill_calendar_invites_from_bodies(&self) -> Result<u64, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT message_id, metadata_json
             FROM bodies
             WHERE metadata_json LIKE '%\"calendar\"%'",
        )
        .fetch_all(self.reader())
        .await?;

        let mut backfilled = 0;
        for row in rows {
            let message_id: MessageId = decode_id(row.try_get::<&str, _>("message_id")?)?;
            let metadata_json: String = row.try_get("metadata_json")?;
            let metadata: MessageMetadata = decode_json(&metadata_json)?;
            if let Some(calendar) = metadata.calendar.as_ref() {
                self.replace_calendar_invite_for_body(&message_id, Some(calendar))
                    .await?;
                backfilled += 1;
            }
        }
        Ok(backfilled)
    }

    pub(crate) async fn replace_calendar_invite_for_body(
        &self,
        message_id: &MessageId,
        calendar: Option<&CalendarMetadata>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM calendar_invites WHERE message_id = ?")
            .bind(message_id.as_str())
            .execute(self.writer())
            .await?;

        let Some(calendar) = calendar else {
            return Ok(());
        };

        let account_id: Option<String> =
            sqlx::query_scalar("SELECT account_id FROM messages WHERE id = ?")
                .bind(message_id.as_str())
                .fetch_optional(self.reader())
                .await?;
        let Some(account_id) = account_id else {
            return Ok(());
        };

        let id = CalendarInviteId::from_provider_id(
            "calendar",
            &format!(
                "{}:{}:{}:{}:{}",
                account_id,
                message_id,
                calendar.uid.as_deref().unwrap_or(""),
                calendar.recurrence_id.as_deref().unwrap_or(""),
                calendar.method.as_deref().unwrap_or("")
            ),
        );
        let metadata_json = encode_json(calendar)?;
        let now = Utc::now().timestamp();
        let organizer_email = calendar
            .organizer
            .as_ref()
            .map(|organizer| organizer.email.clone());
        let current_partstat = calendar
            .attendees
            .iter()
            .find_map(|attendee| attendee.partstat.clone());

        sqlx::query(
            "INSERT INTO calendar_invites (
                 id, account_id, message_id, method, uid, recurrence_id, sequence,
                 summary, starts_at, ends_at, organizer_email, current_partstat,
                 rsvp_requested, metadata_json, raw_ics, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(account_id)
        .bind(message_id.as_str())
        .bind(&calendar.method)
        .bind(&calendar.uid)
        .bind(&calendar.recurrence_id)
        .bind(calendar.sequence)
        .bind(&calendar.summary)
        .bind(&calendar.starts_at)
        .bind(&calendar.ends_at)
        .bind(organizer_email)
        .bind(current_partstat)
        .bind(calendar.rsvp_requested)
        .bind(metadata_json)
        .bind(&calendar.raw_ics)
        .bind(now)
        .bind(now)
        .execute(self.writer())
        .await?;

        Ok(())
    }
}

fn row_to_calendar_invite_record(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CalendarInviteRecord, sqlx::Error> {
    let metadata_json: String = row.try_get("metadata_json")?;
    Ok(CalendarInviteRecord {
        id: decode_id(row.try_get::<&str, _>("id")?)?,
        account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
        message_id: decode_id(row.try_get::<&str, _>("message_id")?)?,
        metadata: decode_json(&metadata_json)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
