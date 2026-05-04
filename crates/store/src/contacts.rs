use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use std::time::Instant;

impl super::Store {
    /// Full-table refresh of the `contacts` materialized table. Cheap on small
    /// mailboxes; the plan calls for an incremental fallback past ~100k
    /// messages. Aggregates per (account_id, email) over `messages`.
    pub async fn refresh_contacts(&self) -> Result<u32, sqlx::Error> {
        let started_at = Instant::now();
        let now_unix = chrono::Utc::now().timestamp();
        // SQLite supports CTE + INSERT OR REPLACE without a transaction
        // wrapper for a single-statement upsert.
        let result = sqlx::query(
            r#"INSERT OR REPLACE INTO contacts (
                account_id, email, display_name,
                first_seen_at, last_seen_at,
                last_inbound_at, last_outbound_at,
                total_inbound, total_outbound,
                replied_count, cadence_days_p50,
                is_list_sender, list_id,
                refreshed_at
            )
            SELECT
                account_id,
                email,
                display_name,
                MIN(date) AS first_seen_at,
                MAX(date) AS last_seen_at,
                MAX(CASE WHEN direction = 'inbound' THEN date END) AS last_inbound_at,
                MAX(CASE WHEN direction = 'outbound' THEN date END) AS last_outbound_at,
                SUM(CASE WHEN direction = 'inbound' THEN 1 ELSE 0 END) AS total_inbound,
                SUM(CASE WHEN direction = 'outbound' THEN 1 ELSE 0 END) AS total_outbound,
                0 AS replied_count,
                NULL AS cadence_days_p50,
                CASE WHEN MAX(list_id IS NOT NULL) > 0 THEN 1 ELSE 0 END AS is_list_sender,
                MAX(list_id) AS list_id,
                ?1 AS refreshed_at
            FROM (
                SELECT
                    account_id,
                    LOWER(from_email) AS email,
                    MAX(from_name)    AS display_name,
                    date,
                    direction,
                    list_id
                FROM messages
                WHERE direction = 'inbound' AND from_email != ''
                GROUP BY account_id, LOWER(from_email), date, direction, list_id

                UNION ALL

                SELECT
                    account_id,
                    LOWER(json_extract(value, '$.email')) AS email,
                    NULL AS display_name,
                    date,
                    direction,
                    list_id
                FROM messages, json_each(messages.to_addrs)
                WHERE direction = 'outbound'
            )
            WHERE email IS NOT NULL AND email != ''
            GROUP BY account_id, email"#,
        )
        .bind(now_unix)
        .execute(self.writer())
        .await?;
        trace_query(
            "contacts.refresh",
            started_at,
            result.rows_affected() as usize,
        );
        Ok(result.rows_affected() as u32)
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
        let account_filter: Option<String> = account_id.map(|a| a.as_str());

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
            .map(|(email, display_name, total_inbound, total_outbound, last_seen_at)| {
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
            })
            .collect()
    }

    pub async fn list_contact_decay(
        &self,
        account_id: Option<&AccountId>,
        threshold_days: u32,
        limit: u32,
    ) -> Result<Vec<ContactDecayRow>, sqlx::Error> {
        let started_at = Instant::now();
        let lim = limit as i64;
        let now_unix = chrono::Utc::now().timestamp();
        let cutoff = now_unix - i64::from(threshold_days) * 86_400;
        let account_filter: Option<String> = account_id.map(|a| a.as_str());

        // Decay = inbound is more recent than outbound by more than threshold.
        // Excludes the boundary (>, not >=) per Slice 13's spec.
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
                AND (
                    last_outbound_at IS NULL
                    OR last_outbound_at < last_inbound_at
                )
              ORDER BY last_inbound_at ASC
              LIMIT ?3"#,
        )
        .bind(account_filter)
        .bind(cutoff)
        .bind(lim)
        .fetch_all(self.reader())
        .await?;
        trace_query("contacts.list_decay", started_at, rows.len());

        rows.into_iter()
            .map(|(email, display_name, last_inbound_at, last_outbound_at)| {
                let last_inbound = decode_timestamp(last_inbound_at)?;
                let last_outbound = decode_optional_timestamp(last_outbound_at)?;
                let days_since_inbound = ((now_unix - last_inbound_at).max(0) / 86_400) as u32;
                let days_since_outbound = last_outbound_at
                    .map(|t| ((now_unix - t).max(0) / 86_400) as u32);
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
}

#[allow(dead_code)]
fn _types_used(_: AccountId, _: ContactRow) {
    // Keeps `decode_id` re-exported even when query helpers don't pull it.
    let _ = decode_id::<AccountId>;
}
