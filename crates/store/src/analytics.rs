use crate::{decode_id, decode_timestamp, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use std::time::Instant;

/// Earliest plausible Unix timestamp for an email date — 2000-01-01 UTC.
/// Messages with a `date` before this almost always have a corrupt or
/// unparseable Date: header that fell back to epoch 0 at parse time, and
/// they pollute time-based analytics. Hardcoded floor; nothing legitimate
/// in this codebase ever lands earlier.
const EARLIEST_PLAUSIBLE_TS: i64 = 946_684_800;

/// Reclassify every `direction='unknown'` row using a closure that queries
/// the account-address lookup. Returns the number of rows updated. Called
/// by `mxr doctor --rebuild-analytics` (Slice 15).
impl super::Store {
    pub async fn reclassify_unknown_directions(
        &self,
        is_account_address: impl Fn(&str) -> bool,
    ) -> Result<u32, sqlx::Error> {
        let started_at = Instant::now();
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"SELECT id, from_email
               FROM messages
               WHERE direction = 'unknown'"#,
        )
        .fetch_all(self.reader())
        .await?;
        let mut updated = 0u32;
        for (id, from_email) in rows {
            let new_direction = if is_account_address(&from_email) {
                "outbound"
            } else {
                "inbound"
            };
            sqlx::query!(
                "UPDATE messages SET direction = ? WHERE id = ?",
                new_direction,
                id,
            )
            .execute(self.writer())
            .await?;
            updated += 1;
        }
        trace_query(
            "analytics.reclassify_unknown_directions",
            started_at,
            updated as usize,
        );
        Ok(updated)
    }

    /// Promote `bodies.metadata_json -> $.list_id` into `messages.list_id`
    /// for rows where the column is still null. Touches only rows whose
    /// body has been cached. Idempotent.
    pub async fn backfill_message_list_ids(&self) -> Result<u32, sqlx::Error> {
        let started_at = Instant::now();
        let result = sqlx::query(
            r#"UPDATE messages
               SET list_id = json_extract(bodies.metadata_json, '$.list_id')
               FROM bodies
               WHERE messages.id = bodies.message_id
                 AND messages.list_id IS NULL
                 AND json_extract(bodies.metadata_json, '$.list_id') IS NOT NULL"#,
        )
        .execute(self.writer())
        .await?;
        let n = result.rows_affected() as u32;
        trace_query("analytics.backfill_list_ids", started_at, n as usize);
        Ok(n)
    }
}

impl super::Store {
    /// Top-N single messages by `size_bytes`. Drives `mxr storage --by message`
    /// — lets users find the one giant attachment that's eating their disk
    /// rather than just the rolled-up bucket totals. Returns up to `limit`
    /// rows ordered by size descending. Filters to in-window messages and
    /// drops bogus epoch-0 dates so spam with a 1970 Date: header doesn't
    /// claim the top spot.
    pub async fn largest_messages(
        &self,
        account_id: Option<&AccountId>,
        since_unix: Option<i64>,
        limit: u32,
    ) -> Result<Vec<LargestMessageRow>, sqlx::Error> {
        let started_at = Instant::now();
        let lim = limit as i64;
        let account_filter: Option<String> = account_id.map(|a| a.as_str());
        let since = since_unix.unwrap_or(0).max(EARLIEST_PLAUSIBLE_TS);

        let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
            r#"SELECT id, from_email, subject, size_bytes, date
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2
                 AND size_bytes > 0
               ORDER BY size_bytes DESC
               LIMIT ?3"#,
        )
        .bind(account_filter)
        .bind(since)
        .bind(lim)
        .fetch_all(self.reader())
        .await?;
        trace_query("analytics.largest_messages", started_at, rows.len());

        rows.into_iter()
            .map(|(id, from_email, subject, size_bytes, date)| {
                Ok(LargestMessageRow {
                    message_id: decode_id(&id)?,
                    from_email,
                    subject,
                    size_bytes: size_bytes.max(0) as u64,
                    date: decode_timestamp(date)?,
                })
            })
            .collect()
    }
}

/// Linear-interpolation percentile on a sorted slice. Returns 0 for empty
/// inputs; clamps `q` to [0, 1].
fn percentile(sorted: &[i64], q: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let q = q.clamp(0.0, 1.0);
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)].max(0) as u32
}

impl super::Store {
    /// Compute clock and (optional) business-hours percentile reply
    /// latencies for a direction. Loads the latency vector into memory and
    /// sorts in Rust — fine for typical datasets (<50k pairs); past that,
    /// consider a SQLite extension or pre-aggregating.
    pub async fn list_response_time(
        &self,
        account_id: Option<&AccountId>,
        direction: ResponseTimeDirection,
        counterparty: Option<&str>,
        since_days: Option<u32>,
    ) -> Result<ResponseTimeSummary, sqlx::Error> {
        let started_at = Instant::now();
        let account_filter: Option<String> = account_id.map(|a| a.as_str());
        let direction_str = direction.as_db_str();
        let counterparty_filter = counterparty.map(|c| c.to_lowercase());
        let since_cutoff: Option<i64> =
            since_days.map(|d| chrono::Utc::now().timestamp() - i64::from(d) * 86_400);

        let rows: Vec<(i64, Option<i64>)> = sqlx::query_as(
            r#"SELECT latency_seconds, business_hours_latency_seconds
               FROM reply_pairs
               WHERE direction = ?1
                 AND (?2 IS NULL OR account_id = ?2)
                 AND (?3 IS NULL OR counterparty_email = ?3)
                 AND (?4 IS NULL OR replied_at >= ?4)"#,
        )
        .bind(direction_str)
        .bind(account_filter)
        .bind(counterparty_filter)
        .bind(since_cutoff)
        .fetch_all(self.reader())
        .await?;
        trace_query("analytics.response_time", started_at, rows.len());

        let mut clock: Vec<i64> = rows.iter().map(|(c, _)| *c).collect();
        let mut business: Vec<i64> = rows.iter().filter_map(|(_, b)| *b).collect();
        clock.sort_unstable();
        business.sort_unstable();
        Ok(ResponseTimeSummary {
            direction,
            sample_count: rows.len() as u32,
            clock_p50_seconds: percentile(&clock, 0.5),
            clock_p90_seconds: percentile(&clock, 0.9),
            business_hours_p50_seconds: if business.is_empty() {
                None
            } else {
                Some(percentile(&business, 0.5))
            },
            business_hours_p90_seconds: if business.is_empty() {
                None
            } else {
                Some(percentile(&business, 0.9))
            },
        })
    }

    /// List threads whose latest message direction matches `perspective` and
    /// whose latest message landed before `cutoff_unix`. Used by `mxr stale`.
    /// Older-stale-first ordering; counterparty email is the latest message's
    /// from_email (when inbound) or first to_addrs entry (when outbound).
    pub async fn list_stale_threads(
        &self,
        account_id: Option<&AccountId>,
        perspective: StaleBallInCourt,
        older_than_unix: i64,
        within_unix: i64,
        limit: u32,
    ) -> Result<Vec<StaleThreadRow>, sqlx::Error> {
        let started_at = Instant::now();
        let lim = limit as i64;
        let account_filter: Option<String> = account_id.map(|a| a.as_str());
        let direction_str = perspective.as_db_str();
        // Hard floor: drop messages with bogus Date: headers that fell back
        // to epoch 0 at parse time. Without this, a 1970 spam message ranks
        // as "the most stale thread ever" forever.
        let date_floor = within_unix.max(EARLIEST_PLAUSIBLE_TS);

        let sql = "WITH thread_latest AS (
                SELECT
                    id,
                    thread_id,
                    subject,
                    direction,
                    date,
                    from_email,
                    to_addrs,
                    ROW_NUMBER() OVER (
                        PARTITION BY thread_id
                        ORDER BY date DESC, id DESC
                    ) AS rn
                FROM messages
                WHERE (?1 IS NULL OR account_id = ?1)
                  AND date >= ?5
            )
            SELECT id, thread_id, subject, date, from_email, to_addrs
            FROM thread_latest
            WHERE rn = 1
              AND direction = ?2
              AND date < ?3
            ORDER BY date DESC
            LIMIT ?4";

        let rows: Vec<(String, String, String, i64, String, String)> = sqlx::query_as(sql)
            .bind(account_filter)
            .bind(direction_str)
            .bind(older_than_unix)
            .bind(lim)
            .bind(date_floor)
            .fetch_all(self.reader())
            .await?;
        trace_query("analytics.list_stale_threads", started_at, rows.len());

        let now = chrono::Utc::now().timestamp();
        rows.into_iter()
            .map(|(id, thread_id, subject, date, from_email, to_addrs)| {
                let counterparty = match perspective {
                    StaleBallInCourt::Mine => from_email,
                    StaleBallInCourt::Theirs => {
                        super::reply_pairs::first_recipient_email(&to_addrs).unwrap_or_default()
                    }
                };
                let days_stale = ((now - date).max(0) / 86_400) as u32;
                Ok(StaleThreadRow {
                    thread_id: decode_id(&thread_id)?,
                    latest_message_id: decode_id(&id)?,
                    latest_subject: subject,
                    counterparty_email: counterparty,
                    latest_date: decode_timestamp(date)?,
                    days_stale,
                })
            })
            .collect()
    }
    /// Roll up disk consumption by a chosen dimension. Returns at most `limit`
    /// rows ordered by `bytes DESC, count DESC`.
    ///
    /// Semantics by `group_by`:
    /// - `Mimetype`: groups `attachments.size_bytes` by `attachments.mime_type`.
    /// - `Sender`:   groups `messages.size_bytes` by `messages.from_email`.
    /// - `Label`:    groups `messages.size_bytes` by joined label name; messages
    ///   with no labels are omitted.
    pub async fn storage_breakdown(
        &self,
        account_id: Option<&AccountId>,
        group_by: StorageGroupBy,
        limit: u32,
    ) -> Result<Vec<StorageBucket>, sqlx::Error> {
        let started_at = Instant::now();
        let account_filter: Option<String> = account_id.map(|a| a.as_str());
        let lim = limit as i64;

        let sql = match group_by {
            StorageGroupBy::Mimetype => {
                "SELECT
                     a.mime_type AS key,
                     COALESCE(SUM(a.size_bytes), 0) AS bytes,
                     COUNT(*) AS count
                 FROM attachments a
                 JOIN messages m ON a.message_id = m.id
                 WHERE (?1 IS NULL OR m.account_id = ?1)
                 GROUP BY a.mime_type
                 ORDER BY bytes DESC, count DESC
                 LIMIT ?2"
            }
            StorageGroupBy::Sender => {
                "SELECT
                     m.from_email AS key,
                     COALESCE(SUM(m.size_bytes), 0) AS bytes,
                     COUNT(*) AS count
                 FROM messages m
                 WHERE (?1 IS NULL OR m.account_id = ?1)
                   AND m.from_email != ''
                 GROUP BY m.from_email
                 ORDER BY bytes DESC, count DESC
                 LIMIT ?2"
            }
            StorageGroupBy::Label => {
                "SELECT
                     l.name AS key,
                     COALESCE(SUM(m.size_bytes), 0) AS bytes,
                     COUNT(*) AS count
                 FROM messages m
                 JOIN message_labels ml ON ml.message_id = m.id
                 JOIN labels l ON l.id = ml.label_id
                 WHERE (?1 IS NULL OR m.account_id = ?1)
                 GROUP BY l.id
                 ORDER BY bytes DESC, count DESC
                 LIMIT ?2"
            }
        };

        let rows: Vec<(String, i64, i64)> = sqlx::query_as(sql)
            .bind(account_filter)
            .bind(lim)
            .fetch_all(self.reader())
            .await?;
        trace_query("analytics.storage_breakdown", started_at, rows.len());

        Ok(rows
            .into_iter()
            .map(|(key, bytes, count)| StorageBucket {
                key,
                bytes: bytes.max(0) as u64,
                count: count.max(0) as u32,
            })
            .collect())
    }
}
