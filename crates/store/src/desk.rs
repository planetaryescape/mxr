//! Raw reads behind the desk (`Request::GetDesk`).
//!
//! The desk asks "what needs me?" instead of "what arrived?". The daemon
//! decides lanes; this module only fetches what that decision needs, in a
//! handful of windowed queries so the whole desk stays a fast local read:
//!
//! * every message of every thread that saw activity inside the window,
//!   with its inbox/trash/snooze/invite/delivery state;
//! * the contact rows and reply latencies for the counterparties involved.

use crate::{decode_id, decode_json, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};
use sqlx::Row;
use std::collections::HashMap;
use std::time::Instant;

pub(crate) const CONVERSATION_THREADS: &str = "SELECT thread_id FROM messages
             WHERE account_id = ?1 AND direction != 'inbound' AND date >= ?3 AND date <= ?4
             UNION SELECT m.thread_id FROM mail_assessments home
             CROSS JOIN messages m ON m.id = home.message_id AND m.account_id = home.account_id
             WHERE home.account_id = ?1 AND home.policy_version = ?5
                 AND home.policy_version NOT LIKE 'asks-%'
                 AND json_extract(home.assessment_json, '$.home') = 'messages'
                 AND m.date >= ?3 AND m.date <= ?4
             UNION SELECT m.thread_id FROM messages m INDEXED BY idx_messages_conversation_inputs
              LEFT JOIN mail_assessments home ON home.message_id = m.id
                  AND home.policy_version = ?5 AND home.account_id = m.account_id
                  AND home.policy_version NOT LIKE 'asks-%'
             LEFT JOIN assessment_thread_inputs inputs
                 ON inputs.account_id = m.account_id AND inputs.thread_id = m.thread_id
              WHERE m.account_id = ?1 AND m.direction = 'inbound' AND m.date >= ?3 AND m.date <= ?4
                 AND (home.message_id IS NULL OR home.input_revision != inputs.revision)
             UNION SELECT m.thread_id FROM assessment_sender_inputs sender
             CROSS JOIN messages m ON m.account_id = sender.account_id AND lower(m.from_email) = +sender.email
             JOIN assessment_thread_inputs inputs ON inputs.account_id = m.account_id AND inputs.thread_id = m.thread_id
             WHERE sender.account_id = ?1 AND sender.revision > inputs.revision
                 AND EXISTS (SELECT 1 FROM messages recent WHERE recent.account_id = m.account_id
                     AND recent.thread_id = m.thread_id AND recent.date >= ?3 AND recent.date <= ?4)";

/// One message of a thread that was active inside the desk window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskMessage {
    pub id: MessageId,
    /// Storage order (the row's rowid): later means stored later, whatever
    /// the Date header says.
    pub seq: i64,
    pub thread_id: ThreadId,
    /// `inbound`, `outbound` or `unknown`, as stored.
    pub direction: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    /// Only your own sent mail knows its Bcc; it still counts as writing
    /// to them.
    pub bcc: Vec<Address>,
    pub subject: String,
    pub evidence: mxr_core::mail_evidence::MailEvidence,
    pub list_id: Option<String>,
    pub unsubscribe: UnsubscribeMethod,
    pub in_inbox: bool,
    /// In Trash or Spam, by label or flag.
    pub trashed: bool,
    pub snoozed: bool,
    pub is_invite: bool,
    pub is_delivery: bool,
}

/// The contact facts the desk needs about a counterparty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskContact {
    pub email: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub is_list_sender: bool,
    /// Their usual interval between messages, in seconds, once the
    /// contacts refresher has seen enough mail.
    pub cadence_seconds: Option<i64>,
    /// Their recent inbound mail. Only read for senders you have never
    /// written to (see `Store::sender_histories`); empty until a caller
    /// fills it.
    pub history: crate::SenderHistory,
}

/// How far a thread had arrived when it was marked done: the ids of the
/// messages it had, so any message the mark never saw is new whatever its
/// Date header says, plus its newest message by (date, id) and its count.
/// Message ids are stable and never reused. Marks written before the id
/// list fall back to the (date, id) watermark and the count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskDismissal {
    /// Unix seconds of the newest message's date.
    pub through_date: i64,
    pub through_id: uuid::Uuid,
    pub through_count: usize,
    /// The messages the mark saw, sorted; `None` on an older mark.
    pub covered: Option<std::sync::Arc<[uuid::Uuid]>>,
}

impl DeskDismissal {
    /// The watermark that covers exactly `messages`; `None` when empty.
    pub fn through<'a>(messages: impl IntoIterator<Item = &'a DeskMessage>) -> Option<Self> {
        let messages: Vec<&DeskMessage> = messages.into_iter().collect();
        let newest = messages.iter().map(|m| watermark_key(m)).max()?;
        let mut covered: Vec<uuid::Uuid> = messages.iter().map(|m| *m.id.as_uuid()).collect();
        covered.sort_unstable();
        covered.dedup();
        Some(Self {
            through_date: newest.0,
            through_id: newest.1,
            through_count: messages.len(),
            covered: Some(covered.into()),
        })
    }

    /// The mark saw this message: it is in the mark's id list, or, on an
    /// older mark without one, at or before its (date, id) watermark.
    pub fn saw(&self, date: DateTime<Utc>, id: &MessageId) -> bool {
        match &self.covered {
            Some(ids) => ids.binary_search(id.as_uuid()).is_ok(),
            None => (date.timestamp(), *id.as_uuid()) <= (self.through_date, self.through_id),
        }
    }

    /// Still dismissed: the mark saw every message of the thread. An older
    /// mark also needs the count unchanged, which catches a new message
    /// dated before its watermark.
    pub fn covers(&self, thread: &[DeskMessage]) -> bool {
        (self.covered.is_some() || thread.len() <= self.through_count)
            && thread
                .iter()
                .all(|message| self.saw(message.date, &message.id))
    }

    /// Which of two marks on one thread is further along.
    pub fn order_key(&self) -> (i64, uuid::Uuid, usize) {
        (self.through_date, self.through_id, self.through_count)
    }
}

/// A message's place in watermark order: date, then id. Uuid order is the
/// order of their lowercase text, so SQL's `ORDER BY date, id` agrees.
fn watermark_key(message: &DeskMessage) -> (i64, uuid::Uuid) {
    (message.date.timestamp(), *message.id.as_uuid())
}

/// Read a watermark's columns (`through_date`, `through_message_id`,
/// `through_count`) from a row.
pub(crate) fn dismissal_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<DeskDismissal, sqlx::Error> {
    let id: &str = row.try_get("through_message_id")?;
    let covered: Option<String> = row.try_get("covered_ids")?;
    let covered = covered
        .map(|json| {
            let mut ids: Vec<uuid::Uuid> =
                serde_json::from_str(&json).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            ids.sort_unstable();
            Ok::<_, sqlx::Error>(std::sync::Arc::from(ids))
        })
        .transpose()?;
    Ok(DeskDismissal {
        through_date: row.try_get("through_date")?,
        through_id: uuid::Uuid::parse_str(id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        through_count: row.try_get::<i64, _>("through_count")?.max(0) as usize,
        covered,
    })
}

/// One past reply with a counterparty (`reply_pairs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskReplyLatency {
    /// Lowercased counterparty address.
    pub email: String,
    /// `i_replied` or `they_replied`.
    pub direction: String,
    pub latency_seconds: i64,
}

/// The newest message exchanged with one address, for rows that start from
/// a person rather than a thread (cadence drift).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskLatestExchange {
    pub thread_id: ThreadId,
    pub message_id: MessageId,
    pub subject: String,
    pub date: DateTime<Utc>,
}

impl super::Store {
    /// Every message of every thread in `account_id` that has a message
    /// dated at or after `since`, ordered by thread then date.
    ///
    /// Threads are picked by recent activity but returned whole, so the
    /// caller sees the full conversation (who wrote last, every message id
    /// a verb must cover) and not just the recent tail.
    pub async fn desk_thread_messages(
        &self,
        account_id: &AccountId,
        since: DateTime<Utc>,
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        // Messages dated in the future (bad Date: headers) would otherwise
        // pin a thread to the top of every lane forever.
        let future_cutoff = Utc::now().timestamp() + 86_400;
        self.desk_messages_where(
            account_id,
            Evidence::PendingAndTimers,
            "SELECT DISTINCT thread_id FROM messages
             WHERE account_id = ?1 AND date >= ?3 AND date <= ?4",
            |query| query.bind(since.timestamp()).bind(future_cutoff),
            "desk.thread_messages",
        )
        .await
    }

    /// Messages history only needs threads that can contain correspondence.
    /// Settled non-Messages homes cannot contribute a person; pending/stale
    /// homes and sent/unknown-direction mail remain candidates. Return whole
    /// threads so filtering never changes their shape or done watermark.
    pub async fn desk_conversation_messages(
        &self,
        account_id: &AccountId,
        since: DateTime<Utc>,
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        self.desk_messages_where(
            account_id,
            Evidence::None,
            CONVERSATION_THREADS,
            |query| {
                query
                    .bind(since.timestamp())
                    .bind(Utc::now().timestamp() + 86_400)
                    .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
            },
            "desk.conversation_messages",
        )
        .await
    }

    /// Every message of the given threads, in the same shape: for rows that
    /// start from a promise or a watched contact rather than recent mail.
    pub async fn desk_messages_in_threads(
        &self,
        account_id: &AccountId,
        thread_ids: &[ThreadId],
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        self.desk_selected_threads(
            account_id,
            thread_ids,
            Evidence::Full,
            "desk.messages_in_threads",
        )
        .await
    }

    /// The same whole threads without body evidence: settled history only
    /// needs geometry and message metadata. Pending facets still materialize
    /// from authoritative sources before the daemon can persist a synopsis.
    pub async fn desk_history_in_threads(
        &self,
        account_id: &AccountId,
        thread_ids: &[ThreadId],
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        self.desk_selected_threads(
            account_id,
            thread_ids,
            Evidence::None,
            "desk.history_inputs",
        )
        .await
    }

    async fn desk_selected_threads(
        &self,
        account_id: &AccountId,
        thread_ids: &[ThreadId],
        evidence: Evidence,
        operation: &'static str,
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        self.desk_messages_where(
            account_id,
            evidence,
            "SELECT value AS thread_id FROM json_each(?3)",
            |query| query.bind(wanted),
            operation,
        )
        .await
    }

    async fn desk_messages_where<'q>(
        &self,
        account_id: &AccountId,
        evidence: Evidence,
        active_threads: &'static str,
        bind: impl FnOnce(
            sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
        )
            -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
        operation: &'static str,
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        let started_at = Instant::now();
        let hidden_flags = i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits());
        let sql = desk_messages_sql(active_threads, evidence);
        // ?1 account, ?2 hidden flags, then the thread selection's own.
        let query = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(hidden_flags);
        let rows = bind(query).fetch_all(self.reader()).await?;
        let messages = rows
            .into_iter()
            .map(|row| {
                let unsubscribe = row
                    .try_get::<Option<String>, _>("unsubscribe_method")?
                    .as_deref()
                    .map(decode_json::<UnsubscribeMethod>)
                    .transpose()?
                    .unwrap_or(UnsubscribeMethod::None);
                Ok(DeskMessage {
                    id: decode_id(row.try_get::<&str, _>("id")?)?,
                    seq: row.try_get("seq")?,
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    direction: row.try_get("direction")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                    flags: MessageFlags::from_bits_truncate(row.try_get::<i64, _>("flags")? as u32),
                    from: Address {
                        name: row.try_get("from_name")?,
                        email: row.try_get("from_email")?,
                    },
                    to: decode_json(row.try_get::<&str, _>("to_addrs")?)?,
                    cc: decode_json(row.try_get::<&str, _>("cc_addrs")?)?,
                    bcc: decode_json(row.try_get::<&str, _>("bcc_addrs")?)?,
                    subject: row.try_get("subject")?,
                    evidence: crate::body::evidence_from_row(&row)?,
                    list_id: row.try_get("list_id")?,
                    unsubscribe,
                    in_inbox: row.try_get("in_inbox")?,
                    trashed: row.try_get("trashed")?,
                    snoozed: row.try_get("snoozed")?,
                    is_invite: row.try_get("is_invite")?,
                    is_delivery: row.try_get("is_delivery")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query(operation, started_at, messages.len());
        Ok(messages)
    }

    /// Contact rows for `emails` (matched case-insensitively).
    pub async fn desk_contacts(
        &self,
        account_id: &AccountId,
        emails: &[String],
    ) -> Result<Vec<DeskContact>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let wanted = serde_json::to_string(emails).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"SELECT email, display_name, first_seen_at, total_inbound, total_outbound,
                      is_list_sender, cadence_days_p50
               FROM contacts
               WHERE account_id = ?1
                 AND email IN (SELECT LOWER(value) FROM json_each(?2))"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        let contacts = rows
            .into_iter()
            .map(|row| {
                Ok(DeskContact {
                    email: row.try_get("email")?,
                    display_name: row.try_get("display_name")?,
                    first_seen_at: decode_timestamp(row.try_get("first_seen_at")?)?,
                    total_inbound: row.try_get::<i64, _>("total_inbound")?.max(0) as u32,
                    total_outbound: row.try_get::<i64, _>("total_outbound")?.max(0) as u32,
                    is_list_sender: row.try_get::<i64, _>("is_list_sender")? != 0,
                    cadence_seconds: row
                        .try_get::<Option<f64>, _>("cadence_days_p50")?
                        .map(|days| (days * 86_400.0).round() as i64),
                    history: crate::SenderHistory::default(),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("desk.contacts", started_at, contacts.len());
        Ok(contacts)
    }

    /// Past reply latencies with `emails`, both directions, excluding pairs
    /// with the account's own addresses (as `list_response_time` does).
    pub async fn desk_reply_latencies(
        &self,
        account_id: &AccountId,
        emails: &[String],
    ) -> Result<Vec<DeskReplyLatency>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let wanted = serde_json::to_string(emails).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"SELECT LOWER(counterparty_email) AS email, direction, latency_seconds
               FROM reply_pairs
               WHERE account_id = ?1
                 AND LOWER(counterparty_email) IN (SELECT LOWER(value) FROM json_each(?2))
                 AND latency_seconds >= 0"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        let latencies = rows
            .into_iter()
            .map(|row| {
                Ok(DeskReplyLatency {
                    email: row.try_get("email")?,
                    direction: row.try_get("direction")?,
                    latency_seconds: row.try_get("latency_seconds")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("desk.reply_latencies", started_at, latencies.len());
        Ok(latencies)
    }

    /// For each `(email, around)`, the newest non-trashed message from or to
    /// that address dated within a day of `around` (the contact's known last
    /// exchange). One query for every contact; the date bound keeps each
    /// lookup to a handful of rows instead of a walk over the mailbox.
    pub async fn desk_latest_exchanges(
        &self,
        account_id: &AccountId,
        wanted: &[(String, DateTime<Utc>)],
    ) -> Result<HashMap<String, DeskLatestExchange>, sqlx::Error> {
        if wanted.is_empty() {
            return Ok(HashMap::new());
        }
        let started_at = Instant::now();
        let hidden_flags = i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits());
        let windows: Vec<serde_json::Value> = wanted
            .iter()
            .map(|(email, around)| {
                serde_json::json!({
                    "email": email.to_ascii_lowercase(),
                    "lo": around.timestamp() - 86_400,
                    "hi": around.timestamp() + 86_400,
                })
            })
            .collect();
        let windows =
            serde_json::to_string(&windows).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"WITH wanted AS (
                SELECT json_extract(value, '$.email') AS email,
                       json_extract(value, '$.lo') AS lo,
                       json_extract(value, '$.hi') AS hi
                FROM json_each(?2)
            )
            SELECT wanted.email AS email, m.id, m.thread_id, m.subject, m.date
            -- Contacts drive the loop: each is a two-day date-range lookup.
            FROM wanted
            CROSS JOIN messages m
              ON m.account_id = ?1
             AND m.date BETWEEN wanted.lo AND wanted.hi
             AND (m.flags & ?3) = 0
             AND (
                 LOWER(m.from_email) = wanted.email
                 OR EXISTS (
                     SELECT 1 FROM json_each(m.to_addrs) recipient
                     WHERE LOWER(json_extract(recipient.value, '$.email')) = wanted.email
                 )
             )
            ORDER BY m.date DESC, m.id DESC"#,
        )
        .bind(account_id.as_str())
        .bind(windows)
        .bind(hidden_flags)
        .fetch_all(self.reader())
        .await?;
        let mut latest = HashMap::new();
        for row in rows {
            let email: String = row.try_get("email")?;
            if latest.contains_key(&email) {
                continue;
            }
            latest.insert(
                email,
                DeskLatestExchange {
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    message_id: decode_id(row.try_get::<&str, _>("id")?)?,
                    subject: row.try_get("subject")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                },
            );
        }
        trace_query("desk.latest_exchanges", started_at, latest.len());
        Ok(latest)
    }

    /// Which of `emails` (lowercase) you have ever sent mail to from this
    /// account, as To, Cc or Bcc: a first-time sender is someone you never
    /// wrote to anywhere, not just in the thread at hand.
    pub async fn addresses_written_to(
        &self,
        account_id: &AccountId,
        emails: &[String],
    ) -> Result<std::collections::HashSet<String>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(std::collections::HashSet::new());
        }
        let wanted = serde_json::to_string(emails).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT DISTINCT r.email
               FROM json_each(?2) wanted
               CROSS JOIN outbound_recipients r ON r.account_id = ?1 AND r.email = wanted.value"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        Ok(rows.into_iter().map(|(email,)| email).collect())
    }

    /// Threads marked "done waiting", with how far they had arrived.
    pub async fn desk_dismissals(
        &self,
        account_id: &AccountId,
    ) -> Result<HashMap<ThreadId, DeskDismissal>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT thread_id, through_date, through_message_id, through_count, covered_ids
             FROM desk_dismissals WHERE account_id = ?1",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    dismissal_from_row(row)?,
                ))
            })
            .collect()
    }

    /// Mark threads "done waiting" through the messages stored so far.
    /// Threads with no stored message are skipped. With `dry_run`, nothing
    /// is written and the same selection is returned, so a preview matches.
    pub async fn dismiss_desk_threads(
        &self,
        thread_ids: &[ThreadId],
        dry_run: bool,
    ) -> Result<Vec<(AccountId, ThreadId)>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        // One selection for the preview and the write, so they agree. The
        // watermark is the newest message by (date, id) plus the count, as
        // `DeskDismissal` reads it.
        const SELECTION: &str = r#"SELECT t.account_id, t.thread_id, t.through_count,
                      t.covered_ids,
                      (SELECT m.date FROM messages m
                       WHERE m.account_id = t.account_id AND m.thread_id = t.thread_id
                       ORDER BY m.date DESC, m.id DESC LIMIT 1) AS through_date,
                      (SELECT m.id FROM messages m
                       WHERE m.account_id = t.account_id AND m.thread_id = t.thread_id
                       ORDER BY m.date DESC, m.id DESC LIMIT 1) AS through_message_id
               FROM (SELECT account_id, thread_id, COUNT(*) AS through_count,
                            json_group_array(id) AS covered_ids
                     FROM messages
                     WHERE thread_id IN (SELECT value FROM json_each(?1))
                     GROUP BY account_id, thread_id) t"#;
        let rows = if dry_run {
            sqlx::query(SELECTION)
                .bind(&wanted)
                .fetch_all(self.reader())
                .await?
        } else {
            let sql = format!(
                "INSERT INTO desk_dismissals
                     (account_id, thread_id, through_rowid, through_date, through_message_id,
                      through_count, covered_ids, dismissed_at)
                 SELECT account_id, thread_id, 0, through_date, through_message_id,
                        through_count, covered_ids, ?2
                 FROM ({SELECTION}) WHERE true
                 ON CONFLICT(account_id, thread_id) DO UPDATE SET
                     covered_ids = excluded.covered_ids,
                     through_date = excluded.through_date,
                     through_message_id = excluded.through_message_id,
                     through_count = excluded.through_count,
                     dismissed_at = excluded.dismissed_at
                 RETURNING account_id, thread_id"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&wanted)
                .bind(Utc::now().timestamp())
                .fetch_all(self.writer())
                .await?
        };
        let mut selected = rows
            .into_iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("account_id")?)?,
                    decode_id(row.try_get::<&str, _>("thread_id")?)?,
                ))
            })
            .collect::<Result<Vec<(AccountId, ThreadId)>, sqlx::Error>>()?;
        selected.sort_by_key(|(account, thread)| (account.as_str(), thread.as_str()));
        Ok(selected)
    }

    /// Undo "done waiting": the threads show under Waiting again.
    pub async fn restore_desk_threads(&self, thread_ids: &[ThreadId]) -> Result<u64, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(0);
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let result = sqlx::query(
            "DELETE FROM desk_dismissals WHERE thread_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(wanted)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
    }
}

/// The per-message desk read over a thread selection (`active_threads`,
/// a statement returning `thread_id`s). Shared with the query-plan test.
#[derive(Clone, Copy)]
enum Evidence {
    Full,
    PendingAndTimers,
    None,
}

fn desk_messages_sql(active_threads: &str, evidence_mode: Evidence) -> String {
    // Settled homes and asks already decide ordinary lanes and geometry.
    // Raw response evidence is only needed for pending mail and explicit
    // timers, where a visibility correction cannot turn automation into an
    // answer. Timer checks must run before raw response classification.
    let gate = if matches!(evidence_mode, Evidence::PendingAndTimers) {
        format!(
            "NOT EXISTS (SELECT 1 FROM mail_assessments home
                JOIN assessment_effective_thread_inputs inputs
                    ON inputs.account_id = m.account_id AND inputs.thread_id = m.thread_id
                WHERE home.message_id = m.id AND home.policy_version = '{policy}'
                    AND home.input_revision = inputs.effective_revision)
              OR EXISTS (SELECT 1 FROM messages timed
                WHERE timed.account_id = m.account_id AND timed.thread_id = m.thread_id
                    AND (EXISTS (SELECT 1 FROM message_flags flag WHERE flag.message_id = timed.id AND flag.reply_later = 1)
                        OR EXISTS (SELECT 1 FROM auto_reminders reminder WHERE reminder.sent_message_id = timed.id)))",
            policy = mxr_core::assessment::CLASSIFIER_POLICY_VERSION
        )
    } else {
        "1".to_string()
    };
    let evidence = if !matches!(evidence_mode, Evidence::None) {
        format!(
            r#"EXISTS (SELECT 1 FROM messages sent WHERE sent.account_id = m.account_id
                    AND sent.thread_id = m.thread_id AND (sent.direction = 'outbound'
                        OR (sent.direction != 'inbound' AND EXISTS (SELECT 1 FROM account_addresses own
                            WHERE own.account_id = sent.account_id AND lower(own.email) = lower(sent.from_email))))
                    AND sent.id != m.id) AS evidence_thread_correspondence,
                CASE WHEN {gate} THEN (SELECT json_object('list_id', json_extract(metadata_json, '$.list_id'),
                    'raw_headers', substr(json_extract(metadata_json, '$.raw_headers'), 1, 131072))
                    FROM bodies WHERE message_id = m.id) END AS evidence_metadata,
                CASE WHEN {gate} THEN (SELECT substr(text_plain, 1, 8192) FROM bodies WHERE message_id = m.id) END AS evidence_text,
                CASE WHEN {gate} THEN COALESCE(
                    (SELECT substr(pb.text_plain, 1, 8192) FROM messages previous JOIN bodies pb ON pb.message_id = previous.id
                        WHERE previous.account_id = m.account_id AND previous.thread_id = m.thread_id
                        AND previous.id != m.id AND previous.message_id_header = m.in_reply_to
                        ORDER BY previous.date DESC, previous.id DESC LIMIT 1),
                    (SELECT substr(pb.text_plain, 1, 8192) FROM messages previous JOIN bodies pb ON pb.message_id = previous.id
                        WHERE previous.account_id = m.account_id AND previous.thread_id = m.thread_id
                        AND (previous.date < m.date OR (previous.date = m.date AND previous.id < m.id))
                          ORDER BY previous.date DESC, previous.id DESC LIMIT 1)) END AS evidence_previous_text"#
        )
    } else {
        "0 AS evidence_thread_correspondence, NULL AS evidence_metadata,
         NULL AS evidence_text, NULL AS evidence_previous_text"
            .to_string()
    };
    format!(
        r#"WITH active AS ({active_threads})
            SELECT
                m.rowid AS seq, m.id, m.thread_id, m.direction, m.date, m.flags,
                m.from_email, m.from_name, m.to_addrs, m.cc_addrs, m.bcc_addrs, m.subject,
                m.list_id, m.unsubscribe_method,
                m.in_reply_to AS evidence_reply,
                {evidence},
                EXISTS (
                    SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                    WHERE ml.message_id = m.id AND l.provider_id = 'INBOX'
                ) AS in_inbox,
                ((m.flags & ?2) != 0 OR EXISTS (
                    SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                    WHERE ml.message_id = m.id AND l.provider_id IN ('TRASH', 'SPAM')
                )) AS trashed,
                EXISTS (SELECT 1 FROM snoozed s WHERE s.message_id = m.id) AS snoozed,
                EXISTS (SELECT 1 FROM calendar_invites ci WHERE ci.message_id = m.id) AS is_invite,
                EXISTS (SELECT 1 FROM delivery_messages dm WHERE dm.message_id = m.id) AS is_delivery
            -- CROSS JOIN keeps the thread list as the outer loop, so each
            -- thread is a thread_id index lookup. A plain JOIN let the
            -- planner walk the account's whole mailbox instead.
            FROM active
            CROSS JOIN messages m ON m.thread_id = active.thread_id
            WHERE m.account_id = ?1
            ORDER BY m.thread_id, m.date, m.id"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::*;
    use crate::Store;
    use chrono::Duration;
    use mxr_core::types::MessageDirection;

    #[tokio::test]
    async fn written_to_follows_all_recipient_lists_and_keeps_other_supporting_messages() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        let other = test_account();
        store.insert_account(&account).await.unwrap();
        store.insert_account(&other).await.unwrap();
        let addr = |email: &str| Address {
            name: None,
            email: email.into(),
        };
        let mut sent = envelope(&account.id, &ThreadId::new(), "me@example.test", Utc::now());
        sent.to = vec![addr("To@Example.Test"), addr("same@example.test")];
        sent.cc = vec![addr("Cc@Example.Test"), addr("SAME@example.test")];
        sent.bcc = vec![addr("Bcc@Example.Test")];
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Inbound)
            .await
            .unwrap();
        let wanted = [
            "to@example.test",
            "cc@example.test",
            "bcc@example.test",
            "same@example.test",
            "never@example.test",
        ]
        .map(str::to_string);
        assert!(store
            .addresses_written_to(&account.id, &wanted)
            .await
            .unwrap()
            .is_empty());
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();
        let expected = wanted[..4].iter().cloned().collect();
        assert_eq!(
            store
                .addresses_written_to(&account.id, &wanted)
                .await
                .unwrap(),
            expected
        );
        assert!(store
            .addresses_written_to(&other.id, &wanted)
            .await
            .unwrap()
            .is_empty());

        let mut second = sent.clone();
        second.id = MessageId::new();
        second.provider_id = "second-sent".into();
        store
            .upsert_envelope_with_direction(&second, MessageDirection::Outbound)
            .await
            .unwrap();
        sent.to.clear();
        sent.cc.clear();
        sent.bcc.clear();
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();
        assert_eq!(
            store
                .addresses_written_to(&account.id, &wanted)
                .await
                .unwrap(),
            expected
        );
        sqlx::query("DELETE FROM messages WHERE id=?1")
            .bind(second.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        assert!(store
            .addresses_written_to(&account.id, &wanted)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn written_to_moves_with_the_account_and_disappears_when_direction_changes() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        let other = test_account();
        store.insert_account(&account).await.unwrap();
        store.insert_account(&other).await.unwrap();
        let mut sent = envelope(&account.id, &ThreadId::new(), "me@example.test", Utc::now());
        sent.to = vec![Address {
            name: None,
            email: "maya@example.test".into(),
        }];
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();
        let wanted = ["maya@example.test".into()];
        sqlx::query("UPDATE messages SET account_id=?1 WHERE id=?2")
            .bind(other.id.as_str())
            .bind(sent.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        assert!(store
            .addresses_written_to(&account.id, &wanted)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .addresses_written_to(&other.id, &wanted)
                .await
                .unwrap(),
            wanted.iter().cloned().collect()
        );
        sqlx::query("UPDATE messages SET direction='unknown' WHERE id=?1")
            .bind(sent.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        assert!(store
            .addresses_written_to(&other.id, &wanted)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn navigation_upgrade_backfills_previously_sent_recipients() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        sqlx::raw_sql("DROP TRIGGER outbound_recipients_insert; DROP TRIGGER outbound_recipients_update; DROP VIEW outbound_recipient_projection; DROP TABLE outbound_recipients; DROP INDEX idx_messages_sender_history;").execute(store.writer()).await.unwrap();
        let mut sent = envelope(&account.id, &ThreadId::new(), "me@example.test", Utc::now());
        sent.to = vec![Address {
            name: None,
            email: "Earlier@Example.Test".into(),
        }];
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/103_navigation_lookups.sql"))
            .execute(store.writer())
            .await
            .unwrap();
        let wanted = ["earlier@example.test".into()];
        assert_eq!(
            store
                .addresses_written_to(&account.id, &wanted)
                .await
                .unwrap(),
            wanted.iter().cloned().collect()
        );
    }

    fn envelope(
        account: &AccountId,
        thread: &ThreadId,
        from: &str,
        at: DateTime<Utc>,
    ) -> mxr_core::types::Envelope {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        envelope.provider_id = format!("p-{}", envelope.id);
        envelope.thread_id = thread.clone();
        envelope.from.email = from.into();
        envelope.date = at;
        envelope
    }

    /// Each selected thread must be an index lookup; letting the planner
    /// walk the account's mailbox took half a second per desk on a 110k
    /// message store. Same for the watched-contact lookups by date.
    #[tokio::test]
    async fn desk_reads_look_threads_and_contacts_up_by_index() {
        let store = Store::in_memory().await.unwrap();
        let plan = |sql: String| {
            let store = &store;
            async move {
                sqlx::query(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
                    .fetch_all(store.reader())
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|row| row.get::<String, _>("detail"))
                    .collect::<Vec<_>>()
            }
        };
        let threads = plan(desk_messages_sql(
            "SELECT value AS thread_id FROM json_each(?3)",
            Evidence::Full,
        ))
        .await;
        assert!(
            threads
                .iter()
                .any(|step| step.starts_with("SEARCH m USING ")
                    && step.contains("INDEX ")
                    && step.contains("(account_id=? AND thread_id=?)")),
            "selected messages must use account and thread: {threads:#?}"
        );
        crate::places::tests::assert_thread_lookups_use_the_thread(&threads);
    }

    #[tokio::test]
    async fn thread_messages_return_whole_active_threads_with_state() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let now = Utc::now();
        let active_thread = ThreadId::new();
        let old = envelope(
            &account.id,
            &active_thread,
            "maya@example.com",
            now - Duration::days(90),
        );
        let recent = envelope(
            &account.id,
            &active_thread,
            "maya@example.com",
            now - Duration::hours(3),
        );
        let stale = envelope(
            &account.id,
            &ThreadId::new(),
            "jon@example.com",
            now - Duration::days(60),
        );
        for envelope in [&old, &recent, &stale] {
            store
                .upsert_envelope_with_direction(envelope, MessageDirection::Inbound)
                .await
                .unwrap();
        }

        let rows = store
            .desk_thread_messages(&account.id, now - Duration::days(30))
            .await
            .unwrap();
        // The old message comes with its recently active thread; the stale
        // thread stays out.
        assert_eq!(
            rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
            vec![old.id.clone(), recent.id.clone()]
        );
        assert!(rows.iter().all(|row| row.direction == "inbound"));
        assert!(rows.iter().all(|row| !row.snoozed && !row.trashed));
    }

    #[tokio::test]
    async fn conversation_history_skips_only_settled_non_conversations_and_keeps_whole_threads() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = Utc::now();
        let person_thread = ThreadId::new();
        let automated = envelope(&account.id, &ThreadId::new(), "robot@example.test", now);
        let old = envelope(
            &account.id,
            &person_thread,
            "maya@example.test",
            now - Duration::days(90),
        );
        let person = envelope(&account.id, &person_thread, "maya@example.test", now);
        let sent = envelope(&account.id, &ThreadId::new(), "me@example.test", now);
        let stale = envelope(&account.id, &ThreadId::new(), "changed@example.test", now);
        for mail in [&automated, &old, &person, &sent, &stale] {
            store
                .upsert_envelope_with_direction(
                    mail,
                    if mail.id == sent.id {
                        MessageDirection::Outbound
                    } else {
                        MessageDirection::Inbound
                    },
                )
                .await
                .unwrap();
        }
        for (mail, home, stale_revision) in [
            (&automated, "updates", false),
            (&old, "updates", false),
            (&person, "messages", false),
            (&stale, "updates", true),
        ] {
            sqlx::query("INSERT INTO mail_assessments(message_id,account_id,policy_version,input_fingerprint,input_revision,assessment_json) SELECT ?1,?2,?3,'fixture',CASE WHEN ?5 THEN -1 ELSE effective_revision END,json_object('home',?4) FROM assessment_effective_thread_inputs WHERE account_id=?2 AND thread_id=?6")
                .bind(mail.id.as_str()).bind(account.id.as_str()).bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
                .bind(home).bind(stale_revision).bind(mail.thread_id.as_str()).execute(store.writer()).await.unwrap();
        }
        for mail in [&automated, &old, &person, &sent, &stale] {
            store
                .insert_body(&mxr_core::MessageBody {
                    message_id: mail.id.clone(),
                    text_plain: Some(
                        "Body evidence must survive for conversations and pending mail.".into(),
                    ),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: now,
                    metadata: Default::default(),
                })
                .await
                .unwrap();
        }
        // Body insertion changes the input generation. Restore the settled
        // fixtures, but leave the deliberately stale home stale.
        sqlx::query("UPDATE mail_assessments SET input_revision = (SELECT effective_revision FROM assessment_effective_thread_inputs i JOIN messages m ON m.account_id=i.account_id AND m.thread_id=i.thread_id WHERE m.id=mail_assessments.message_id) WHERE message_id != ?1")
            .bind(stale.id.as_str()).execute(store.writer()).await.unwrap();
        let recent = store
            .desk_thread_messages(&account.id, now - Duration::days(1))
            .await
            .unwrap();
        for row in recent {
            assert_eq!(
                row.evidence.text.is_some(),
                row.id == stale.id || row.id == sent.id,
                "settled timer-free mail needs only its stored facets: {}",
                row.id
            );
        }
        let rows = store
            .desk_conversation_messages(&account.id, now - Duration::days(1))
            .await
            .unwrap();
        let ids: std::collections::HashSet<_> = rows.iter().map(|m| &m.id).collect();
        assert_eq!(
            ids,
            [&old.id, &person.id, &sent.id, &stale.id]
                .into_iter()
                .collect()
        );
        assert!(!ids.contains(&automated.id));
        store.set_reply_later(&automated.id, now).await.unwrap();
        let timed = store
            .desk_thread_messages(&account.id, now - Duration::days(1))
            .await
            .unwrap();
        assert!(
            timed
                .iter()
                .find(|m| m.id == automated.id)
                .unwrap()
                .evidence
                .text
                .is_some(),
            "timer rules still need human-response evidence on non-conversation mail"
        );
        sqlx::query("UPDATE assessment_sender_inputs SET revision = revision + 100000 WHERE account_id = ?1 AND email = ?2")
            .bind(account.id.as_str()).bind(&automated.from.email).execute(store.writer()).await.unwrap();
        let changed = store
            .desk_conversation_messages(&account.id, now - Duration::days(1))
            .await
            .unwrap();
        assert!(
            changed.iter().any(|m| m.id == automated.id),
            "sender-only invalidation must bring a previously settled non-conversation back"
        );
    }

    #[tokio::test]
    async fn dismissals_cover_the_thread_through_its_newest_message() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = Utc::now();
        let thread = ThreadId::new();
        let sent = envelope(
            &account.id,
            &thread,
            "me@example.com",
            now - Duration::days(2),
        );
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();

        let preview = store
            .dismiss_desk_threads(std::slice::from_ref(&thread), true)
            .await
            .unwrap();
        assert_eq!(preview, vec![(account.id.clone(), thread.clone())]);
        assert!(store.desk_dismissals(&account.id).await.unwrap().is_empty());

        let done = store
            .dismiss_desk_threads(&[thread.clone(), ThreadId::new()], false)
            .await
            .unwrap();
        assert_eq!(done, preview, "unknown threads are skipped");
        let dismissals = store.desk_dismissals(&account.id).await.unwrap();
        let thread_messages = store
            .desk_messages_in_threads(&account.id, std::slice::from_ref(&thread))
            .await
            .unwrap();
        assert!(dismissals[&thread].covers(&thread_messages));

        // Dismissing again is an upsert, and restoring clears it.
        store
            .dismiss_desk_threads(std::slice::from_ref(&thread), false)
            .await
            .unwrap();
        assert_eq!(
            store
                .restore_desk_threads(std::slice::from_ref(&thread))
                .await
                .unwrap(),
            1
        );
        assert!(store.desk_dismissals(&account.id).await.unwrap().is_empty());

        // A message stored later ends a dismissal, even with a Date older
        // than the rest of the thread (a delayed delivery).
        store
            .dismiss_desk_threads(std::slice::from_ref(&thread), false)
            .await
            .unwrap();
        let late = envelope(
            &account.id,
            &thread,
            "jon@example.com",
            now - Duration::days(30),
        );
        store
            .upsert_envelope_with_direction(&late, MessageDirection::Inbound)
            .await
            .unwrap();
        let dismissals = store.desk_dismissals(&account.id).await.unwrap();
        let thread_messages = store
            .desk_messages_in_threads(&account.id, std::slice::from_ref(&thread))
            .await
            .unwrap();
        assert!(!dismissals[&thread].covers(&thread_messages));
    }

    #[tokio::test]
    async fn the_date_watermark_migration_backfills_rowid_marks() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = Utc::now();
        let thread = ThreadId::new();
        let first = envelope(
            &account.id,
            &thread,
            "maya@example.com",
            now - Duration::days(2),
        );
        let second = envelope(
            &account.id,
            &thread,
            "maya@example.com",
            now - Duration::days(1),
        );
        for message in [&first, &second] {
            store
                .upsert_envelope_with_direction(message, MessageDirection::Inbound)
                .await
                .unwrap();
        }
        // A mark as the rowid watermark wrote it, through the first message.
        let first_rowid: i64 = sqlx::query_scalar("SELECT rowid FROM messages WHERE id = ?1")
            .bind(first.id.as_str())
            .fetch_one(store.reader())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO desk_dismissals
                 (account_id, thread_id, through_rowid, through_count, dismissed_at)
             VALUES (?1, ?2, ?3, 1, 0)",
        )
        .bind(account.id.as_str())
        .bind(thread.as_str())
        .bind(first_rowid)
        .execute(store.writer())
        .await
        .unwrap();

        sqlx::raw_sql(include_str!("../migrations/062_done_watermark_by_date.sql"))
            .execute(store.writer())
            .await
            .unwrap();
        let mark = store.desk_dismissals(&account.id).await.unwrap()[&thread].clone();
        assert_eq!(mark.through_date, first.date.timestamp());
        assert_eq!(mark.through_id, *first.id.as_uuid());
        let messages = store
            .desk_messages_in_threads(&account.id, std::slice::from_ref(&thread))
            .await
            .unwrap();
        assert!(
            !mark.covers(&messages),
            "the second message came after the mark"
        );
        assert!(mark.saw(messages[0].date, &messages[0].id));

        // Running it again leaves a backfilled or new mark alone.
        sqlx::raw_sql(include_str!("../migrations/062_done_watermark_by_date.sql"))
            .execute(store.writer())
            .await
            .unwrap();
        assert_eq!(
            store.desk_dismissals(&account.id).await.unwrap()[&thread],
            mark
        );
    }
}
