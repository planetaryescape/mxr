use crate::{decode_id, decode_timestamp, trace_query, NON_SELF_ADDRESSED_MESSAGE_PREDICATE};
use mxr_core::id::*;
use mxr_core::types::*;
use std::time::Instant;

/// Earliest plausible Unix timestamp for an email date — 2000-01-01 UTC.
/// Messages with a `date` before this almost always have a corrupt or
/// unparseable Date: header that fell back to epoch 0 at parse time, and
/// they pollute time-based analytics. Hardcoded floor; nothing legitimate
/// in this codebase ever lands earlier.
const EARLIEST_PLAUSIBLE_TS: i64 = 946_684_800;

/// How many stored messages of one account an owned-set change flipped.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AccountDirectionChanges {
    /// Inbound rows whose sender is owned now.
    pub to_outbound: u32,
    /// Outbound rows whose sender is no longer owned.
    pub to_inbound: u32,
}

/// The rows an owned set changes: rows whose stored direction disagrees with
/// `MessageDirection::for_message`. Unknown rows belong to
/// `reclassify_unknown_directions`. Rows are `(id, from_email, direction, flags)`.
/// A provider-sent row is never demoted, whatever its From.
fn direction_flips(
    rows: &[(String, String, String, i64)],
    owned: &OwnedAddresses,
) -> Vec<(String, MessageDirection)> {
    rows.iter()
        .filter_map(|(id, from_email, direction, flags)| {
            let flags = MessageFlags::from_bits_truncate(*flags as u32);
            let target = MessageDirection::for_message(owned.contains(from_email), flags);
            match (direction.as_str(), target) {
                ("inbound", MessageDirection::Outbound)
                | ("outbound", MessageDirection::Inbound) => Some((id.clone(), target)),
                _ => None,
            }
        })
        .collect()
}

/// The rows of one account whose direction is known, as `(id, from_email,
/// direction, flags)`. Both the preview and the real change read them here.
async fn account_direction_rows<'e, E>(
    executor: E,
    account_id: &AccountId,
) -> Result<Vec<(String, String, String, i64)>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_as(
        r#"SELECT id, from_email, direction, flags
           FROM messages
           WHERE account_id = ? AND direction != 'unknown'"#,
    )
    .bind(account_id.as_str())
    .fetch_all(executor)
    .await
}

impl AccountDirectionChanges {
    fn from_flips(flips: &[(String, MessageDirection)]) -> Self {
        let mut changes = Self::default();
        for (_, direction) in flips {
            match direction {
                MessageDirection::Outbound => changes.to_outbound += 1,
                _ => changes.to_inbound += 1,
            }
        }
        changes
    }
}

impl super::Store {
    /// Reclassify one account's stored messages against `owned`, the set the
    /// change produces. With `apply: false` it runs the same selection and
    /// returns the counts a real run would apply, and writes nothing. Its
    /// callers are the address commands' dry run and the real change.
    ///
    /// With `apply: true`, inbound rows from an owned sender or filed as sent
    /// become outbound, and outbound rows from an unowned sender that the
    /// provider did not file as sent become inbound. Their reply pairs move
    /// with them. It all runs in one writer transaction, so sync cannot write between the
    /// read and the flip. Arrivals are not touched: they record what arrived
    /// when it arrived, and their corrections stay as they were.
    pub async fn reclassify_account_directions(
        &self,
        account_id: &AccountId,
        owned: &OwnedAddresses,
        apply: bool,
    ) -> Result<AccountDirectionChanges, sqlx::Error> {
        if !apply {
            let rows = account_direction_rows(self.reader(), account_id).await?;
            let flips = direction_flips(&rows, owned);
            return Ok(AccountDirectionChanges::from_flips(&flips));
        }

        let started_at = Instant::now();
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let rows = account_direction_rows(&mut *tx, account_id).await?;
        let flips = direction_flips(&rows, owned);
        for (id, direction) in &flips {
            sqlx::query("UPDATE messages SET direction = ? WHERE id = ?")
                .bind(direction.as_db_str())
                .bind(id)
                .execute(&mut *tx)
                .await?;
            Self::reseat_reply_pair_tx(&mut tx, id).await?;
        }
        // Pairs a late writer rebuilt from an old direction are not in `flips`.
        // Rebuild every mismatched pair of the account here as well.
        Self::reseat_mismatched_pairs_tx(&mut tx, &account_id.as_str()).await?;
        tx.commit().await?;
        let changes = AccountDirectionChanges::from_flips(&flips);
        trace_query(
            "analytics.reclassify_account_directions",
            started_at,
            (changes.to_outbound + changes.to_inbound) as usize,
        );
        Ok(changes)
    }
}

impl super::Store {
    pub async fn reclassify_unknown_directions(
        &self,
        is_account_address: impl Fn(&AccountId, &str) -> bool,
    ) -> Result<u32, sqlx::Error> {
        let started_at = Instant::now();
        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            r#"SELECT id, account_id, from_email, flags
               FROM messages
               WHERE direction = 'unknown'"#,
        )
        .fetch_all(self.reader())
        .await?;
        let mut updated = 0u32;
        for (id, account_id, from_email, flags) in rows {
            let account_id: AccountId = decode_id(&account_id)?;
            let flags = MessageFlags::from_bits_truncate(flags as u32);
            let new_direction =
                MessageDirection::for_message(is_account_address(&account_id, &from_email), flags);
            sqlx::query!(
                "UPDATE messages SET direction = ? WHERE id = ?",
                new_direction.as_db_str(),
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

    /// Normalize cached List-Id and its index together. Bounded, retry-safe
    /// metadata-only writes preserve unknown metadata fields and all caches.
    /// Historical startup/explicit doctor repair, not an incremental sync step:
    /// current body ingestion canonicalizes new data in `insert_body_tx`.
    pub async fn backfill_message_list_ids(&self) -> Result<u32, sqlx::Error> {
        let started_at = Instant::now();
        let mut cursor = String::new();
        let mut n = 0u32;
        loop {
            // Take the writer transaction before reading: sync cannot replace
            // the cached body between observation and the two related writes.
            let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
            let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT b.message_id, json_object('list_id', json_extract(b.metadata_json, '$.list_id'),
                    'raw_headers', substr(json_extract(b.metadata_json, '$.raw_headers'), 1, 131072)),
                    m.list_id FROM bodies b
                 JOIN messages m ON m.id = b.message_id WHERE b.message_id > ?
                 ORDER BY b.message_id LIMIT 400",
            )
            .bind(&cursor)
            .fetch_all(&mut *tx)
            .await?;
            if rows.is_empty() {
                tx.commit().await?;
                break;
            }
            for (id, json, indexed) in rows {
                cursor.clone_from(&id);
                let metadata: MessageMetadata = crate::decode_json(&json)?;
                let evidence = mxr_core::mail_evidence::MailEvidence::cached(&metadata, None);
                let Some(list_id) = evidence.list_id else {
                    continue;
                };
                if metadata.list_id.as_ref() == Some(&list_id) && indexed.as_ref() == Some(&list_id)
                {
                    continue;
                }
                sqlx::query("UPDATE bodies SET metadata_json = json_set(metadata_json, '$.list_id', ?) WHERE message_id = ?")
                    .bind(&list_id).bind(&id).execute(&mut *tx).await?;
                sqlx::query("UPDATE messages SET list_id = ? WHERE id = ?")
                    .bind(&list_id)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                n = n.saturating_add(1);
            }
            tx.commit().await?;
        }
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
        let account_filter: Option<String> = account_id.map(mxr_core::AccountId::as_str);
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

/// Bucket clock latencies into the fixed histogram edges defined in
/// `RESPONSE_TIME_HISTOGRAM_EDGES`. Negative latencies are skipped
/// (shouldn't occur, but guard against bad data). Single linear pass.
fn build_response_time_histogram(clock_seconds: &[i64]) -> Vec<ResponseTimeBucket> {
    let mut counts = [0u32; RESPONSE_TIME_HISTOGRAM_EDGES.len()];
    for &latency in clock_seconds {
        if latency < 0 {
            continue;
        }
        let latency = u32::try_from(latency).unwrap_or(u32::MAX);
        for (i, edge) in RESPONSE_TIME_HISTOGRAM_EDGES.iter().enumerate() {
            if latency < *edge {
                counts[i] += 1;
                break;
            }
        }
    }
    RESPONSE_TIME_HISTOGRAM_EDGES
        .iter()
        .zip(counts.iter())
        .map(|(&upper, &count)| ResponseTimeBucket {
            upper_bound_seconds: upper,
            count,
        })
        .collect()
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
        let account_filter: Option<String> = account_id.map(mxr_core::AccountId::as_str);
        let direction_str = direction.as_db_str();
        let counterparty_filter = counterparty.map(str::to_lowercase);
        let since_cutoff: Option<i64> =
            since_days.map(|d| chrono::Utc::now().timestamp() - i64::from(d) * 86_400);

        let rows: Vec<(i64, Option<i64>)> = sqlx::query_as(
            r#"SELECT latency_seconds, business_hours_latency_seconds
               FROM reply_pairs
               WHERE direction = ?1
                 AND (?2 IS NULL OR account_id = ?2)
                 AND (?3 IS NULL OR counterparty_email = ?3)
                 AND (?4 IS NULL OR replied_at >= ?4)
                 AND NOT EXISTS (
                     SELECT 1
                     FROM account_addresses self_addr
                     WHERE LOWER(self_addr.email) = LOWER(counterparty_email)
                 )"#,
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
        let histogram = build_response_time_histogram(&clock);
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
            histogram,
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
        let account_filter: Option<String> = account_id.map(mxr_core::AccountId::as_str);
        let direction_str = perspective.as_db_str();
        // Hard floor: drop messages with bogus Date: headers that fell back
        // to epoch 0 at parse time. Without this, a 1970 spam message ranks
        // as "the most stale thread ever" forever.
        let date_floor = within_unix.max(EARLIEST_PLAUSIBLE_TS);

        let sql = format!(
            "WITH thread_latest AS (
                SELECT
                    m.id,
                    m.thread_id,
                    m.subject,
                    m.direction,
                    m.date,
                    m.from_email,
                    m.to_addrs,
                    ROW_NUMBER() OVER (
                        PARTITION BY m.thread_id
                        ORDER BY m.date DESC, m.id DESC
                    ) AS rn
                FROM messages m
                WHERE (?1 IS NULL OR m.account_id = ?1)
                  AND m.date >= ?5
                  AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
            )
            SELECT id, thread_id, subject, date, from_email, to_addrs
            FROM thread_latest
            WHERE rn = 1
              AND direction = ?2
              AND date < ?3
            ORDER BY date DESC
            LIMIT ?4"
        );

        let rows: Vec<(String, String, String, i64, String, String)> =
            sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
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
        let account_filter: Option<String> = account_id.map(mxr_core::AccountId::as_str);
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

#[cfg(test)]
mod tests;
