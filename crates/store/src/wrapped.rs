use crate::{decode_id, decode_timestamp, trace_query};
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

        let volume = self.wrapped_volume(&acct, floor, until_unix).await?;
        let time_patterns = self.wrapped_time_patterns(&acct, floor, until_unix).await?;
        let top_contacts = self.wrapped_top_contacts(&acct, floor, until_unix).await?;
        let reply_discipline = self
            .wrapped_reply_discipline(&acct, floor, until_unix)
            .await?;
        let storage = self.wrapped_storage(&acct, floor, until_unix).await?;
        let newsletters = self.wrapped_newsletters(&acct, floor, until_unix).await?;
        let superlatives = self.wrapped_superlatives(&acct, floor, until_unix).await?;

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
        let row: (i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                COALESCE(SUM(CASE WHEN direction = 'inbound' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN direction = 'outbound' THEN 1 ELSE 0 END), 0),
                COUNT(DISTINCT thread_id)
              FROM messages
              WHERE (?1 IS NULL OR account_id = ?1)
                AND date >= ?2
                AND date <= ?3"#,
        )
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
        // Full DOW distribution (up to 7 rows). SQLite's strftime('%w')
        // returns 0=Sunday..6=Saturday; the public `day_of_week_distribution`
        // array uses 0=Monday..6=Sunday, so remap by `(sqlite_dow + 6) % 7`.
        let dow_rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"SELECT CAST(strftime('%w', date, 'unixepoch') AS INTEGER) AS dow,
                      COUNT(*) AS c
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
               GROUP BY dow"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;

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

        // Full hour distribution (up to 24 rows).
        let hour_rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"SELECT CAST(strftime('%H', date, 'unixepoch') AS INTEGER) AS hr,
                      COUNT(*) AS c
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
               GROUP BY hr"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;

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

        let day: Option<(i64, i64)> = sqlx::query_as(
            r#"SELECT MIN(date) AS first_ts, COUNT(*) AS c
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
               GROUP BY date(date, 'unixepoch')
               ORDER BY c DESC, first_ts ASC
               LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_optional(self.reader())
        .await?;

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
        // Top inbound senders.
        let inbound: Vec<(String, Option<String>, i64)> = sqlx::query_as(
            r#"SELECT LOWER(from_email), MAX(from_name), COUNT(*) AS c
               FROM messages
               WHERE direction = 'inbound'
                 AND from_email != ''
                 AND (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
               GROUP BY LOWER(from_email)
               ORDER BY c DESC
               LIMIT ?4"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .bind(TOP_N)
        .fetch_all(self.reader())
        .await?;

        // Top outbound recipients (json_each over to_addrs).
        let outbound: Vec<(String, i64)> = sqlx::query_as(
            r#"SELECT LOWER(json_extract(t.value, '$.email')) AS email,
                      COUNT(*) AS c
               FROM messages m, json_each(m.to_addrs) t
               WHERE m.direction = 'outbound'
                 AND (?1 IS NULL OR m.account_id = ?1)
                 AND m.date >= ?2 AND m.date <= ?3
               GROUP BY email
               HAVING email IS NOT NULL AND email != ''
               ORDER BY c DESC
               LIMIT ?4"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .bind(TOP_N)
        .fetch_all(self.reader())
        .await?;

        // Most asymmetric over the window — recompute (the materialized
        // contacts table is all-time, doesn't respect window). Requires
        // ≥5 inbound to filter out one-off senders.
        let asym: Vec<(String, Option<String>, i64, i64)> = sqlx::query_as(
            r#"WITH window_contacts AS (
                SELECT LOWER(from_email) AS email, from_name AS name,
                       'in' AS dir
                  FROM messages
                  WHERE direction = 'inbound' AND from_email != ''
                    AND (?1 IS NULL OR account_id = ?1)
                    AND date >= ?2 AND date <= ?3
                UNION ALL
                SELECT LOWER(json_extract(t.value, '$.email')),
                       json_extract(t.value, '$.name'),
                       'out'
                  FROM messages m, json_each(m.to_addrs) t
                  WHERE m.direction = 'outbound'
                    AND (?1 IS NULL OR m.account_id = ?1)
                    AND m.date >= ?2 AND m.date <= ?3
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
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;

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
                 AND (?1 IS NULL OR account_id = ?1)"#,
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
        let total_row: (i64,) = sqlx::query_as(
            r#"SELECT COALESCE(SUM(size_bytes), 0)
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_one(self.reader())
        .await?;

        let mime: Option<(String, i64, i64)> = sqlx::query_as(
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
        .fetch_optional(self.reader())
        .await?;

        let heaviest: Option<(String, String, String, i64, i64)> = sqlx::query_as(
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
        .fetch_optional(self.reader())
        .await?;

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
        let unique_row: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(DISTINCT list_id)
               FROM messages
               WHERE list_id IS NOT NULL
                 AND (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_one(self.reader())
        .await?;

        let read_flag = MessageFlags::READ.bits() as i64;
        let top_list: Option<(String, i64, i64)> = sqlx::query_as(
            r#"SELECT list_id,
                      COUNT(*),
                      SUM(CASE WHEN (flags & ?4) = ?4 THEN 1 ELSE 0 END)
               FROM messages
               WHERE list_id IS NOT NULL
                 AND (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
               GROUP BY list_id
               ORDER BY COUNT(*) DESC
               LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .bind(read_flag)
        .fetch_optional(self.reader())
        .await?;

        // Share of inbound that is list-bearing.
        let share_row: (Option<f64>,) = sqlx::query_as(
            r#"SELECT
                CASE WHEN COUNT(*) = 0 THEN NULL
                     ELSE 100.0 * SUM(CASE WHEN list_id IS NOT NULL THEN 1 ELSE 0 END)
                                / CAST(COUNT(*) AS REAL)
                END
              FROM messages
              WHERE direction = 'inbound'
                AND (?1 IS NULL OR account_id = ?1)
                AND date >= ?2 AND date <= ?3"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_one(self.reader())
        .await?;

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
        let longest: Option<(String, i64, String)> = sqlx::query_as(
            r#"SELECT thread_id, COUNT(*) AS c, MAX(subject)
               FROM messages
               WHERE (?1 IS NULL OR account_id = ?1)
                 AND date >= ?2 AND date <= ?3
               GROUP BY thread_id
               HAVING c > 1
               ORDER BY c DESC
               LIMIT 1"#,
        )
        .bind(acct)
        .bind(since)
        .bind(until)
        .fetch_optional(self.reader())
        .await?;

        // Most ghosted: high inbound, zero outbound. Reuses the window_contacts
        // CTE shape from top_contacts but constrains to out_count = 0.
        let ghosted: Option<(String, i64, i64)> = sqlx::query_as(
            r#"WITH window_contacts AS (
                SELECT LOWER(from_email) AS email, 'in' AS dir
                  FROM messages
                  WHERE direction = 'inbound' AND from_email != ''
                    AND (?1 IS NULL OR account_id = ?1)
                    AND date >= ?2 AND date <= ?3
                UNION ALL
                SELECT LOWER(json_extract(t.value, '$.email')), 'out'
                  FROM messages m, json_each(m.to_addrs) t
                  WHERE m.direction = 'outbound'
                    AND (?1 IS NULL OR m.account_id = ?1)
                    AND m.date >= ?2 AND m.date <= ?3
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
        )
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
