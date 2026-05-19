use crate::{decode_id, decode_timestamp, trace_query, NON_SELF_ADDRESSED_MESSAGE_PREDICATE};
use mxr_core::id::*;
use mxr_core::types::*;
use std::time::Instant;

/// Same floor as analytics::EARLIEST_PLAUSIBLE_TS — 2000-01-01 UTC.
/// Without this, a 1970-stamped phishing message claims "longest thread"
/// or "fastest reply" forever.
const EARLIEST_PLAUSIBLE_TS: i64 = 946_684_800;

/// Slowest-reply cap — 30 days. Replies that took longer almost always mean
/// "I came back to it months later" rather than "I had a slow week", and
/// they make the slowest-reply superlative absurd.
const SLOWEST_REPLY_CAP_SECONDS: i64 = 30 * 86_400;

/// Number of contacts per top-N section.
const TOP_N: i64 = 5;

type CountBucketRow = (i64, i64);
type NamedContactCountRow = (String, Option<String>, i64);
type EmailCountRow = (String, i64);
type ContactAsymmetrySqlRow = (String, Option<String>, i64, i64);
type MimeStorageSqlRow = (String, i64, i64);
type HeaviestMessageSqlRow = (String, String, String, i64, i64);
type TopListSqlRow = (String, i64, i64);

impl super::Store {
    /// Compute a year-in-review summary for the given window. Powers
    /// `mxr wrapped`. Runs ~10 SQL queries against the local store; each
    /// query filters to the same window and respects the
    /// `EARLIEST_PLAUSIBLE_TS` floor so corrupt-date spam doesn't dominate.
    ///
    /// The `label` is a human-readable window name ("2025", "year-to-date",
    /// "last 90 days") used by the CLI render; the store only echoes it
    /// back via `WrappedSummary::label`.
    pub async fn wrapped_summary(
        &self,
        account_id: Option<&AccountId>,
        since_unix: i64,
        until_unix: i64,
        label: &str,
    ) -> Result<WrappedSummary, sqlx::Error> {
        let started_at = Instant::now();
        let acct: Option<String> = account_id.map(|a| a.as_str());
        let floor = since_unix.max(EARLIEST_PLAUSIBLE_TS);

        // The seven sub-aggregates are fully independent — they share no
        // intermediate state and just dispatch their own SQL. Run them
        // concurrently so the reader pool (4 connections) can saturate
        // instead of blocking on a strict serial chain. On a large
        // mailbox (multi-million rows) this is the difference between
        // "minutes" and "seconds" because the bound is read-IO + index
        // walking, not CPU.
        let (
            volume,
            time_patterns,
            top_contacts,
            reply_discipline,
            storage,
            newsletters,
            superlatives,
        ) = tokio::try_join!(
            self.wrapped_volume(&acct, floor, until_unix),
            self.wrapped_time_patterns(&acct, floor, until_unix),
            self.wrapped_top_contacts(&acct, floor, until_unix),
            self.wrapped_reply_discipline(&acct, floor, until_unix),
            self.wrapped_storage(&acct, floor, until_unix),
            self.wrapped_newsletters(&acct, floor, until_unix),
            self.wrapped_superlatives(&acct, floor, until_unix),
        )?;

        trace_query("wrapped.summary", started_at, 1);
        Ok(WrappedSummary {
            window_start: decode_timestamp(floor)?,
            window_end: decode_timestamp(until_unix)?,
            label: label.to_string(),
            volume,
            time_patterns,
            top_contacts,
            reply_discipline,
            storage,
            newsletters,
            superlatives,
        })
    }

    async fn wrapped_volume(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<WrappedVolume, sqlx::Error> {
        let sql = format!(
            r#"SELECT
                COALESCE(SUM(CASE WHEN m.direction = 'inbound' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN m.direction = 'outbound' THEN 1 ELSE 0 END), 0),
                COUNT(DISTINCT m.thread_id)
              FROM messages m
              WHERE (?1 IS NULL OR m.account_id = ?1)
                AND m.date >= ?2
                AND m.date <= ?3
                AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}"#
        );
        let row: (i64, i64, i64) = sqlx::query_as(&sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_one(self.reader())
            .await?;
        Ok(WrappedVolume {
            inbound_count: row.0.max(0) as u32,
            outbound_count: row.1.max(0) as u32,
            thread_count: row.2.max(0) as u32,
        })
    }

    async fn wrapped_time_patterns(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<WrappedTimePatterns, sqlx::Error> {
        // Full DOW distribution (up to 7 rows), full hour distribution
        // (up to 24 rows), and busiest-day all share the same predicate
        // shape; run them concurrently against the reader pool.
        // SQLite's strftime('%w') returns 0=Sunday..6=Saturday; the
        // public `day_of_week_distribution` array uses 0=Monday..6=Sunday,
        // so remap by `(sqlite_dow + 6) % 7`.
        let dow_sql = format!(
            r#"SELECT CAST(strftime('%w', m.date, 'unixepoch') AS INTEGER) AS dow,
                      COUNT(*) AS c
               FROM messages m
               WHERE (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
               GROUP BY dow"#,
        );
        let dow_query = sqlx::query_as::<_, (i64, i64)>(&dow_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_all(self.reader());

        let hour_sql = format!(
            r#"SELECT CAST(strftime('%H', m.date, 'unixepoch') AS INTEGER) AS hr,
                      COUNT(*) AS c
               FROM messages m
               WHERE (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
               GROUP BY hr"#,
        );
        let hour_query = sqlx::query_as::<_, (i64, i64)>(&hour_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_all(self.reader());

        let day_sql = format!(
            r#"SELECT MIN(m.date) AS first_ts, COUNT(*) AS c
               FROM messages m
               WHERE (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
               GROUP BY date(m.date, 'unixepoch')
               ORDER BY c DESC, first_ts ASC
               LIMIT 1"#,
        );
        let day_query = sqlx::query_as::<_, (i64, i64)>(&day_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_optional(self.reader());

        let (dow_rows, hour_rows, day): (
            Vec<CountBucketRow>,
            Vec<CountBucketRow>,
            Option<CountBucketRow>,
        ) = tokio::try_join!(dow_query, hour_query, day_query)?;

        let mut day_of_week_distribution = [0u32; 7];
        let mut busiest_dow_sqlite: Option<i64> = None;
        let mut busiest_dow_count: u32 = 0;
        for (sqlite_dow, count) in &dow_rows {
            let idx = (((*sqlite_dow).rem_euclid(7) + 6) % 7) as usize;
            let count_u32 = (*count).max(0) as u32;
            day_of_week_distribution[idx] = count_u32;
            if count_u32 > busiest_dow_count
                || (count_u32 == busiest_dow_count && busiest_dow_sqlite.is_none())
            {
                busiest_dow_count = count_u32;
                busiest_dow_sqlite = Some(*sqlite_dow);
            }
        }

        let mut hour_distribution = [0u32; 24];
        let mut busiest_hour: Option<u8> = None;
        let mut busiest_hour_count: u32 = 0;
        for (hr, count) in &hour_rows {
            let hr_clamped = (*hr).clamp(0, 23) as usize;
            let count_u32 = (*count).max(0) as u32;
            hour_distribution[hr_clamped] = count_u32;
            if count_u32 > busiest_hour_count
                || (count_u32 == busiest_hour_count && busiest_hour.is_none())
            {
                busiest_hour_count = count_u32;
                busiest_hour = Some(hr_clamped as u8);
            }
        }

        Ok(WrappedTimePatterns {
            busiest_day_of_week: busiest_dow_sqlite.map(|d| dow_name(d).to_string()),
            busiest_day_of_week_count: busiest_dow_count,
            busiest_hour_utc: busiest_hour,
            busiest_hour_count,
            busiest_date: match day {
                Some((ts, _)) => Some(decode_timestamp(ts)?),
                None => None,
            },
            busiest_date_count: day.map(|(_, c)| c.max(0) as u32).unwrap_or(0),
            hour_distribution,
            day_of_week_distribution,
        })
    }

    async fn wrapped_top_contacts(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<WrappedTopContacts, sqlx::Error> {
        // Three independent aggregates — top inbound, top outbound,
        // and most-asymmetric — fan out to the reader pool together.
        let inbound_sql = format!(
            r#"SELECT LOWER(m.from_email), MAX(m.from_name), COUNT(*) AS c
               FROM messages m
               WHERE m.direction = 'inbound'
                 AND m.from_email != ''
                 AND (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
                 AND NOT EXISTS (
                     SELECT 1
                     FROM account_addresses self_addr
                     WHERE LOWER(self_addr.email) = LOWER(m.from_email)
                 )
               GROUP BY LOWER(m.from_email)
               ORDER BY c DESC
               LIMIT ?4"#,
        );
        let inbound_query = sqlx::query_as::<_, (String, Option<String>, i64)>(&inbound_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .bind(TOP_N)
            .fetch_all(self.reader());

        let outbound_query = sqlx::query_as::<_, (String, i64)>(
            r#"SELECT LOWER(json_extract(t.value, '$.email')) AS email,
                      COUNT(*) AS c
               FROM messages m, json_each(m.to_addrs) t
               WHERE m.direction = 'outbound'
                 AND (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND NOT EXISTS (
                     SELECT 1
                     FROM account_addresses self_addr
                     WHERE LOWER(self_addr.email) = LOWER(json_extract(t.value, '$.email'))
                 )
               GROUP BY email
               HAVING email IS NOT NULL AND email != ''
               ORDER BY c DESC
               LIMIT ?4"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .bind(TOP_N)
        .fetch_all(self.reader());

        // Most asymmetric over the window — recompute (the materialized
        // contacts table is all-time, doesn't respect window). Requires
        // ≥5 inbound to filter out one-off senders.
        let asym_sql = format!(
            r#"WITH window_contacts AS (
                SELECT LOWER(m.from_email) AS email, m.from_name AS name,
                       'in' AS dir
                  FROM messages m
                  WHERE m.direction = 'inbound' AND m.from_email != ''
                    AND (?1 IS NULL OR m.account_id = ?1)
                    AND m.date >= ?2 AND m.date <= ?3
                    AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
                    AND NOT EXISTS (
                        SELECT 1
                        FROM account_addresses self_addr
                        WHERE LOWER(self_addr.email) = LOWER(m.from_email)
                    )
                UNION ALL
                SELECT LOWER(json_extract(t.value, '$.email')),
                       json_extract(t.value, '$.name'),
                       'out'
                  FROM messages m, json_each(m.to_addrs) t
                  WHERE m.direction = 'outbound'
                    AND (?1 IS NULL OR m.account_id = ?1)
                    AND m.date >= ?2 AND m.date <= ?3
                    AND NOT EXISTS (
                        SELECT 1
                        FROM account_addresses self_addr
                        WHERE LOWER(self_addr.email) = LOWER(json_extract(t.value, '$.email'))
                    )
              )
              SELECT email, MAX(name),
                     SUM(CASE WHEN dir = 'in' THEN 1 ELSE 0 END) AS in_c,
                     SUM(CASE WHEN dir = 'out' THEN 1 ELSE 0 END) AS out_c
              FROM window_contacts
              WHERE email IS NOT NULL AND email != ''
              GROUP BY email
              HAVING in_c >= 5
              ORDER BY CAST(ABS(in_c - out_c) AS REAL)
                     / CAST(MAX(in_c, out_c, 1) AS REAL) DESC,
                     in_c + out_c DESC
              LIMIT 3"#,
        );
        let asym_query = sqlx::query_as::<_, (String, Option<String>, i64, i64)>(&asym_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_all(self.reader());

        let (inbound, outbound, asym): (
            Vec<NamedContactCountRow>,
            Vec<EmailCountRow>,
            Vec<ContactAsymmetrySqlRow>,
        ) = tokio::try_join!(inbound_query, outbound_query, asym_query)?;

        // last_seen_at on ContactAsymmetryRow needs a value — reuse
        // window_end as a placeholder rather than running another query.
        let placeholder_last_seen = decode_timestamp(until)?;

        Ok(WrappedTopContacts {
            most_emailed_to_me: inbound
                .into_iter()
                .map(|(email, name, count)| WrappedContactRank {
                    email,
                    display_name: name,
                    count: count.max(0) as u32,
                })
                .collect(),
            most_emailed_by_me: outbound
                .into_iter()
                .map(|(email, count)| WrappedContactRank {
                    email,
                    display_name: None,
                    count: count.max(0) as u32,
                })
                .collect(),
            most_asymmetric: asym
                .into_iter()
                .map(|(email, name, in_c, out_c)| {
                    let inbound = in_c.max(0) as u32;
                    let outbound = out_c.max(0) as u32;
                    let denom = inbound.max(outbound).max(1) as f64;
                    let asymmetry = (inbound as f64 - outbound as f64).abs() / denom;
                    ContactAsymmetryRow {
                        email,
                        display_name: name,
                        total_inbound: inbound,
                        total_outbound: outbound,
                        asymmetry,
                        last_seen_at: placeholder_last_seen,
                    }
                })
                .collect(),
        })
    }

    async fn wrapped_reply_discipline(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<Option<WrappedReplyDiscipline>, sqlx::Error> {
        let pairs: Vec<(i64, Option<i64>)> = sqlx::query_as(
            r#"SELECT latency_seconds, business_hours_latency_seconds
               FROM reply_pairs
               WHERE direction = 'i_replied'
                 AND replied_at >= ?2 AND replied_at <= ?3
                 AND (?1 IS NULL OR account_id = ?1)
                 AND NOT EXISTS (
                     SELECT 1
                     FROM account_addresses self_addr
                     WHERE LOWER(self_addr.email) = LOWER(counterparty_email)
                 )"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;

        if pairs.is_empty() {
            return Ok(None);
        }

        let mut clock: Vec<i64> = pairs.iter().map(|(c, _)| *c).collect();
        let mut business: Vec<i64> = pairs.iter().filter_map(|(_, b)| *b).collect();
        clock.sort_unstable();
        business.sort_unstable();

        let fastest: Option<(String, i64, i64)> = sqlx::query_as(
            r#"SELECT counterparty_email, latency_seconds, replied_at
               FROM reply_pairs
               WHERE direction = 'i_replied'
                 AND replied_at >= ?2 AND replied_at <= ?3
                 AND latency_seconds > 0
                 AND (?1 IS NULL OR account_id = ?1)
                 AND NOT EXISTS (
                     SELECT 1
                     FROM account_addresses self_addr
                     WHERE LOWER(self_addr.email) = LOWER(counterparty_email)
                 )
                ORDER BY latency_seconds ASC
                LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_optional(self.reader())
        .await?;

        let slowest: Option<(String, i64, i64)> = sqlx::query_as(
            r#"SELECT counterparty_email, latency_seconds, replied_at
               FROM reply_pairs
               WHERE direction = 'i_replied'
                 AND replied_at >= ?2 AND replied_at <= ?3
                 AND latency_seconds > 0
                 AND latency_seconds <= ?4
                 AND (?1 IS NULL OR account_id = ?1)
                 AND NOT EXISTS (
                     SELECT 1
                     FROM account_addresses self_addr
                     WHERE LOWER(self_addr.email) = LOWER(counterparty_email)
                 )
                ORDER BY latency_seconds DESC
                LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .bind(SLOWEST_REPLY_CAP_SECONDS)
        .fetch_optional(self.reader())
        .await?;

        Ok(Some(WrappedReplyDiscipline {
            sample_count: pairs.len() as u32,
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
            fastest: match fastest {
                Some((email, secs, when)) => Some(WrappedReplyExtreme {
                    counterparty_email: email,
                    latency_seconds: secs.max(0) as u32,
                    replied_at: decode_timestamp(when)?,
                }),
                None => None,
            },
            slowest: match slowest {
                Some((email, secs, when)) => Some(WrappedReplyExtreme {
                    counterparty_email: email,
                    latency_seconds: secs.max(0) as u32,
                    replied_at: decode_timestamp(when)?,
                }),
                None => None,
            },
        }))
    }

    async fn wrapped_storage(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<WrappedStorage, sqlx::Error> {
        let total_query = sqlx::query_as::<_, (i64,)>(
            r#"SELECT COALESCE(SUM(size_bytes), 0)
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_one(self.reader());

        let mime_query = sqlx::query_as::<_, (String, i64, i64)>(
            r#"SELECT a.mime_type,
                      COALESCE(SUM(a.size_bytes), 0),
                      COUNT(*)
               FROM attachments a
               JOIN messages m ON m.id = a.message_id
               WHERE (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
               GROUP BY a.mime_type
               ORDER BY SUM(a.size_bytes) DESC
               LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_optional(self.reader());

        let heaviest_query = sqlx::query_as::<_, (String, String, String, i64, i64)>(
            r#"SELECT id, from_email, subject, size_bytes, date
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
                 AND size_bytes > 0
               ORDER BY size_bytes DESC
               LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_optional(self.reader());

        let (total_row, mime, heaviest): (
            (i64,),
            Option<MimeStorageSqlRow>,
            Option<HeaviestMessageSqlRow>,
        ) = tokio::try_join!(total_query, mime_query, heaviest_query)?;

        Ok(WrappedStorage {
            total_bytes: total_row.0.max(0) as u64,
            top_mimetype: mime.map(|(key, bytes, count)| WrappedStorageBucket {
                key,
                bytes: bytes.max(0) as u64,
                count: count.max(0) as u32,
            }),
            heaviest_message: match heaviest {
                Some((id, from, subj, size, date)) => Some(LargestMessageRow {
                    message_id: decode_id(&id)?,
                    from_email: from,
                    subject: subj,
                    size_bytes: size.max(0) as u64,
                    date: decode_timestamp(date)?,
                }),
                None => None,
            },
        })
    }

    async fn wrapped_newsletters(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<WrappedNewsletters, sqlx::Error> {
        let read_flag = MessageFlags::READ.bits() as i64;

        let unique_sql = format!(
            r#"SELECT COUNT(DISTINCT m.list_id)
               FROM messages m
               WHERE m.list_id IS NOT NULL
                 AND (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}"#
        );
        let unique_query = sqlx::query_as::<_, (i64,)>(&unique_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_one(self.reader());

        let top_list_sql = format!(
            r#"SELECT m.list_id,
                      COUNT(*),
                      SUM(CASE WHEN (m.flags & ?4) = ?4 THEN 1 ELSE 0 END)
               FROM messages m
               WHERE m.list_id IS NOT NULL
                 AND (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
               GROUP BY m.list_id
               ORDER BY COUNT(*) DESC
               LIMIT 1"#,
        );
        let top_list_query = sqlx::query_as::<_, (String, i64, i64)>(&top_list_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .bind(read_flag)
            .fetch_optional(self.reader());

        // Share of inbound that is list-bearing.
        let share_sql = format!(
            r#"SELECT
                CASE WHEN COUNT(*) = 0 THEN NULL
                     ELSE 100.0 * SUM(CASE WHEN m.list_id IS NOT NULL THEN 1 ELSE 0 END)
                                 / CAST(COUNT(*) AS REAL)
                END
              FROM messages m
              WHERE m.direction = 'inbound'
                AND (?1 IS NULL OR m.account_id = ?1)
                AND m.date >= ?2 AND m.date <= ?3
                AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}"#
        );
        let share_query = sqlx::query_as::<_, (Option<f64>,)>(&share_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_one(self.reader());

        let (unique_row, top_list, share_row): ((i64,), Option<TopListSqlRow>, (Option<f64>,)) =
            tokio::try_join!(unique_query, top_list_query, share_query)?;

        Ok(WrappedNewsletters {
            unique_lists: unique_row.0.max(0) as u32,
            top_list: top_list.map(|(list_id, total, opened)| WrappedTopList {
                list_id,
                message_count: total.max(0) as u32,
                opened_count: opened.max(0) as u32,
            }),
            list_share_of_inbound_pct: share_row.0.unwrap_or(0.0),
        })
    }

    async fn wrapped_superlatives(
        &self,
        acct: &Option<String>,
        since: i64,
        until: i64,
    ) -> Result<WrappedSuperlatives, sqlx::Error> {
        // Longest thread by message count in window.
        let longest_sql = format!(
            r#"SELECT m.thread_id, COUNT(*) AS c, MAX(m.subject)
               FROM messages m
               WHERE (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
                 AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
               GROUP BY m.thread_id
               HAVING c > 1
               ORDER BY c DESC
               LIMIT 1"#,
        );
        let longest: Option<(String, i64, String)> = sqlx::query_as(&longest_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_optional(self.reader())
            .await?;

        // Most ghosted: high inbound, zero outbound. Reuses the window_contacts
        // CTE shape from top_contacts but constrains to out_count = 0.
        let ghosted_sql = format!(
            r#"WITH window_contacts AS (
                SELECT LOWER(m.from_email) AS email, 'in' AS dir
                  FROM messages m
                  WHERE m.direction = 'inbound' AND m.from_email != ''
                    AND (?1 IS NULL OR m.account_id = ?1)
                    AND m.date >= ?2 AND m.date <= ?3
                    AND {NON_SELF_ADDRESSED_MESSAGE_PREDICATE}
                    AND NOT EXISTS (
                        SELECT 1
                        FROM account_addresses self_addr
                        WHERE LOWER(self_addr.email) = LOWER(m.from_email)
                    )
                UNION ALL
                SELECT LOWER(json_extract(t.value, '$.email')), 'out'
                  FROM messages m, json_each(m.to_addrs) t
                  WHERE m.direction = 'outbound'
                    AND (?1 IS NULL OR m.account_id = ?1)
                    AND m.date >= ?2 AND m.date <= ?3
                    AND NOT EXISTS (
                        SELECT 1
                        FROM account_addresses self_addr
                        WHERE LOWER(self_addr.email) = LOWER(json_extract(t.value, '$.email'))
                    )
              )
              SELECT email,
                     SUM(CASE WHEN dir = 'in' THEN 1 ELSE 0 END) AS in_c,
                     SUM(CASE WHEN dir = 'out' THEN 1 ELSE 0 END) AS out_c
              FROM window_contacts
              WHERE email IS NOT NULL AND email != ''
              GROUP BY email
              HAVING out_c = 0 AND in_c >= 10
              ORDER BY in_c DESC
              LIMIT 1"#,
        );
        let ghosted: Option<(String, i64, i64)> = sqlx::query_as(&ghosted_sql)
            .bind(acct)
            .bind(since)
            .bind(until)
            .fetch_optional(self.reader())
            .await?;

        Ok(WrappedSuperlatives {
            longest_thread: match longest {
                Some((id, count, subj)) => Some(WrappedLongestThread {
                    thread_id: decode_id(&id)?,
                    subject: subj,
                    message_count: count.max(0) as u32,
                }),
                None => None,
            },
            most_ghosted: ghosted.map(|(email, in_c, out_c)| WrappedMostGhosted {
                email,
                inbound_count: in_c.max(0) as u32,
                outbound_count: out_c.max(0) as u32,
            }),
        })
    }
}

/// Linear-interpolation percentile on a sorted slice. Returns 0 for empty
/// inputs; clamps `q` to [0, 1]. Same shape as the analytics helper but
/// duplicated to keep the module self-contained.
fn percentile(sorted: &[i64], q: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let q = q.clamp(0.0, 1.0);
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)].max(0) as u32
}

/// SQLite `strftime('%w', ...)` returns 0=Sunday … 6=Saturday.
fn dow_name(d: i64) -> &'static str {
    match d {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "Unknown",
    }
}
