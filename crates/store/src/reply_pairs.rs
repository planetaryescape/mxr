use chrono::{Datelike, Duration, NaiveTime, TimeZone, Utc, Weekday};
use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    /// Attempt to link a reply to its parent. Looks up the parent by
    /// `message_id_header`; on success inserts a `reply_pairs` row. Returns
    /// `Ok(true)` if a pair was created, `Ok(false)` if the parent isn't yet
    /// known locally (caller should follow up with `enqueue_reply_pair_pending`).
    ///
    /// Direction logic: `reply_direction == Outbound` ⇒ `'i_replied'` (I replied
    /// to their inbound message). `reply_direction == Inbound` ⇒ `'they_replied'`
    /// (they replied to my outbound message). When direction is `Unknown` we
    /// skip pair creation — the reconciler retries later once the address
    /// cache is populated.
    pub async fn try_create_reply_pair(
        &self,
        reply: &Envelope,
        reply_direction: MessageDirection,
    ) -> Result<bool, sqlx::Error> {
        let in_reply_to = match reply.in_reply_to.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => return Ok(false),
        };
        let pair_direction = match reply_direction {
            MessageDirection::Outbound => "i_replied",
            MessageDirection::Inbound => "they_replied",
            MessageDirection::Unknown => return Ok(false),
        };

        let reply_account_id = reply.account_id.as_str();
        // Resolve parent by RFC 5322 Message-ID within the same account.
        let parent_row = sqlx::query!(
            r#"SELECT id as "id!", date as "date!", from_email as "from_email!",
                      to_addrs as "to_addrs!"
               FROM messages
               WHERE account_id = ? AND message_id_header = ?
               LIMIT 1"#,
            reply_account_id,
            in_reply_to,
        )
        .fetch_optional(self.reader())
        .await?;

        let Some(parent) = parent_row else {
            return Ok(false);
        };

        // Counterparty: for `i_replied` the inbound parent's `from_email`;
        // for `they_replied` the outbound parent's first `to_addrs` entry.
        let counterparty = match pair_direction {
            "i_replied" => parent.from_email,
            _ => first_recipient_email(&parent.to_addrs).unwrap_or_default(),
        };

        let parent_received_at: i64 = parent.date;
        let replied_at: i64 = reply.date.timestamp();
        let latency_seconds: i64 = (replied_at - parent_received_at).max(0);

        let reply_id_str = reply.id.as_str();
        sqlx::query!(
            "INSERT OR REPLACE INTO reply_pairs (
                reply_message_id, parent_message_id, account_id,
                counterparty_email, direction,
                parent_received_at, replied_at, latency_seconds,
                business_hours_latency_seconds, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
            reply_id_str,
            parent.id,
            reply_account_id,
            counterparty,
            pair_direction,
            parent_received_at,
            replied_at,
            latency_seconds,
            replied_at,
        )
        .execute(self.writer())
        .await?;

        // Pair resolved — drop any pending row that was waiting for this reply.
        sqlx::query!(
            "DELETE FROM reply_pair_pending WHERE reply_message_id = ?",
            reply_id_str,
        )
        .execute(self.writer())
        .await?;
        Ok(true)
    }

    /// Park a reply for later resolution when the parent isn't available yet.
    /// Backfill `business_hours_latency_seconds` on rows where it's still null.
    /// Cheap idempotent UPDATE per row using the pure helper. Called by the
    /// reconciler loop and `mxr doctor --rebuild-analytics`.
    pub async fn backfill_business_hours_latency(&self) -> Result<u32, sqlx::Error> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            r#"SELECT reply_message_id, parent_received_at, replied_at
               FROM reply_pairs
               WHERE business_hours_latency_seconds IS NULL"#,
        )
        .fetch_all(self.reader())
        .await?;
        let mut updated = 0u32;
        for (id, start, end) in rows {
            let secs = compute_business_hours_seconds(start, end);
            sqlx::query!(
                "UPDATE reply_pairs SET business_hours_latency_seconds = ?
                 WHERE reply_message_id = ?",
                secs,
                id,
            )
            .execute(self.writer())
            .await?;
            updated += 1;
        }
        Ok(updated)
    }

    pub async fn enqueue_reply_pair_pending(&self, reply: &Envelope) -> Result<(), sqlx::Error> {
        let in_reply_to = match reply.in_reply_to.as_deref() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => return Ok(()),
        };
        let reply_id_str = reply.id.as_str();
        let account_id_str = reply.account_id.as_str();
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            "INSERT INTO reply_pair_pending
                (reply_message_id, in_reply_to_header, account_id, created_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(reply_message_id) DO UPDATE SET
                in_reply_to_header = excluded.in_reply_to_header,
                created_at = excluded.created_at",
            reply_id_str,
            in_reply_to,
            account_id_str,
            now,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Walk the pending queue and resolve any rows whose parent is now known.
    /// Returns the count migrated. Slice 10 wires this into a periodic loop;
    /// Slice 15's doctor command also calls it. Idempotent and crash-safe.
    pub async fn reconcile_reply_pair_pending(&self) -> Result<u32, sqlx::Error> {
        let pending = sqlx::query!(
            r#"SELECT
                pending.reply_message_id  as "reply_message_id!",
                pending.in_reply_to_header as "in_reply_to_header!",
                reply.id                  as "reply_id!",
                reply.account_id          as "reply_account_id!",
                reply.date                as "reply_date!",
                reply.in_reply_to         as "reply_in_reply_to",
                reply.from_email          as "reply_from_email!",
                reply.direction           as "reply_direction!"
               FROM reply_pair_pending pending
               JOIN messages reply ON reply.id = pending.reply_message_id"#,
        )
        .fetch_all(self.reader())
        .await?;

        let mut migrated = 0u32;
        for row in pending {
            let direction = MessageDirection::from_db_str(&row.reply_direction)
                .unwrap_or(MessageDirection::Unknown);
            let envelope_stub = ReplyStubEnvelope {
                id: row.reply_id,
                account_id: row.reply_account_id,
                in_reply_to: row.reply_in_reply_to,
                date_unix: row.reply_date,
            };
            if self
                .try_create_reply_pair_from_stub(&envelope_stub, direction)
                .await?
            {
                migrated += 1;
            }
        }
        Ok(migrated)
    }

    async fn try_create_reply_pair_from_stub(
        &self,
        stub: &ReplyStubEnvelope,
        reply_direction: MessageDirection,
    ) -> Result<bool, sqlx::Error> {
        let in_reply_to = match stub.in_reply_to.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => return Ok(false),
        };
        let pair_direction = match reply_direction {
            MessageDirection::Outbound => "i_replied",
            MessageDirection::Inbound => "they_replied",
            MessageDirection::Unknown => return Ok(false),
        };

        let parent_row = sqlx::query!(
            r#"SELECT id as "id!", date as "date!", from_email as "from_email!",
                      to_addrs as "to_addrs!"
               FROM messages
               WHERE account_id = ? AND message_id_header = ?
               LIMIT 1"#,
            stub.account_id,
            in_reply_to,
        )
        .fetch_optional(self.reader())
        .await?;
        let Some(parent) = parent_row else {
            return Ok(false);
        };

        let counterparty = match pair_direction {
            "i_replied" => parent.from_email,
            _ => first_recipient_email(&parent.to_addrs).unwrap_or_default(),
        };
        let parent_received_at: i64 = parent.date;
        let replied_at: i64 = stub.date_unix;
        let latency_seconds: i64 = (replied_at - parent_received_at).max(0);
        sqlx::query!(
            "INSERT OR REPLACE INTO reply_pairs (
                reply_message_id, parent_message_id, account_id,
                counterparty_email, direction,
                parent_received_at, replied_at, latency_seconds,
                business_hours_latency_seconds, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
            stub.id,
            parent.id,
            stub.account_id,
            counterparty,
            pair_direction,
            parent_received_at,
            replied_at,
            latency_seconds,
            replied_at,
        )
        .execute(self.writer())
        .await?;
        sqlx::query!(
            "DELETE FROM reply_pair_pending WHERE reply_message_id = ?",
            stub.id,
        )
        .execute(self.writer())
        .await?;
        Ok(true)
    }
}

struct ReplyStubEnvelope {
    id: String,
    account_id: String,
    in_reply_to: Option<String>,
    date_unix: i64,
}

/// Compute business-hours seconds between two unix timestamps using the
/// default schedule (Monday–Friday, 09:00–17:00 UTC). Pure function — easily
/// unit-testable and the canonical building block for `mxr response-time
/// --working-hours`.
///
/// Time zones: this works in UTC. Slice 14's plan says local-time is the
/// eventual default, but UTC keeps the pure function deterministic and
/// independent of the host's TZ database. Localization is a follow-up.
pub fn compute_business_hours_seconds(start_unix: i64, end_unix: i64) -> i64 {
    if end_unix <= start_unix {
        return 0;
    }
    let start = Utc.timestamp_opt(start_unix, 0).single();
    let end = Utc.timestamp_opt(end_unix, 0).single();
    let (Some(start), Some(end)) = (start, end) else {
        return 0;
    };

    let business_open = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
    let business_close = NaiveTime::from_hms_opt(17, 0, 0).unwrap();

    let mut total: i64 = 0;
    let mut cursor_date = start.date_naive();
    let end_date = end.date_naive();

    while cursor_date <= end_date {
        if matches!(cursor_date.weekday(), Weekday::Sat | Weekday::Sun) {
            cursor_date = cursor_date.succ_opt().expect("date overflow");
            continue;
        }
        let day_open = Utc
            .from_utc_datetime(&cursor_date.and_time(business_open))
            .timestamp();
        let day_close = Utc
            .from_utc_datetime(&cursor_date.and_time(business_close))
            .timestamp();

        let window_start = day_open.max(start_unix);
        let window_end = day_close.min(end_unix);
        if window_end > window_start {
            total += window_end - window_start;
        }
        cursor_date = cursor_date.succ_opt().expect("date overflow");
    }
    total
}

/// Suppress unused-import warnings for the imports above when feature flags
/// trim some paths. Cheap, no runtime cost.
#[allow(dead_code)]
fn _drop_imports() {
    let _ = Duration::seconds(0);
    let _ = (0i64).hours();
}
trait _UnusedHourMarker {
    fn hours(&self) -> i64;
}
impl _UnusedHourMarker for i64 {
    fn hours(&self) -> i64 {
        *self
    }
}

/// Pull the first `email` field out of the JSON-encoded `to_addrs` array on
/// `messages`. Best-effort — if the JSON is malformed or empty, returns None.
pub(crate) fn first_recipient_email(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = value.as_array()?;
    let first = arr.first()?;
    first
        .get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[allow(dead_code)]
fn _types_used(_: AccountId, _: MessageId) {}
