use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::FromRow;
use std::time::Instant;

/// Intermediate row shape for the read half of `refresh_contacts`.
/// Mirrors the columns of the inner SELECT so the chunked writer phase
/// can rebind them one row at a time.
#[derive(Debug, Clone, FromRow)]
struct ContactAggregateRow {
    account_id: String,
    email: String,
    display_name: Option<String>,
    first_seen_at: i64,
    last_seen_at: i64,
    last_inbound_at: Option<i64>,
    last_outbound_at: Option<i64>,
    total_inbound: i64,
    total_outbound: i64,
    replied_count: i64,
    cadence_days_p50: Option<f64>,
    is_list_sender: i64,
    list_id: Option<String>,
}

/// Same floor as analytics::EARLIEST_PLAUSIBLE_TS — duplicated rather than
/// pub(crate)'d to keep modules independent. 2000-01-01 UTC.
const EARLIEST_PLAUSIBLE_TS: i64 = 946_684_800;

impl super::Store {
    /// Full refresh of the `contacts` materialized table.
    ///
    /// The aggregation runs against the **reader pool** (no writer lock
    /// held during the expensive scan over `messages` and
    /// `reply_pairs`), then the UPSERT into `contacts` runs in
    /// **bounded chunks** on the writer with `yield_now()` between
    /// chunks so other writers (sync, mutations, snooze wake, activity
    /// recorder, reply-pair reconciler) interleave instead of queueing
    /// behind a single multi-second `INSERT OR REPLACE`.
    ///
    /// Tradeoff: the table is briefly half-updated between chunks.
    /// Acceptable — this is a materialized cache, the SELECT helpers
    /// just see a mix of fresh and stale rows for a fraction of a
    /// second.
    pub async fn refresh_contacts(&self) -> Result<u32, sqlx::Error> {
        const CHUNK_SIZE: usize = 500;

        let started_at = Instant::now();
        let now_unix = chrono::Utc::now().timestamp();

        // Phase 1: aggregate on the reader pool. The single statement
        // below is the inner SELECT from the prior implementation,
        // promoted to a top-level query. Returns one row per (account,
        // email).
        let aggregated: Vec<ContactAggregateRow> = sqlx::query_as(
            r#"WITH contact_events AS (
                SELECT
                    messages.account_id,
                    messages.id AS message_id,
                    LOWER(from_email) AS email,
                    from_name AS display_name,
                    date,
                    direction,
                    list_id
                FROM messages
                WHERE direction = 'inbound' AND from_email != ''

                UNION

                SELECT
                    messages.account_id,
                    messages.id AS message_id,
                    LOWER(json_extract(value, '$.email')) AS email,
                    NULL AS display_name,
                    date,
                    direction,
                    list_id
                FROM messages, json_each(messages.to_addrs)
                WHERE direction = 'outbound'

                UNION

                SELECT
                    messages.account_id,
                    messages.id AS message_id,
                    LOWER(json_extract(value, '$.email')) AS email,
                    NULL AS display_name,
                    date,
                    direction,
                    list_id
                FROM messages, json_each(messages.cc_addrs)
                WHERE direction = 'outbound'

                UNION

                SELECT
                    messages.account_id,
                    messages.id AS message_id,
                    LOWER(json_extract(value, '$.email')) AS email,
                    NULL AS display_name,
                    date,
                    direction,
                    list_id
                FROM messages, json_each(messages.bcc_addrs)
                WHERE direction = 'outbound'
            ),
            contact_base AS (
                SELECT
                    account_id,
                    email,
                    MAX(display_name) AS display_name,
                    MIN(date) AS first_seen_at,
                    MAX(date) AS last_seen_at,
                    MAX(CASE WHEN direction = 'inbound' THEN date END) AS last_inbound_at,
                    MAX(CASE WHEN direction = 'outbound' THEN date END) AS last_outbound_at,
                    SUM(CASE WHEN direction = 'inbound' THEN 1 ELSE 0 END) AS total_inbound,
                    SUM(CASE WHEN direction = 'outbound' THEN 1 ELSE 0 END) AS total_outbound,
                    CASE WHEN MAX(list_id IS NOT NULL) > 0 THEN 1 ELSE 0 END AS is_list_sender,
                    MAX(list_id) AS list_id
                FROM contact_events
                WHERE email IS NOT NULL AND email != ''
                GROUP BY account_id, email
            ),
            reply_stats AS (
                SELECT
                    account_id,
                    LOWER(counterparty_email) AS email,
                    COUNT(*) AS replied_count
                FROM reply_pairs
                WHERE direction = 'i_replied'
                  AND counterparty_email != ''
                GROUP BY account_id, LOWER(counterparty_email)
            ),
            ranked_latencies AS (
                SELECT
                    account_id,
                    LOWER(counterparty_email) AS email,
                    latency_seconds,
                    ROW_NUMBER() OVER (
                        PARTITION BY account_id, LOWER(counterparty_email)
                        ORDER BY latency_seconds
                    ) AS row_num,
                    COUNT(*) OVER (
                        PARTITION BY account_id, LOWER(counterparty_email)
                    ) AS row_count
                FROM reply_pairs
                WHERE direction = 'i_replied'
                  AND counterparty_email != ''
            ),
            cadence_stats AS (
                SELECT
                    account_id,
                    email,
                    AVG(CAST(latency_seconds AS REAL)) / 86400.0 AS cadence_days_p50
                FROM ranked_latencies
                WHERE row_num IN ((row_count + 1) / 2, (row_count + 2) / 2)
                GROUP BY account_id, email
            )
            SELECT
                base.account_id AS account_id,
                base.email AS email,
                base.display_name AS display_name,
                base.first_seen_at AS first_seen_at,
                base.last_seen_at AS last_seen_at,
                base.last_inbound_at AS last_inbound_at,
                base.last_outbound_at AS last_outbound_at,
                base.total_inbound AS total_inbound,
                base.total_outbound AS total_outbound,
                COALESCE(replies.replied_count, 0) AS replied_count,
                cadence.cadence_days_p50 AS cadence_days_p50,
                base.is_list_sender AS is_list_sender,
                base.list_id AS list_id
            FROM contact_base base
            LEFT JOIN reply_stats replies
              ON replies.account_id = base.account_id
             AND replies.email = base.email
            LEFT JOIN cadence_stats cadence
              ON cadence.account_id = base.account_id
             AND cadence.email = base.email"#,
        )
        .fetch_all(self.reader())
        .await?;

        // Phase 2: chunked UPSERT on the writer. Each chunk is its own
        // transaction; `yield_now()` between chunks lets other writers
        // grab the lock instead of starving behind one giant write.
        let mut affected: u32 = 0;
        for chunk in aggregated.chunks(CHUNK_SIZE) {
            let mut tx = self.writer().begin().await?;
            for row in chunk {
                sqlx::query(
                    r#"INSERT OR REPLACE INTO contacts (
                        account_id, email, display_name,
                        first_seen_at, last_seen_at,
                        last_inbound_at, last_outbound_at,
                        total_inbound, total_outbound,
                        replied_count, cadence_days_p50,
                        is_list_sender, list_id,
                        refreshed_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                )
                .bind(&row.account_id)
                .bind(&row.email)
                .bind(&row.display_name)
                .bind(row.first_seen_at)
                .bind(row.last_seen_at)
                .bind(row.last_inbound_at)
                .bind(row.last_outbound_at)
                .bind(row.total_inbound)
                .bind(row.total_outbound)
                .bind(row.replied_count)
                .bind(row.cadence_days_p50)
                .bind(row.is_list_sender)
                .bind(&row.list_id)
                .bind(now_unix)
                .execute(&mut *tx)
                .await?;
                affected = affected.saturating_add(1);
            }
            tx.commit().await?;
            tokio::task::yield_now().await;
        }
        trace_query("contacts.refresh", started_at, affected as usize);
        Ok(affected)
    }

    pub async fn list_contact_asymmetry(
        &self,
        account_id: Option<&AccountId>,
        min_inbound: u32,
        limit: u32,
    ) -> Result<Vec<ContactAsymmetryRow>, sqlx::Error> {
        let started_at = Instant::now();
        let lim = limit as i64;
        let min_in = min_inbound as i64;
        let account_filter: Option<String> = account_id.map(mxr_core::AccountId::as_str);

        let rows: Vec<(String, Option<String>, i64, i64, i64)> = sqlx::query_as(
            r#"SELECT
                email,
                display_name,
                total_inbound,
                total_outbound,
                last_seen_at
              FROM contacts
              WHERE (?1 IS NULL OR account_id = ?1)
                AND total_inbound >= ?2
                AND NOT EXISTS (
                    SELECT 1
                    FROM account_addresses self_addr
                    WHERE LOWER(self_addr.email) = LOWER(contacts.email)
                )
              ORDER BY
                CAST(ABS(total_inbound - total_outbound) AS REAL)
                  / CAST(MAX(total_inbound, total_outbound, 1) AS REAL) DESC,
                total_inbound + total_outbound DESC
              LIMIT ?3"#,
        )
        .bind(account_filter)
        .bind(min_in)
        .bind(lim)
        .fetch_all(self.reader())
        .await?;
        trace_query("contacts.list_asymmetry", started_at, rows.len());

        rows.into_iter()
            .map(
                |(email, display_name, total_inbound, total_outbound, last_seen_at)| {
                    let inbound = total_inbound.max(0) as u32;
                    let outbound = total_outbound.max(0) as u32;
                    let denom = inbound.max(outbound).max(1) as f64;
                    let asymmetry = (inbound as f64 - outbound as f64).abs() / denom;
                    Ok(ContactAsymmetryRow {
                        email,
                        display_name,
                        total_inbound: inbound,
                        total_outbound: outbound,
                        asymmetry,
                        last_seen_at: decode_timestamp(last_seen_at)?,
                    })
                },
            )
            .collect()
    }

    pub async fn list_contact_decay(
        &self,
        account_id: Option<&AccountId>,
        threshold_days: u32,
        max_lookback_days: u32,
        limit: u32,
    ) -> Result<Vec<ContactDecayRow>, sqlx::Error> {
        let now_unix = chrono::Utc::now().timestamp();

        self.list_contact_decay_at(
            account_id,
            threshold_days,
            max_lookback_days,
            limit,
            now_unix,
        )
        .await
    }

    pub(crate) async fn list_contact_decay_at(
        &self,
        account_id: Option<&AccountId>,
        threshold_days: u32,
        max_lookback_days: u32,
        limit: u32,
        now_unix: i64,
    ) -> Result<Vec<ContactDecayRow>, sqlx::Error> {
        let started_at = Instant::now();
        let lim = limit as i64;
        let cutoff = now_unix - i64::from(threshold_days) * 86_400;
        let max_lookback_unix = now_unix - i64::from(max_lookback_days) * 86_400;
        // Floor at year 2000: messages with epoch-0 Date headers would otherwise
        // dominate the result. Use whichever is later: the user-asked window or
        // the hard floor.
        let lookback_floor = max_lookback_unix.max(EARLIEST_PLAUSIBLE_TS);
        let account_filter: Option<String> = account_id.map(mxr_core::AccountId::as_str);

        // Decay = inbound is more recent than outbound by more than threshold.
        // Excludes the boundary (>, not >=) per Slice 13's spec.
        // Hard floor on last_inbound_at filters out epoch-0 garbage.
        let rows: Vec<(String, Option<String>, i64, Option<i64>)> = sqlx::query_as(
            r#"SELECT
                email,
                display_name,
                last_inbound_at,
                last_outbound_at
              FROM contacts
              WHERE (?1 IS NULL OR account_id = ?1)
                AND last_inbound_at IS NOT NULL
                AND last_inbound_at < ?2
                AND last_inbound_at >= ?4
                AND NOT EXISTS (
                    SELECT 1
                    FROM account_addresses self_addr
                    WHERE LOWER(self_addr.email) = LOWER(contacts.email)
                )
                AND (
                    last_outbound_at IS NULL
                    OR last_outbound_at < last_inbound_at
                )
              ORDER BY last_inbound_at DESC
              LIMIT ?3"#,
        )
        .bind(account_filter)
        .bind(cutoff)
        .bind(lim)
        .bind(lookback_floor)
        .fetch_all(self.reader())
        .await?;
        trace_query("contacts.list_decay", started_at, rows.len());

        rows.into_iter()
            .map(|(email, display_name, last_inbound_at, last_outbound_at)| {
                let last_inbound = decode_timestamp(last_inbound_at)?;
                let last_outbound = decode_optional_timestamp(last_outbound_at)?;
                let days_since_inbound = ((now_unix - last_inbound_at).max(0) / 86_400) as u32;
                let days_since_outbound =
                    last_outbound_at.map(|t| ((now_unix - t).max(0) / 86_400) as u32);
                Ok(ContactDecayRow {
                    email,
                    display_name,
                    last_inbound_at: last_inbound,
                    last_outbound_at: last_outbound,
                    days_since_inbound,
                    days_since_outbound,
                })
            })
            .collect()
    }

    /// Upsert a single `contacts` row. Production refreshes the table
    /// in bulk via `refresh_contacts`; this helper exists for callers
    /// (and tests) that need to materialize a known contact directly.
    pub async fn upsert_contact(&self, row: &ContactRow) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO contacts (
                account_id, email, display_name,
                first_seen_at, last_seen_at,
                last_inbound_at, last_outbound_at,
                total_inbound, total_outbound,
                replied_count, cadence_days_p50,
                is_list_sender, list_id,
                refreshed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, ?)
            ON CONFLICT(account_id, email) DO UPDATE SET
              display_name = excluded.display_name,
              first_seen_at = excluded.first_seen_at,
              last_seen_at = excluded.last_seen_at,
              last_inbound_at = excluded.last_inbound_at,
              last_outbound_at = excluded.last_outbound_at,
              total_inbound = excluded.total_inbound,
              total_outbound = excluded.total_outbound,
              replied_count = excluded.replied_count,
              cadence_days_p50 = excluded.cadence_days_p50,
              refreshed_at = excluded.refreshed_at"#,
        )
        .bind(row.account_id.as_str())
        .bind(&row.email)
        .bind(&row.display_name)
        .bind(row.first_seen_at.timestamp())
        .bind(row.last_seen_at.timestamp())
        .bind(row.last_inbound_at.map(|d| d.timestamp()))
        .bind(row.last_outbound_at.map(|d| d.timestamp()))
        .bind(row.total_inbound as i64)
        .bind(row.total_outbound as i64)
        .bind(row.replied_count as i64)
        .bind(row.cadence_days_p50)
        .bind(chrono::Utc::now().timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Return the top-N contacts for an account, ranked by combined
    /// inbound+outbound volume (descending). Used by the pre-send
    /// safety pipeline to drive typo-distance and first-time-external
    /// checks. Excludes the account's own addresses.
    pub async fn list_known_contacts(
        &self,
        account_id: &AccountId,
        limit: u32,
    ) -> Result<Vec<ContactRow>, sqlx::Error> {
        let started_at = Instant::now();
        let lim = limit as i64;
        let rows = sqlx::query(
            r#"SELECT
                account_id, email, display_name,
                first_seen_at, last_seen_at,
                last_inbound_at, last_outbound_at,
                total_inbound, total_outbound,
                replied_count, cadence_days_p50
              FROM contacts
              WHERE account_id = ?1
                AND NOT EXISTS (
                    SELECT 1
                    FROM account_addresses self_addr
                    WHERE LOWER(self_addr.email) = LOWER(contacts.email)
                )
              ORDER BY (total_inbound + total_outbound) DESC
              LIMIT ?2"#,
        )
        .bind(account_id.as_str())
        .bind(lim)
        .fetch_all(self.reader())
        .await?;
        trace_query("contacts.list_known", started_at, rows.len());

        use sqlx::Row;
        rows.into_iter()
            .map(|row| {
                Ok(ContactRow {
                    account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
                    email: row.get("email"),
                    display_name: row.get("display_name"),
                    first_seen_at: decode_timestamp(row.get::<i64, _>("first_seen_at"))?,
                    last_seen_at: decode_timestamp(row.get::<i64, _>("last_seen_at"))?,
                    last_inbound_at: decode_optional_timestamp(row.get("last_inbound_at"))?,
                    last_outbound_at: decode_optional_timestamp(row.get("last_outbound_at"))?,
                    total_inbound: row.get::<i64, _>("total_inbound").max(0) as u32,
                    total_outbound: row.get::<i64, _>("total_outbound").max(0) as u32,
                    replied_count: row.get::<i64, _>("replied_count").max(0) as u32,
                    cadence_days_p50: row.get("cadence_days_p50"),
                })
            })
            .collect()
    }
}

#[allow(dead_code)]
fn _types_used(_: AccountId, _: ContactRow) {
    // Keeps `decode_id` re-exported even when query helpers don't pull it.
    let _ = decode_id::<AccountId>;
}
