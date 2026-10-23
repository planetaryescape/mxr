use chrono::{Datelike, Duration, NaiveTime, TimeZone, Utc, Weekday};
use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    /// Build the reply's pair from the reply's direction as stored when this
    /// runs, inside the writer transaction. Sync writes the envelope and then
    /// calls this, and a repair can land between the two. Building from the
    /// direction sync computed earlier would overwrite the repaired pair.
    /// Returns `Ok(true)` if a pair exists after the call, `Ok(false)` if the
    /// parent is not stored yet (the reply is left on the pending queue).
    pub async fn try_create_reply_pair(&self, reply: &Envelope) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let reply_id = reply.id.as_str();
        Self::reseat_reply_pair_tx(&mut tx, &reply_id).await?;
        let linked: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM reply_pairs WHERE reply_message_id = ?)",
        )
        .bind(reply_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(linked)
    }

    /// One-shot backfill that scans every classified message with an
    /// `in_reply_to` header and creates `reply_pairs` rows for those whose
    /// parent already lives in the local store. Idempotent via
    /// `INSERT OR IGNORE`; called by `mxr doctor --rebuild-analytics`.
    /// Returns the number of new pairs.
    pub async fn backfill_reply_pairs_from_messages(&self) -> Result<u32, sqlx::Error> {
        let result = sqlx::query(PAIR_BACKFILL_SQL)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected() as u32)
    }

    /// Rebuild one stored reply's pair from its current direction, inside the
    /// caller's transaction. A reply whose parent is not stored yet goes back
    /// on the pending queue, as sync leaves it.
    pub(crate) async fn reseat_reply_pair_tx(
        tx: &mut sqlx::SqliteConnection,
        reply_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM reply_pairs WHERE reply_message_id = ?")
            .bind(reply_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(PAIR_FOR_REPLY_SQL)
            .bind(reply_id)
            .execute(&mut *tx)
            .await?;
        // The rebuilt pair starts without business-hours latency, which the
        // backfill would only fill later. Compute it now, as the backfill does.
        let window: Option<(i64, i64)> = sqlx::query_as(
            "SELECT parent_received_at, replied_at FROM reply_pairs WHERE reply_message_id = ?",
        )
        .bind(reply_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((start, end)) = window {
            sqlx::query(
                "UPDATE reply_pairs SET business_hours_latency_seconds = ?
                 WHERE reply_message_id = ?",
            )
            .bind(compute_business_hours_seconds(start, end))
            .bind(reply_id)
            .execute(&mut *tx)
            .await?;
        }
        // A pending row is for a reply whose pair is still missing. Once the
        // pair exists, the row is stale.
        sqlx::query(
            "DELETE FROM reply_pair_pending
             WHERE reply_message_id = ?1
               AND EXISTS (SELECT 1 FROM reply_pairs WHERE reply_message_id = ?1)",
        )
        .bind(reply_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO reply_pair_pending
                (reply_message_id, in_reply_to_header, account_id, created_at)
             SELECT id, in_reply_to, account_id, strftime('%s', 'now')
             FROM messages
             WHERE id = ?1 AND in_reply_to IS NOT NULL AND in_reply_to != ''
               AND direction IN ('inbound', 'outbound')
               AND NOT EXISTS (SELECT 1 FROM reply_pairs WHERE reply_message_id = ?1)",
        )
        .bind(reply_id)
        .execute(&mut *tx)
        .await?;
        Ok(())
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
    /// Returns the count migrated. Each row is rebuilt in its own transaction
    /// from the reply's stored direction, so a repair that lands while the
    /// queue is walked is not overwritten. Idempotent and crash-safe.
    pub async fn reconcile_reply_pair_pending(&self) -> Result<u32, sqlx::Error> {
        let pending: Vec<String> =
            sqlx::query_scalar("SELECT reply_message_id FROM reply_pair_pending")
                .fetch_all(self.reader())
                .await?;
        let mut migrated = 0u32;
        for reply_id in pending {
            let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
            Self::reseat_reply_pair_tx(&mut tx, &reply_id).await?;
            let linked: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM reply_pairs WHERE reply_message_id = ?)",
            )
            .bind(&reply_id)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            if linked {
                migrated += 1;
            }
        }
        Ok(migrated)
    }

    /// Rebuild every pair of `account_id` whose recorded direction or
    /// counterparty disagrees with its reply's stored direction, inside the
    /// caller's transaction. Covers pairs a late writer put back with an old
    /// classification, which the flip itself never sees. Returns the count.
    pub(crate) async fn reseat_mismatched_pairs_tx(
        tx: &mut sqlx::SqliteConnection,
        account_id: &str,
    ) -> Result<u32, sqlx::Error> {
        let stale: Vec<String> = sqlx::query_scalar(
            r#"SELECT p.reply_message_id
               FROM reply_pairs p
               JOIN messages reply ON reply.id = p.reply_message_id
               JOIN messages parent ON parent.id = p.parent_message_id
               WHERE reply.account_id = ?1
                 AND reply.direction IN ('inbound', 'outbound')
                 AND (
                     p.direction != CASE WHEN reply.direction = 'outbound'
                                         THEN 'i_replied' ELSE 'they_replied' END
                  OR p.counterparty_email != CASE WHEN reply.direction = 'outbound'
                                                  THEN parent.from_email
                                                  ELSE COALESCE(json_extract(parent.to_addrs, '$[0].email'), '')
                                             END
                 )"#,
        )
        .bind(account_id)
        .fetch_all(&mut *tx)
        .await?;
        for reply_id in &stale {
            Self::reseat_reply_pair_tx(tx, reply_id).await?;
        }
        Ok(stale.len() as u32)
    }
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

    let business_open = NaiveTime::from_hms_opt(9, 0, 0).expect("09:00:00 is a valid time literal");
    let business_close =
        NaiveTime::from_hms_opt(17, 0, 0).expect("17:00:00 is a valid time literal");

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
        .map(std::string::ToString::to_string)
}

#[allow(dead_code)]
fn _types_used(_: AccountId, _: MessageId) {}

/// Builds `reply_pairs` rows from stored replies, using their stored direction.
/// The tail restricts it: none for the backfill, `reply.id = ?1` for one reply.
/// The one-reply form keeps SQLite on the `messages.id` index, which an
/// optional `?1 IS NULL OR` filter defeats.
macro_rules! pair_from_stored_reply_sql {
    ($tail:literal) => {
        concat!(
            "INSERT OR IGNORE INTO reply_pairs (
        reply_message_id, parent_message_id, account_id,
        counterparty_email, direction,
        parent_received_at, replied_at, latency_seconds,
        business_hours_latency_seconds, created_at
    )
    SELECT
        reply.id,
        parent.id,
        reply.account_id,
        CASE
            WHEN reply.direction = 'outbound' THEN parent.from_email
            ELSE COALESCE(json_extract(parent.to_addrs, '$[0].email'), '')
        END,
        CASE
            WHEN reply.direction = 'outbound' THEN 'i_replied'
            ELSE 'they_replied'
        END,
        parent.date,
        reply.date,
        MAX(reply.date - parent.date, 0),
        NULL,
        strftime('%s', 'now')
    FROM messages reply
    JOIN messages parent
        ON parent.account_id = reply.account_id
        AND parent.message_id_header = reply.in_reply_to
    WHERE reply.in_reply_to IS NOT NULL
        AND reply.in_reply_to != ''
        AND reply.direction IN ('inbound', 'outbound')
        AND parent.message_id_header IS NOT NULL
        AND parent.message_id_header != ''
        AND reply.id != parent.id",
            $tail
        )
    };
}

const PAIR_BACKFILL_SQL: &str = pair_from_stored_reply_sql!("");
pub(crate) const PAIR_FOR_REPLY_SQL: &str = pair_from_stored_reply_sql!(" AND reply.id = ?1");
