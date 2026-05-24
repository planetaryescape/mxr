//! Per-sender relationship aggregates: a single read joining the
//! `contacts` materialized view with on-the-fly counts that don't live
//! in `contacts` (open threads, reply-later flags from this sender).
//!
//! Lightweight v1 — surfaces the stats users care about glancing at
//! when deciding "do I need to deal with this person right now?":
//!
//!   * Volume (in/out).
//!   * Last seen in either direction.
//!   * Median cadence (when refresh has computed it).
//!   * Replied count (proxy for "do I usually engage?").
//!   * Open thread count: threads whose latest message is from this
//!     sender and has no outbound reply yet.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_lookup, trace_query};
use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{ResponseTimeBucket, RESPONSE_TIME_HISTOGRAM_EDGES};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq)]
pub struct SenderProfile {
    pub account_id: AccountId,
    pub email: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_inbound_at: Option<DateTime<Utc>>,
    pub last_outbound_at: Option<DateTime<Utc>>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub replied_count: u32,
    pub cadence_days_p50: Option<f64>,
    pub is_list_sender: bool,
    pub list_id: Option<String>,
    pub open_thread_count: u32,
    pub inbound_storage_bytes: u64,
    pub outbound_storage_bytes: u64,
    pub attachment_count: u32,
    pub attachment_bytes: u64,
    pub unanswered_question: Option<SenderUnansweredQuestion>,
    pub response_histogram: Vec<ResponseTimeBucket>,
    pub weekly_activity: Vec<SenderWeeklyActivity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderEmailReference {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub snippet: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub date: DateTime<Utc>,
    pub direction: String,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderSummary {
    pub account_id: AccountId,
    pub display_name: Option<String>,
    pub sender_email: String,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_subject: String,
    pub latest_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderUnansweredQuestion {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub received_at: DateTime<Utc>,
    pub days_waiting: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderWeeklyActivity {
    pub week_start: DateTime<Utc>,
    pub inbound_count: u32,
    pub outbound_count: u32,
}

impl super::Store {
    /// Look up the per-sender profile for an (account, email) pair.
    /// Returns `None` if the contact is unknown.
    pub async fn get_sender_profile(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<Option<SenderProfile>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT account_id as "account_id!", email as "email!",
                      display_name, first_seen_at as "first_seen_at!",
                      last_seen_at as "last_seen_at!",
                      last_inbound_at, last_outbound_at,
                      total_inbound as "total_inbound!",
                      total_outbound as "total_outbound!",
                      replied_count as "replied_count!",
                      cadence_days_p50,
                      is_list_sender as "is_list_sender!",
                      list_id
               FROM contacts
               WHERE account_id = ? AND email = ? COLLATE NOCASE"#,
            aid,
            email,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("sender_profile.get", started_at, row.is_some());

        let Some(r) = row else { return Ok(None) };

        // "Open threads": threads where the latest envelope is from this
        // sender and there's no outbound message in the same thread that's
        // chronologically after it. Approximated by checking each thread
        // whose most-recent envelope is from `email`.
        let open = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64"
               FROM (
                   SELECT thread_id, MAX(date) AS latest_date
                   FROM messages
                   WHERE account_id = ?
                   GROUP BY thread_id
               ) t
               JOIN messages m
                 ON m.account_id = ? AND m.thread_id = t.thread_id AND m.date = t.latest_date
               WHERE LOWER(m.from_email) = LOWER(?)
                 AND NOT EXISTS (
                     SELECT 1 FROM messages om
                     WHERE om.account_id = ? AND om.thread_id = m.thread_id
                       AND om.direction = 'outbound' AND om.date >= m.date
                 )"#,
            aid,
            aid,
            email,
            aid,
        )
        .fetch_one(self.reader())
        .await?;

        let (inbound_storage_bytes, outbound_storage_bytes, attachment_count, attachment_bytes): (
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            r#"WITH matched AS (
                   SELECT
                     m.id,
                     CASE WHEN LOWER(m.from_email) = LOWER(?2) THEN m.size_bytes ELSE 0 END
                       AS inbound_storage_bytes,
                     CASE
                       WHEN m.direction = 'outbound' AND (
                         EXISTS (
                           SELECT 1 FROM json_each(m.to_addrs)
                           WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                         )
                         OR EXISTS (
                           SELECT 1 FROM json_each(m.cc_addrs)
                           WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                         )
                         OR EXISTS (
                           SELECT 1 FROM json_each(m.bcc_addrs)
                           WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                         )
                       )
                       THEN m.size_bytes
                       ELSE 0
                     END AS outbound_storage_bytes
                   FROM messages m
                   WHERE m.account_id = ?1
                     AND (
                       LOWER(m.from_email) = LOWER(?2)
                       OR (
                         m.direction = 'outbound'
                         AND (
                           EXISTS (
                             SELECT 1 FROM json_each(m.to_addrs)
                             WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                           )
                           OR EXISTS (
                             SELECT 1 FROM json_each(m.cc_addrs)
                             WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                           )
                           OR EXISTS (
                             SELECT 1 FROM json_each(m.bcc_addrs)
                             WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                           )
                         )
                       )
                     )
                 )
                 SELECT
                   COALESCE((SELECT SUM(inbound_storage_bytes) FROM matched), 0),
                   COALESCE((SELECT SUM(outbound_storage_bytes) FROM matched), 0),
                   COALESCE((SELECT COUNT(*) FROM attachments a JOIN matched ON matched.id = a.message_id), 0),
                   COALESCE((SELECT SUM(a.size_bytes) FROM attachments a JOIN matched ON matched.id = a.message_id), 0)"#,
        )
        .bind(aid)
        .bind(email)
        .fetch_one(self.reader())
        .await?;

        let unanswered_question = self
            .sender_unanswered_question(account_id, email, r.cadence_days_p50)
            .await?;
        let response_histogram = self.sender_response_histogram(account_id, email).await?;
        let weekly_activity = self.sender_weekly_activity(account_id, email).await?;

        Ok(Some(SenderProfile {
            account_id: decode_id(&r.account_id)?,
            email: r.email,
            display_name: r.display_name,
            first_seen_at: decode_timestamp(r.first_seen_at)?,
            last_seen_at: decode_timestamp(r.last_seen_at)?,
            last_inbound_at: decode_optional_timestamp(r.last_inbound_at)?,
            last_outbound_at: decode_optional_timestamp(r.last_outbound_at)?,
            total_inbound: r.total_inbound as u32,
            total_outbound: r.total_outbound as u32,
            replied_count: r.replied_count as u32,
            cadence_days_p50: r.cadence_days_p50,
            is_list_sender: r.is_list_sender != 0,
            list_id: r.list_id,
            open_thread_count: open as u32,
            inbound_storage_bytes: inbound_storage_bytes.max(0) as u64,
            outbound_storage_bytes: outbound_storage_bytes.max(0) as u64,
            attachment_count: attachment_count.max(0) as u32,
            attachment_bytes: attachment_bytes.max(0) as u64,
            unanswered_question,
            response_histogram,
            weekly_activity,
        }))
    }

    async fn sender_unanswered_question(
        &self,
        account_id: &AccountId,
        email: &str,
        cadence_days_p50: Option<f64>,
    ) -> Result<Option<SenderUnansweredQuestion>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, thread_id, subject, snippet, date
               FROM messages inbound
               WHERE inbound.account_id = ?1
                 AND inbound.direction = 'inbound'
                 AND LOWER(inbound.from_email) = LOWER(?2)
               ORDER BY inbound.date DESC, inbound.id DESC
               LIMIT 1"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .fetch_optional(self.reader())
        .await?;

        let Some(row) = row else { return Ok(None) };
        let subject: String = row.get("subject");
        let snippet: String = row.get("snippet");
        if !subject.contains('?') && !snippet.contains('?') {
            return Ok(None);
        }

        let thread_id: String = row.get("thread_id");
        let date: i64 = row.get("date");
        let replied: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1
               FROM messages outbound
               WHERE outbound.account_id = ?1
                 AND outbound.thread_id = ?2
                 AND outbound.direction = 'outbound'
                 AND outbound.date > ?3
                 AND (
                   EXISTS (
                     SELECT 1 FROM json_each(outbound.to_addrs)
                     WHERE LOWER(json_extract(value, '$.email')) = LOWER(?4)
                   )
                   OR EXISTS (
                     SELECT 1 FROM json_each(outbound.cc_addrs)
                     WHERE LOWER(json_extract(value, '$.email')) = LOWER(?4)
                   )
                   OR EXISTS (
                     SELECT 1 FROM json_each(outbound.bcc_addrs)
                     WHERE LOWER(json_extract(value, '$.email')) = LOWER(?4)
                   )
                 )
               LIMIT 1"#,
        )
        .bind(account_id.as_str())
        .bind(&thread_id)
        .bind(date)
        .bind(email)
        .fetch_optional(self.reader())
        .await?;
        if replied.is_some() {
            return Ok(None);
        }

        let received_at = decode_timestamp(date)?;
        let days_waiting = ((Utc::now().timestamp() - date).max(0) / 86_400) as u32;
        let cadence_days = cadence_days_p50.unwrap_or(1.0).max(1.0);
        if f64::from(days_waiting) < cadence_days {
            return Ok(None);
        }

        Ok(Some(SenderUnansweredQuestion {
            message_id: decode_id(row.get::<String, _>("id").as_str())?,
            thread_id: decode_id(&thread_id)?,
            subject,
            received_at,
            days_waiting,
        }))
    }

    async fn sender_response_histogram(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<Vec<ResponseTimeBucket>, sqlx::Error> {
        let rows: Vec<i64> = sqlx::query_scalar(
            r#"SELECT latency_seconds
               FROM reply_pairs
               WHERE account_id = ?1
                 AND direction = 'i_replied'
                 AND LOWER(counterparty_email) = LOWER(?2)"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .fetch_all(self.reader())
        .await?;
        Ok(build_sender_response_histogram(&rows))
    }

    async fn sender_weekly_activity(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<Vec<SenderWeeklyActivity>, sqlx::Error> {
        let current_week = week_start_unix(Utc::now().timestamp());
        let first_week = current_week - Duration::weeks(11).num_seconds();
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"SELECT direction, date
               FROM messages m
               WHERE m.account_id = ?1
                 AND m.date >= ?2
                 AND (
                   (m.direction = 'inbound' AND LOWER(m.from_email) = LOWER(?3))
                   OR (
                     m.direction = 'outbound'
                     AND (
                       EXISTS (
                         SELECT 1 FROM json_each(m.to_addrs)
                         WHERE LOWER(json_extract(value, '$.email')) = LOWER(?3)
                       )
                       OR EXISTS (
                         SELECT 1 FROM json_each(m.cc_addrs)
                         WHERE LOWER(json_extract(value, '$.email')) = LOWER(?3)
                       )
                       OR EXISTS (
                         SELECT 1 FROM json_each(m.bcc_addrs)
                         WHERE LOWER(json_extract(value, '$.email')) = LOWER(?3)
                       )
                     )
                   )
                 )"#,
        )
        .bind(account_id.as_str())
        .bind(first_week)
        .bind(email)
        .fetch_all(self.reader())
        .await?;

        let mut weeks: Vec<SenderWeeklyActivity> = (0..12)
            .map(|index| {
                let week_unix = first_week + Duration::weeks(index).num_seconds();
                SenderWeeklyActivity {
                    week_start: Utc
                        .timestamp_opt(week_unix, 0)
                        .single()
                        .expect("week bucket timestamp is generated from a valid UTC date"),
                    inbound_count: 0,
                    outbound_count: 0,
                }
            })
            .collect();

        for (direction, date) in rows {
            let week = week_start_unix(date);
            let offset = ((week - first_week) / Duration::weeks(1).num_seconds()) as usize;
            if let Some(bucket) = weeks.get_mut(offset) {
                match direction.as_str() {
                    "inbound" => bucket.inbound_count += 1,
                    "outbound" => bucket.outbound_count += 1,
                    _ => {}
                }
            }
        }

        Ok(weeks)
    }

    pub async fn list_recent_sender_messages(
        &self,
        account_id: &AccountId,
        email: &str,
        limit: u32,
    ) -> Result<Vec<SenderEmailReference>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let limit = i64::from(limit.clamp(1, 25));
        let rows = sqlx::query(
            r#"SELECT id, thread_id, subject, snippet, from_name, from_email,
                      date, direction, has_attachments
               FROM messages
               WHERE account_id = ?1
                 AND LOWER(from_email) = LOWER(?2)
               ORDER BY date DESC
               LIMIT ?3"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .bind(limit)
        .fetch_all(self.reader())
        .await?;
        trace_query("sender_profile.recent_messages", started_at, rows.len());

        rows.into_iter()
            .map(|row| {
                Ok(SenderEmailReference {
                    message_id: decode_id(row.get::<String, _>("id").as_str())?,
                    thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
                    subject: row.get::<String, _>("subject"),
                    snippet: row.get::<String, _>("snippet"),
                    from_name: row.get::<Option<String>, _>("from_name"),
                    from_email: row.get::<String, _>("from_email"),
                    date: decode_timestamp(row.get::<i64, _>("date"))?,
                    direction: row.get::<String, _>("direction"),
                    has_attachments: row.get::<bool, _>("has_attachments"),
                })
            })
            .collect()
    }

    pub async fn list_top_senders(&self, limit: u32) -> Result<Vec<SenderSummary>, sqlx::Error> {
        self.list_top_senders_since(limit, None).await
    }

    /// Like [`Self::list_top_senders`] but only counts (and ranks)
    /// inbound messages with `date >= since_unix`. Pass `None` for
    /// the un-bounded form. Used by `mxr senders --since 90d` to
    /// answer "who's been emailing me lately" rather than "who has
    /// emailed me ever".
    pub async fn list_top_senders_since(
        &self,
        limit: u32,
        since_unix: Option<i64>,
    ) -> Result<Vec<SenderSummary>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let limit = i64::from(limit.clamp(1, 500));
        let mut sql = String::from(
            r#"WITH grouped AS (
                   SELECT account_id, LOWER(from_email) AS sender_key,
                          MAX(date) AS latest_at,
                          COUNT(*) AS message_count,
                          SUM(CASE WHEN (flags & 1) = 0 THEN 1 ELSE 0 END) AS unread_count
                   FROM messages
                   WHERE direction = 'inbound' AND from_email != ''"#,
        );
        if since_unix.is_some() {
            sql.push_str(" AND date >= ?");
        }
        sql.push_str(
            r#"
                   GROUP BY account_id, LOWER(from_email)
               )
               SELECT g.account_id, latest.from_name AS display_name,
                      latest.from_email AS sender_email,
                      g.message_count, g.unread_count,
                      latest.subject AS latest_subject,
                      g.latest_at
               FROM grouped g
               JOIN messages latest
                 ON latest.account_id = g.account_id
                AND LOWER(latest.from_email) = g.sender_key
                AND latest.date = g.latest_at
               ORDER BY g.message_count DESC, g.latest_at DESC
               LIMIT ?"#,
        );

        let mut query = sqlx::query(&sql);
        if let Some(since) = since_unix {
            query = query.bind(since);
        }
        query = query.bind(limit);
        let rows = query.fetch_all(self.reader()).await?;
        trace_query("sender_profile.list_top_senders", started_at, rows.len());

        rows.into_iter()
            .map(|row| {
                Ok(SenderSummary {
                    account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
                    display_name: row.get::<Option<String>, _>("display_name"),
                    sender_email: row.get::<String, _>("sender_email"),
                    message_count: row.get::<i64, _>("message_count") as u32,
                    unread_count: row.get::<i64, _>("unread_count") as u32,
                    latest_subject: row.get::<String, _>("latest_subject"),
                    latest_at: decode_timestamp(row.get::<i64, _>("latest_at"))?,
                })
            })
            .collect()
    }
}

fn build_sender_response_histogram(clock_seconds: &[i64]) -> Vec<ResponseTimeBucket> {
    let mut counts = [0u32; RESPONSE_TIME_HISTOGRAM_EDGES.len()];
    for &latency in clock_seconds {
        if latency < 0 {
            continue;
        }
        let latency = u32::try_from(latency).unwrap_or(u32::MAX);
        for (index, edge) in RESPONSE_TIME_HISTOGRAM_EDGES.iter().enumerate() {
            if latency < *edge {
                counts[index] += 1;
                break;
            }
        }
    }

    RESPONSE_TIME_HISTOGRAM_EDGES
        .iter()
        .zip(counts)
        .map(|(&upper_bound_seconds, count)| ResponseTimeBucket {
            upper_bound_seconds,
            count,
        })
        .collect()
}

fn week_start_unix(timestamp: i64) -> i64 {
    let dt = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .expect("stored message timestamps must be valid UTC instants");
    let date = dt.date_naive() - Duration::days(i64::from(dt.weekday().num_days_from_monday()));
    Utc.from_utc_datetime(&date.and_time(NaiveTime::MIN))
        .timestamp()
}

#[cfg(test)]
mod tests {
    use super::super::Store;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use chrono::{TimeZone, Utc};
    use mxr_core::id::AccountId;
    use mxr_core::{Address, MessageDirection};

    #[tokio::test]
    async fn get_sender_profile_returns_none_for_unknown_contact() {
        let store = Store::in_memory().await.unwrap();
        let got = store
            .get_sender_profile(&AccountId::new(), "nobody@example.com")
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn list_recent_sender_messages_returns_latest_first() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut older = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        older.provider_id = "older".into();
        older.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        older.subject = "Older note".into();
        older.snippet = "The first note".into();
        older.date = Utc.with_ymd_and_hms(2026, 5, 10, 9, 0, 0).unwrap();
        older.has_attachments = true;

        let mut newer = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        newer.provider_id = "newer".into();
        newer.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        newer.subject = "Newer note".into();
        newer.snippet = "The latest note".into();
        newer.date = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();

        let mut other = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        other.provider_id = "other".into();
        other.from = Address {
            name: Some("Bob".into()),
            email: "bob@example.com".into(),
        };
        other.subject = "Different sender".into();
        other.date = Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap();

        store
            .upsert_envelope_with_direction(&older, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .upsert_envelope_with_direction(&newer, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .upsert_envelope_with_direction(&other, MessageDirection::Inbound)
            .await
            .unwrap();

        let messages = store
            .list_recent_sender_messages(&account.id, "ALICE@example.com", 10)
            .await
            .unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].subject, "Newer note");
        assert_eq!(messages[1].subject, "Older note");
        assert!(messages[1].has_attachments);
    }

    #[tokio::test]
    async fn get_sender_profile_includes_question_latency_and_weekly_activity() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let question_thread = mxr_core::id::ThreadId::new();
        let mut question = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        question.provider_id = "question".into();
        question.thread_id = question_thread;
        question.message_id_header = Some("<question@example.com>".into());
        question.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        question.to = vec![Address {
            name: None,
            email: account.email.clone(),
        }];
        question.subject = "Can you review this?".into();
        question.snippet = "Need your call by Friday".into();
        question.date = Utc::now() - chrono::Duration::days(14);
        store
            .upsert_envelope_with_direction(&question, MessageDirection::Inbound)
            .await
            .unwrap();

        let reply_thread = mxr_core::id::ThreadId::new();
        let mut parent = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        parent.provider_id = "parent".into();
        parent.thread_id = reply_thread.clone();
        parent.message_id_header = Some("<parent@example.com>".into());
        parent.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        parent.to = vec![Address {
            name: None,
            email: account.email.clone(),
        }];
        parent.subject = "Earlier request".into();
        parent.date = Utc::now() - chrono::Duration::days(20);
        store
            .upsert_envelope_with_direction(&parent, MessageDirection::Inbound)
            .await
            .unwrap();

        let mut reply = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        reply.provider_id = "reply".into();
        reply.thread_id = reply_thread;
        reply.message_id_header = Some("<reply@example.com>".into());
        reply.in_reply_to = Some("<parent@example.com>".into());
        reply.from = Address {
            name: Some("Me".into()),
            email: account.email.clone(),
        };
        reply.to = vec![Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        }];
        reply.subject = "Re: Earlier request".into();
        reply.date = parent.date + chrono::Duration::hours(3);
        store
            .upsert_envelope_with_direction(&reply, MessageDirection::Outbound)
            .await
            .unwrap();
        assert!(store
            .try_create_reply_pair(&reply, MessageDirection::Outbound)
            .await
            .unwrap());

        store.refresh_contacts().await.unwrap();

        let profile = store
            .get_sender_profile(&account.id, "alice@example.com")
            .await
            .unwrap()
            .expect("sender profile");

        let question = profile.unanswered_question.expect("question signal");
        assert_eq!(question.subject, "Can you review this?");
        assert!(question.days_waiting >= 14);
        assert_eq!(
            profile
                .response_histogram
                .iter()
                .find(|bucket| bucket.upper_bound_seconds == 21_600)
                .map(|bucket| bucket.count),
            Some(1)
        );
        assert_eq!(
            profile
                .weekly_activity
                .iter()
                .map(|week| week.inbound_count + week.outbound_count)
                .sum::<u32>(),
            3
        );
    }

    /// Phase 2.6: `list_top_senders` orders strictly by message count
    /// descending. Equal-count senders break ties by `latest_at`
    /// descending so the most recently active sender ranks first.
    /// The user-facing `mxr senders --top N` command depends on this
    /// for the "who's filling my inbox" workflow.
    #[tokio::test]
    async fn list_top_senders_orders_by_message_count_desc() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Alice: 3 messages
        // Bob:   1 message
        // Carol: 2 messages
        // Expected ranking: Alice (3), Carol (2), Bob (1).
        let senders = [
            ("alice@example.com", "Alice", 3),
            ("bob@example.com", "Bob", 1),
            ("carol@example.com", "Carol", 2),
        ];

        for (email, name, count) in senders {
            for i in 0..count {
                let mut env = TestEnvelopeBuilder::new()
                    .account_id(account.id.clone())
                    .build();
                env.provider_id = format!("{email}-{i}");
                env.message_id_header = Some(format!("<{email}-{i}@example.com>"));
                env.from = Address {
                    name: Some(name.into()),
                    email: email.into(),
                };
                env.subject = format!("from {name} #{i}");
                env.date = Utc
                    .with_ymd_and_hms(2026, 5, 10 + i as u32, 9, 0, 0)
                    .unwrap();
                store
                    .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                    .await
                    .unwrap();
            }
        }

        let rows = store.list_top_senders(10).await.unwrap();

        assert_eq!(rows.len(), 3, "all three senders present");
        assert_eq!(rows[0].sender_email, "alice@example.com");
        assert_eq!(rows[0].message_count, 3);
        assert_eq!(rows[1].sender_email, "carol@example.com");
        assert_eq!(rows[1].message_count, 2);
        assert_eq!(rows[2].sender_email, "bob@example.com");
        assert_eq!(rows[2].message_count, 1);
    }

    /// Phase 2.6: `latest_at` carries the most recent inbound date.
    /// Two senders with the same message count must order by who
    /// emailed most recently — that's what the user means by "top".
    #[tokio::test]
    async fn list_top_senders_breaks_ties_by_latest_message() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Two senders, one message each. The one with the more-recent
        // `date` must come first.
        for (email, slug, date) in [
            (
                "old@example.com",
                "old-1",
                Utc.with_ymd_and_hms(2026, 5, 1, 9, 0, 0).unwrap(),
            ),
            (
                "fresh@example.com",
                "fresh-1",
                Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap(),
            ),
        ] {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            env.provider_id = slug.into();
            env.message_id_header = Some(format!("<{slug}@example.com>"));
            env.from = Address {
                name: Some("Counter Party".into()),
                email: email.into(),
            };
            env.subject = format!("from {email}");
            env.date = date;
            store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
        }

        let rows = store.list_top_senders(10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].sender_email, "fresh@example.com",
            "more-recent sender breaks the tie at message_count == 1"
        );
        assert_eq!(rows[1].sender_email, "old@example.com");
    }

    /// Phase 2.6: only INBOUND messages are counted. Outbound messages
    /// the user sent must not pollute the "top senders" list — those
    /// belong to the *user*, not to a sender. A subtle but important
    /// boundary case.
    #[tokio::test]
    async fn list_top_senders_excludes_outbound_messages() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Outbound: this should never appear in the top-senders list.
        let mut sent = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        sent.provider_id = "outbound-1".into();
        sent.message_id_header = Some("<outbound-1@example.com>".into());
        sent.from = Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        };
        sent.subject = "Outbound reply".into();
        sent.date = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();

        // Inbound: should appear.
        let mut received = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        received.provider_id = "inbound-1".into();
        received.message_id_header = Some("<inbound-1@example.com>".into());
        received.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        received.subject = "Hello".into();
        received.date = Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap();
        store
            .upsert_envelope_with_direction(&received, MessageDirection::Inbound)
            .await
            .unwrap();

        let rows = store.list_top_senders(10).await.unwrap();
        assert_eq!(rows.len(), 1, "only the inbound sender appears");
        assert_eq!(rows[0].sender_email, "alice@example.com");
        assert!(
            !rows.iter().any(|row| row.sender_email == "me@example.com"),
            "outbound 'me@example.com' must not appear as a top sender"
        );
    }

    /// Phase 2.6: `--since` bounds the count window. A sender whose
    /// last message landed outside the window must NOT appear in the
    /// list; this is the difference between "who's emailed me ever"
    /// and "who's emailing me lately."
    #[tokio::test]
    async fn list_top_senders_since_excludes_older_messages() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        // Three months apart so any reasonable cutoff in between
        // catches one and rejects the other.
        let old_date = Utc.with_ymd_and_hms(2025, 12, 1, 9, 0, 0).unwrap();
        let fresh_date = Utc.with_ymd_and_hms(2026, 5, 1, 9, 0, 0).unwrap();

        for (slug, email, name, date) in [
            ("ancient-1", "ancient@example.com", "Ancient", old_date),
            ("recent-1", "recent@example.com", "Recent", fresh_date),
        ] {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            env.provider_id = slug.into();
            env.message_id_header = Some(format!("<{slug}@example.com>"));
            env.from = Address {
                name: Some(name.into()),
                email: email.into(),
            };
            env.subject = format!("from {name}");
            env.date = date;
            store
                .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
                .await
                .unwrap();
        }

        // No bound: both senders appear.
        let unbounded = store.list_top_senders_since(10, None).await.unwrap();
        assert_eq!(unbounded.len(), 2, "unbounded list returns both");

        // Cutoff between the two dates: only the recent one.
        let cutoff = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .unwrap()
            .timestamp();
        let bounded = store
            .list_top_senders_since(10, Some(cutoff))
            .await
            .unwrap();
        assert_eq!(bounded.len(), 1, "since-cutoff drops the older sender");
        assert_eq!(bounded[0].sender_email, "recent@example.com");
    }

    /// Phase 2.6: a cutoff in the future returns an empty list rather
    /// than panicking or returning all-time. Safety net for off-by-one
    /// errors in CLI-driven date math.
    #[tokio::test]
    async fn list_top_senders_since_future_cutoff_returns_empty() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        env.provider_id = "any".into();
        env.message_id_header = Some("<any@example.com>".into());
        env.from = Address {
            name: Some("Any".into()),
            email: "any@example.com".into(),
        };
        env.date = Utc.with_ymd_and_hms(2026, 5, 1, 9, 0, 0).unwrap();
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();

        let cutoff = Utc
            .with_ymd_and_hms(2099, 1, 1, 0, 0, 0)
            .unwrap()
            .timestamp();
        let rows = store
            .list_top_senders_since(10, Some(cutoff))
            .await
            .unwrap();
        assert!(rows.is_empty(), "future cutoff filters everything out");
    }
}
