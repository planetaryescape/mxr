use crate::{decode_id, decode_timestamp, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use std::collections::HashMap;

use crate::message::{future_date_cutoff_timestamp, record_to_envelope};

impl super::Store {
    pub async fn get_thread(&self, thread_id: &ThreadId) -> Result<Option<Thread>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let mut batch = self
            .get_threads_batch(std::slice::from_ref(thread_id))
            .await?;
        trace_lookup("thread.get_thread", started_at, !batch.is_empty());
        Ok(batch.pop())
    }

    /// Hydrate Thread rows in bulk. Ids with zero matching messages
    /// are silently skipped — callers that need tombstone Threads for
    /// missing ids (typically the sync engine emitting
    /// `threads_changed`) must synthesise them using their own
    /// account_id context.
    ///
    /// Three round-trips regardless of batch size: aggregate, distinct
    /// participants, and date-ordered member ids. Bound by SQLite's
    /// `SQLITE_MAX_VARIABLE_NUMBER` only insofar as `thread_ids` is
    /// serialised to a JSON array and parsed via `json_each`.
    pub async fn get_threads_batch(
        &self,
        thread_ids: &[ThreadId],
    ) -> Result<Vec<Thread>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(vec![]);
        }
        let cutoff = future_date_cutoff_timestamp();
        let ids_json =
            serde_json::to_string(&thread_ids.iter().map(|t| t.as_str()).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let started_at = std::time::Instant::now();
        let aggregate_rows = sqlx::query!(
            r#"SELECT
                thread_id as "thread_id!",
                account_id as "account_id!",
                MIN(subject) as "subject!: String",
                COUNT(*) as "message_count!: i64",
                SUM(CASE WHEN (flags & 1) = 0 THEN 1 ELSE 0 END) as "unread_count!: i64",
                MAX(CASE WHEN date > ? THEN 0 ELSE date END) as "latest_date!: i64",
                snippet as "snippet!"
             FROM messages
             WHERE thread_id IN (SELECT value FROM json_each(?))
             GROUP BY thread_id"#,
            cutoff,
            ids_json,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "thread.get_threads_batch.aggregate",
            started_at,
            aggregate_rows.len(),
        );

        let started_at = std::time::Instant::now();
        let participant_rows = sqlx::query!(
            r#"SELECT DISTINCT
                thread_id as "thread_id!",
                from_name,
                from_email as "from_email!"
             FROM messages
             WHERE thread_id IN (SELECT value FROM json_each(?))"#,
            ids_json,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "thread.get_threads_batch.participants",
            started_at,
            participant_rows.len(),
        );

        let mut participants_by_thread: HashMap<String, Vec<Address>> = HashMap::new();
        for row in participant_rows {
            participants_by_thread
                .entry(row.thread_id)
                .or_default()
                .push(Address {
                    name: row.from_name,
                    email: row.from_email,
                });
        }

        let started_at = std::time::Instant::now();
        let id_rows = sqlx::query!(
            r#"SELECT
                thread_id as "thread_id!",
                id as "id!"
             FROM messages
             WHERE thread_id IN (SELECT value FROM json_each(?))
             ORDER BY thread_id ASC,
                      CASE WHEN date > ? THEN 0 ELSE date END ASC,
                      id ASC"#,
            ids_json,
            cutoff,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "thread.get_threads_batch.message_ids",
            started_at,
            id_rows.len(),
        );

        let mut message_ids_by_thread: HashMap<String, Vec<MessageId>> = HashMap::new();
        for row in id_rows {
            message_ids_by_thread
                .entry(row.thread_id)
                .or_default()
                .push(decode_id(&row.id)?);
        }

        aggregate_rows
            .into_iter()
            .map(|row| {
                let thread_id_str = row.thread_id.clone();
                Ok(Thread {
                    id: decode_id(&row.thread_id)?,
                    account_id: decode_id(&row.account_id)?,
                    subject: row.subject,
                    participants: participants_by_thread
                        .remove(&thread_id_str)
                        .unwrap_or_default(),
                    message_count: row.message_count as u32,
                    unread_count: row.unread_count as u32,
                    latest_date: decode_timestamp(row.latest_date)?,
                    snippet: row.snippet,
                    message_ids: message_ids_by_thread
                        .remove(&thread_id_str)
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Paginated list of threads. Two-stage: SELECT the matching
    /// thread_ids with the filter + sort + LIMIT/OFFSET, then call
    /// `get_threads_batch` to hydrate. `label_id` matches threads
    /// where ANY constituent message carries that label. `sort`
    /// `Relevance` falls back to `DateDesc` (latest message first).
    pub async fn list_threads(
        &self,
        account_id: Option<&AccountId>,
        label_id: Option<&LabelId>,
        limit: u32,
        offset: u32,
        sort: SortOrder,
    ) -> Result<Vec<Thread>, sqlx::Error> {
        let cutoff = future_date_cutoff_timestamp();
        let account_filter = account_id.map(|a| a.as_str().to_string());
        let label_filter = label_id.map(|l| l.as_str().to_string());
        let limit_i = limit as i64;
        let offset_i = offset as i64;

        let started_at = std::time::Instant::now();
        let id_rows = match sort {
            SortOrder::DateAsc => sqlx::query!(
                r#"SELECT thread_id as "thread_id!"
                       FROM messages
                       WHERE (?1 IS NULL OR account_id = ?1)
                         AND (?2 IS NULL OR id IN (
                             SELECT ml.message_id FROM message_labels ml
                             JOIN labels l ON l.id = ml.label_id
                             WHERE l.id = ?2
                         ))
                       GROUP BY thread_id
                       ORDER BY MAX(CASE WHEN date > ?3 THEN 0 ELSE date END) ASC
                       LIMIT ?4 OFFSET ?5"#,
                account_filter,
                label_filter,
                cutoff,
                limit_i,
                offset_i,
            )
            .fetch_all(self.reader())
            .await?
            .into_iter()
            .map(|r| r.thread_id)
            .collect::<Vec<_>>(),
            // DateDesc | Relevance (Relevance has no defined meaning
            // for thread listing — fall back to "most recent first").
            _ => sqlx::query!(
                r#"SELECT thread_id as "thread_id!"
                   FROM messages
                   WHERE (?1 IS NULL OR account_id = ?1)
                     AND (?2 IS NULL OR id IN (
                         SELECT ml.message_id FROM message_labels ml
                         JOIN labels l ON l.id = ml.label_id
                         WHERE l.id = ?2
                     ))
                   GROUP BY thread_id
                   ORDER BY MAX(CASE WHEN date > ?3 THEN 0 ELSE date END) DESC
                   LIMIT ?4 OFFSET ?5"#,
                account_filter,
                label_filter,
                cutoff,
                limit_i,
                offset_i,
            )
            .fetch_all(self.reader())
            .await?
            .into_iter()
            .map(|r| r.thread_id)
            .collect::<Vec<_>>(),
        };
        trace_query("thread.list_threads.ids", started_at, id_rows.len());

        let thread_ids: Vec<ThreadId> = id_rows
            .iter()
            .map(|s| decode_id::<ThreadId>(s))
            .collect::<Result<Vec<_>, _>>()?;

        let mut threads = self.get_threads_batch(&thread_ids).await?;
        // get_threads_batch returns in arbitrary order; restore the
        // SQL sort order before returning.
        threads.sort_by_key(|t| {
            thread_ids
                .iter()
                .position(|id| id == &t.id)
                .unwrap_or(usize::MAX)
        });
        Ok(threads)
    }

    pub async fn get_thread_envelopes(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let tid = thread_id.as_str();
        let cutoff = future_date_cutoff_timestamp();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT
                id as "id!", account_id as "account_id!", provider_id as "provider_id!",
                thread_id as "thread_id!", message_id_header, in_reply_to,
                reference_headers, from_name, from_email as "from_email!",
                to_addrs as "to_addrs!", cc_addrs as "cc_addrs!", bcc_addrs as "bcc_addrs!",
                subject as "subject!", date as "date!", flags as "flags!",
                snippet as "snippet!", has_attachments as "has_attachments!: bool",
                size_bytes as "size_bytes!", unsubscribe_method,
                COALESCE((
                    SELECT GROUP_CONCAT(labels.provider_id, char(31))
                    FROM message_labels
                    JOIN labels ON labels.id = message_labels.label_id
                    WHERE message_labels.message_id = messages.id
                ), '') as "label_provider_ids!: String"
             FROM messages
             WHERE thread_id = ?
             ORDER BY CASE WHEN date > ? THEN 0 ELSE date END ASC, id ASC"#,
            tid,
            cutoff,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("thread.get_thread_envelopes", started_at, rows.len());

        rows.into_iter()
            .map(|r| {
                record_to_envelope(
                    &r.id,
                    &r.account_id,
                    &r.provider_id,
                    &r.thread_id,
                    r.message_id_header.as_deref(),
                    r.in_reply_to.as_deref(),
                    r.reference_headers.as_deref(),
                    r.from_name.as_deref(),
                    &r.from_email,
                    &r.to_addrs,
                    &r.cc_addrs,
                    &r.bcc_addrs,
                    &r.subject,
                    r.date,
                    r.flags,
                    &r.snippet,
                    r.has_attachments,
                    r.size_bytes,
                    r.unsubscribe_method.as_deref(),
                    &r.label_provider_ids,
                )
            })
            .collect()
    }
}
