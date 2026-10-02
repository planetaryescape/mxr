//! Deleting messages together with everything derived from them.
//!
//! The invariant: deleting an email deletes everything derived from it.
//! Most per-message tables reach it through `ON DELETE CASCADE` on
//! `messages(id)`. The tables below hold text built from mail but are keyed
//! by thread, contact or a JSON list of ids, which a cascade cannot follow,
//! so [`Store::delete_messages_and_derived`] clears them in the same
//! transaction as the message rows. `MESSAGE_DELETION_RULES` records the
//! rule of every table that refers to messages without a cascade; the
//! schema test fails for a new table that has neither.

use crate::decode_id;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::Row;

/// How a table that refers to messages without a cascading foreign key
/// follows a message delete. Only the schema test reads it; the variants and
/// reasons are the record a new table's author adds to.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageDeletionRule {
    /// Cleared by [`Store::delete_messages_and_derived`].
    ClearedWithMessages,
    /// Cleared when its contact is pruned for having no mail left
    /// (`Store::refresh_contacts`).
    ClearedWithContact,
    /// Holds ids, hashes or counts only, never text from the message, and is
    /// kept on purpose. The reason says why.
    KeptNoText(&'static str),
    /// Made by the user, not derived from the message, so it outlives it.
    KeptUserMade(&'static str),
}

/// Every table that refers to messages (by id, thread or evidence list) and
/// has no `ON DELETE CASCADE` to `messages`. Checked against the live schema
/// by `every_table_referring_to_messages_has_a_deletion_rule`.
#[cfg(test)]
pub(crate) const MESSAGE_DELETION_RULES: &[(&str, MessageDeletionRule)] = &[
    (
        "context_briefings",
        MessageDeletionRule::ClearedWithMessages,
    ),
    ("thread_summaries", MessageDeletionRule::ClearedWithMessages),
    ("decision_log", MessageDeletionRule::ClearedWithMessages),
    (
        "contact_commitments",
        MessageDeletionRule::ClearedWithMessages,
    ),
    ("deliveries", MessageDeletionRule::ClearedWithMessages),
    ("desk_dismissals", MessageDeletionRule::ClearedWithMessages),
    ("event_log", MessageDeletionRule::ClearedWithMessages),
    // Cleared for every person a delete touches, and with its contact.
    (
        "contact_relationship_summary",
        MessageDeletionRule::ClearedWithMessages,
    ),
    ("contact_style", MessageDeletionRule::ClearedWithContact),
    (
        "pending_message_forgets",
        MessageDeletionRule::KeptNoText(
            "ids of deleted messages awaiting cleanup outside SQLite; the daemon drains it",
        ),
    ),
    (
        "search_reindex_pending",
        MessageDeletionRule::KeptNoText(
            "ids only; the next reindex drops a deleted message from search and clears its row",
        ),
    ),
    (
        "drafts",
        MessageDeletionRule::KeptUserMade(
            "the user's own writing; `message_id_header` is the draft's own header",
        ),
    ),
    (
        "mutation_dedup_log",
        MessageDeletionRule::KeptNoText("provider ids in a 24h retry window that expires"),
    ),
    (
        "rule_execution_log",
        MessageDeletionRule::KeptNoText("rule id, rule name and actions applied"),
    ),
    (
        "sent_draft_receipts",
        MessageDeletionRule::KeptNoText("ids a sent draft became"),
    ),
    (
        "todo_runs",
        MessageDeletionRule::KeptNoText(
            "the first run's newest-first position: an id and a date, never text",
        ),
    ),
];

/// Failed cleanups of a deleted message's files before it is left alone.
pub const MAX_FORGET_ATTEMPTS: u32 = 10;

/// What a delete removed, for the caller to clear outside SQLite.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeletedMessages {
    pub message_ids: Vec<MessageId>,
    pub thread_ids: Vec<ThreadId>,
    /// The people the deleted mail was with. Their relationship data was
    /// built partly from it and is worth recomputing.
    pub counterparties: Vec<(AccountId, String)>,
}

impl super::Store {
    /// Deletes the account's messages with these provider ids and every row
    /// derived from them, in one transaction.
    ///
    /// Per thread or contact caches that may quote the deleted mail (thread
    /// gists and summaries, recipient briefings) are dropped and regenerate
    /// on demand. Extracted facts are dropped when no source message is
    /// left: a commitment's single evidence message, a decision's whole
    /// evidence list, a delivery's whole provenance.
    pub async fn delete_messages_and_derived(
        &self,
        account_id: &AccountId,
        provider_ids: &[String],
    ) -> Result<DeletedMessages, sqlx::Error> {
        if provider_ids.is_empty() {
            return Ok(DeletedMessages::default());
        }
        // Read before the transaction: the counterparties come from rows
        // about to be deleted, and the lookup needs the reader pool, which
        // may share the writer's only connection (in-memory stores).
        let mut preview = Vec::new();
        for chunk in provider_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            preview.extend(
                provider_id_query("SELECT id FROM messages", account_id, chunk)
                    .fetch_all(self.reader())
                    .await?
                    .iter()
                    .map(|row| decode_id::<MessageId>(&row.get::<String, _>("id")))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if preview.is_empty() {
            return Ok(DeletedMessages::default());
        }
        let counterparties = self.relationship_contacts_for_messages(&preview).await?;

        let account = account_id.as_str();
        let mut tx = self.writer().begin().await?;
        for statement in [
            "CREATE TEMP TABLE mxr_deleting (id TEXT PRIMARY KEY, thread_id TEXT NOT NULL)",
            "CREATE TEMP TABLE mxr_deleting_people (email TEXT PRIMARY KEY)",
        ] {
            sqlx::query(statement).execute(&mut *tx).await?;
        }
        // Resolved again inside the transaction, so what is deleted is
        // exactly what the database holds now.
        for chunk in provider_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            provider_id_query(
                "INSERT OR IGNORE INTO temp.mxr_deleting (id, thread_id) SELECT id, thread_id FROM messages",
                account_id,
                chunk,
            )
            .execute(&mut *tx)
            .await?;
        }
        for (_, email) in &counterparties {
            // Lowercased as recipient briefings key it; SQL `lower()` folds
            // ASCII only.
            sqlx::query("INSERT OR IGNORE INTO temp.mxr_deleting_people (email) VALUES (?)")
                .bind(email.to_lowercase())
                .execute(&mut *tx)
                .await?;
        }
        let message_ids = sqlx::query_scalar::<_, String>("SELECT id FROM temp.mxr_deleting")
            .fetch_all(&mut *tx)
            .await?
            .iter()
            .map(|id| decode_id::<MessageId>(id))
            .collect::<Result<Vec<_>, _>>()?;
        let thread_ids =
            sqlx::query_scalar::<_, String>("SELECT DISTINCT thread_id FROM temp.mxr_deleting")
                .fetch_all(&mut *tx)
                .await?
                .iter()
                .map(|id| decode_id::<ThreadId>(id))
                .collect::<Result<Vec<_>, _>>()?;

        // Statements bind `?1` to the account. Order matters: the
        // provenance and evidence lookups read rows the message delete
        // cascades away, so they are captured first and checked after.
        for statement in [
            // Thread gists and summaries can quote any message of the
            // thread, so a partial delete drops them too.
            // `GetThreadBriefing` keys on the bare thread id, the gist on
            // `gist:<thread id>`.
            "DELETE FROM context_briefings WHERE account_id = ?1 AND kind = 'thread'
               AND (subject_key IN (SELECT thread_id FROM temp.mxr_deleting)
                    OR subject_key IN (SELECT 'gist:' || thread_id FROM temp.mxr_deleting))",
            "DELETE FROM context_briefings WHERE account_id = ?1 AND kind = 'recipient'
               AND subject_key IN (SELECT email FROM temp.mxr_deleting_people)",
            // A relationship summary is model text drawn from the person's
            // mail. A refresh skips rewriting one when no newer mail exists,
            // so it is dropped here and rebuilt from the mail that is left.
            "DELETE FROM contact_relationship_summary WHERE account_id = ?1
               AND email IN (SELECT email FROM temp.mxr_deleting_people)",
            // Moves the version a recipient briefing in flight read, so its
            // late write is refused. `refreshed_at` is otherwise written
            // only by the contacts refresh, which sets it afresh.
            "UPDATE contacts SET refreshed_at = refreshed_at - 1
               WHERE account_id = ?1
                 AND (email IN (SELECT email FROM temp.mxr_deleting_people)
                      OR LOWER(email) IN (SELECT email FROM temp.mxr_deleting_people))",
            "DELETE FROM thread_summaries WHERE account_id = ?1
               AND thread_id IN (SELECT thread_id FROM temp.mxr_deleting)",
            "DELETE FROM contact_commitments WHERE account_id = ?1
               AND evidence_msg_id IN (SELECT id FROM temp.mxr_deleting)",
            "DELETE FROM event_log WHERE message_id IN (SELECT id FROM temp.mxr_deleting)",
            // A detected to-do goes with its email. One the user made or
            // edited is theirs and stays, without the words it copied from
            // the email; the foreign keys then clear its pointers to it.
            concat!(
                "DELETE FROM todos WHERE account_id = ?1
                   AND source_message_id IN (SELECT id FROM temp.mxr_deleting)
                   AND scheduled_for IS NULL AND ",
                todo_untouched_sql!("")
            ),
            "UPDATE todos SET due_words = NULL, action_url = NULL, action_gate = NULL,
                    looks_done_reason = NULL, reason = 'Its email was deleted.',
                    field_sources = COALESCE(
                        (SELECT json_group_object(key, json_remove(value, '$.evidence'))
                         FROM json_each(todos.field_sources)), '{}')
               WHERE account_id = ?1
                 AND source_message_id IN (SELECT id FROM temp.mxr_deleting)",
            "UPDATE todos SET looks_done_reason = NULL
               WHERE account_id = ?1
                 AND looks_done_message_id IN (SELECT id FROM temp.mxr_deleting)",
            "CREATE TEMP TABLE mxr_deleting_deliveries AS
               SELECT DISTINCT delivery_id AS id FROM delivery_messages
               WHERE message_id IN (SELECT id FROM temp.mxr_deleting)",
            "CREATE TEMP TABLE mxr_deleting_decisions AS
               SELECT DISTINCT decision_id AS id FROM decision_evidence
               WHERE message_id IN (SELECT id FROM temp.mxr_deleting)",
            // Recorded with the delete so the cleanup outside SQLite
            // survives a failed pass or a crash before the daemon runs it.
            "INSERT OR IGNORE INTO pending_message_forgets (message_id, account_id, deleted_at)
               SELECT id, ?1, unixepoch() FROM temp.mxr_deleting",
            "DELETE FROM messages WHERE id IN (SELECT id FROM temp.mxr_deleting)",
            "DELETE FROM deliveries
               WHERE id IN (SELECT id FROM temp.mxr_deleting_deliveries)
                 AND NOT EXISTS (
                     SELECT 1 FROM delivery_messages
                     WHERE delivery_messages.delivery_id = deliveries.id)",
            // A decision survives while any of its evidence is still here;
            // the message delete cascaded the rows of the evidence that went.
            "DELETE FROM decision_log
               WHERE id IN (SELECT id FROM temp.mxr_deleting_decisions)
                 AND NOT EXISTS (
                     SELECT 1 FROM decision_evidence
                     WHERE decision_evidence.decision_id = decision_log.id)",
            "DELETE FROM desk_dismissals WHERE account_id = ?1
               AND thread_id IN (SELECT thread_id FROM temp.mxr_deleting)
               AND NOT EXISTS (
                   SELECT 1 FROM messages WHERE messages.thread_id = desk_dismissals.thread_id)",
            "DROP TABLE temp.mxr_deleting",
            "DROP TABLE temp.mxr_deleting_people",
            "DROP TABLE temp.mxr_deleting_deliveries",
            "DROP TABLE temp.mxr_deleting_decisions",
        ] {
            let query = sqlx::query(statement);
            let query = if statement.contains("?1") {
                query.bind(&account)
            } else {
                query
            };
            query.execute(&mut *tx).await?;
        }
        tx.commit().await?;

        Ok(DeletedMessages {
            message_ids,
            thread_ids,
            counterparties,
        })
    }
}

impl super::Store {
    /// Deleted messages whose cleanup outside SQLite (semantic index,
    /// attachment files) is due: not yet confirmed, not waiting out a retry
    /// backoff, and not given up on. Oldest first.
    pub async fn list_pending_message_forgets(
        &self,
        limit: u32,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT message_id FROM pending_message_forgets
             WHERE retry_after <= unixepoch() AND attempts < ?
             ORDER BY deleted_at, message_id LIMIT ?",
        )
        .bind(i64::from(MAX_FORGET_ATTEMPTS))
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?
        .iter()
        .map(|id| decode_id(id))
        .collect()
    }

    /// Records a failed cleanup: it is retried after a backoff that doubles
    /// per attempt (one minute, capped at six hours), and not after
    /// `MAX_FORGET_ATTEMPTS` failures. Returns how many hit the cap now.
    pub async fn record_failed_message_forgets(
        &self,
        message_ids: &[MessageId],
    ) -> Result<u64, sqlx::Error> {
        let mut gave_up = 0;
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let ids = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "UPDATE pending_message_forgets
                 SET attempts = attempts + 1,
                     retry_after = unixepoch() + MIN(60 * (1 << MIN(attempts, 10)), 21600)
                 WHERE message_id IN ({ids})
                 RETURNING attempts"
            );
            let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            gave_up += query
                .fetch_all(self.writer())
                .await?
                .into_iter()
                .filter(|attempts| *attempts >= i64::from(MAX_FORGET_ATTEMPTS))
                .count() as u64;
        }
        Ok(gave_up)
    }

    /// Held by the daemon from checking that a deleted message's id is
    /// still absent until its files and index entries are gone, and by
    /// `apply_sync_upserts` while it stores messages. Without it a sync could
    /// store a new message under that id between the check and the removal,
    /// and the cleanup would take the new message's files.
    pub async fn lock_message_cleanup(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.message_cleanup.lock().await
    }

    /// Which of these message ids name a stored message. An IMAP id is
    /// derived from account, folder and UID, so a deleted message's id can
    /// come back for a new message; cleanup owed to the old one must not
    /// touch it.
    pub async fn existing_message_ids(
        &self,
        message_ids: &[MessageId],
    ) -> Result<std::collections::HashSet<MessageId>, sqlx::Error> {
        let mut existing = std::collections::HashSet::new();
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "SELECT id FROM messages WHERE id IN ({})",
                vec!["?"; chunk.len()].join(", ")
            );
            let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for id in query.fetch_all(self.reader()).await? {
                existing.insert(decode_id(&id)?);
            }
        }
        Ok(existing)
    }

    /// Marks the cleanup of these deleted messages as done.
    pub async fn clear_pending_message_forgets(
        &self,
        message_ids: &[MessageId],
    ) -> Result<(), sqlx::Error> {
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "DELETE FROM pending_message_forgets WHERE message_id IN ({})",
                vec!["?"; chunk.len()].join(", ")
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            query.execute(self.writer()).await?;
        }
        Ok(())
    }
}

/// `<select> WHERE account_id = ? AND provider_id IN (<chunk>)`, bound.
fn provider_id_query<'q>(
    select: &str,
    account_id: &AccountId,
    chunk: &'q [String],
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    let sql = format!(
        "{select} WHERE account_id = ? AND provider_id IN ({})",
        vec!["?"; chunk.len()].join(", ")
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(account_id.as_str());
    for provider_id in chunk {
        query = query.bind(provider_id);
    }
    query
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use mxr_core::types::{Address, MessageDirection};
    use sqlx::Row;
    use std::collections::{BTreeMap, BTreeSet};

    async fn inbound(
        store: &Store,
        account_id: &AccountId,
        provider_id: &str,
        thread_id: &ThreadId,
        from: &str,
    ) -> MessageId {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .build();
        envelope.provider_id = provider_id.to_string();
        envelope.thread_id = thread_id.clone();
        envelope.from = Address {
            name: None,
            email: from.to_string(),
        };
        store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        envelope.id
    }

    async fn exec(store: &Store, sql: &str, binds: &[&str]) {
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for bind in binds {
            query = query.bind(*bind);
        }
        query.execute(store.writer()).await.unwrap();
    }

    async fn ids(store: &Store, sql: &str) -> BTreeSet<String> {
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
            .fetch_all(store.reader())
            .await
            .unwrap()
            .into_iter()
            .collect()
    }

    fn set(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    /// Thread A keeps one of its two messages; thread B loses its only one.
    #[tokio::test]
    async fn deleting_mail_clears_what_was_derived_from_it_and_keeps_the_rest() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let (thread_a, thread_b, thread_c) = (ThreadId::new(), ThreadId::new(), ThreadId::new());
        let gone = inbound(&store, &account.id, "gone", &thread_a, "alice@example.com").await;
        let kept = inbound(&store, &account.id, "kept", &thread_a, "alice@example.com").await;
        let gone_b = inbound(&store, &account.id, "gone-b", &thread_b, "JÖRG@Example.com").await;
        let other = inbound(&store, &account.id, "other", &thread_c, "dave@example.com").await;
        let (a, b, c) = (thread_a.as_str(), thread_b.as_str(), thread_c.as_str());
        let (gone_s, kept_s, gone_b_s, other_s) = (
            gone.as_str(),
            kept.as_str(),
            gone_b.as_str(),
            other.as_str(),
        );
        let acct = account.id.as_str();

        for (id, kind, key) in [
            ("brief-a", "thread", a.as_str()),
            ("brief-b", "thread", b.as_str()),
            ("brief-c", "thread", c.as_str()),
            ("gist-a", "thread", &format!("gist:{a}")),
            ("gist-c", "thread", &format!("gist:{c}")),
            ("brief-jorg", "recipient", "jörg@example.com"),
            ("brief-alice", "recipient", "alice@example.com"),
            ("brief-dave", "recipient", "dave@example.com"),
        ] {
            exec(
                &store,
                "INSERT INTO context_briefings (id, account_id, kind, subject_key, content_hash,
                     body_markdown, citations_json, generated_at)
                 VALUES (?, ?, ?, ?, 'h', 'quoted mail', '[]', 0)",
                &[id, &acct, kind, key],
            )
            .await;
        }
        for thread in [&a, &b, &c] {
            exec(
                &store,
                "INSERT INTO thread_summaries (thread_id, account_id, content_hash, text, model,
                     generated_at, updated_at)
                 VALUES (?, ?, 'h', 'summary', 'm', 0, 0)",
                &[thread, &acct],
            )
            .await;
        }
        for (id, thread, evidence) in [
            ("decision-gone", &thread_a, vec![gone.clone()]),
            (
                "decision-mixed",
                &thread_a,
                vec![gone.clone(), kept.clone()],
            ),
            ("decision-b", &thread_b, vec![gone_b.clone()]),
        ] {
            store
                .upsert_decision(&crate::DecisionLogEntry {
                    id: id.to_string(),
                    account_id: account.id.clone(),
                    thread_id: thread.clone(),
                    topic: None,
                    decision: "ship friday".to_string(),
                    rationale: None,
                    evidence_msg_ids: evidence,
                    decided_at: None,
                    extracted_at: chrono::Utc::now(),
                    source_hash: "h".to_string(),
                })
                .await
                .unwrap();
        }
        for (id, evidence) in [("commit-gone", &gone_s), ("commit-kept", &kept_s)] {
            exec(
                &store,
                "INSERT INTO contact_commitments (id, account_id, email, thread_id, direction,
                     status, who_owes, what, evidence_msg_id, extracted_at)
                 VALUES (?, ?, 'alice@example.com', ?, 'theirs', 'open', 'Alice', 'send deck', ?, 0)",
                &[id, &acct, &a, evidence],
            )
            .await;
        }
        for (delivery, sources) in [
            ("delivery-gone", vec![&gone_s]),
            ("delivery-mixed", vec![&gone_s, &kept_s]),
        ] {
            exec(
                &store,
                "INSERT INTO deliveries (id, account_id, dedup_key, status, source, last_event_at,
                     created_at, updated_at)
                 VALUES (?, ?, ?, 'shipped', 'heuristic', 0, 0, 0)",
                &[delivery, &acct, delivery],
            )
            .await;
            for source in sources {
                exec(
                    &store,
                    "INSERT INTO delivery_messages (delivery_id, message_id, detected_at)
                     VALUES (?, ?, 0)",
                    &[delivery, source],
                )
                .await;
            }
        }
        for thread in [&a, &b] {
            exec(
                &store,
                "INSERT INTO desk_dismissals (account_id, thread_id, through_rowid, through_count,
                     dismissed_at)
                 VALUES (?, ?, 1, 1, 0)",
                &[&acct, thread],
            )
            .await;
        }
        for email in ["Alice@Example.com", "dave@example.com"] {
            exec(
                &store,
                "INSERT INTO contact_relationship_summary (account_id, email, text, model,
                     computed_at, source_hash)
                 VALUES (?, ?, 'quotes the deleted mail', 'm', 0, 'h')",
                &[&acct, email],
            )
            .await;
        }
        for (message, summary) in [(&gone_s, "Applied rules to gone"), (&other_s, "kept")] {
            exec(
                &store,
                "INSERT INTO event_log (timestamp, level, category, message_id, summary)
                 VALUES (0, 'info', 'rule', ?, ?)",
                &[message, summary],
            )
            .await;
        }

        let deleted = store
            .delete_messages_and_derived(
                &account.id,
                &[
                    "gone".to_string(),
                    "gone-b".to_string(),
                    "never-synced".to_string(),
                ],
            )
            .await
            .unwrap();

        assert_eq!(
            deleted
                .message_ids
                .iter()
                .map(MessageId::as_str)
                .collect::<BTreeSet<_>>(),
            set(&[&gone_s, &gone_b_s])
        );
        assert_eq!(
            deleted
                .thread_ids
                .iter()
                .map(ThreadId::as_str)
                .collect::<BTreeSet<_>>(),
            set(&[&a, &b])
        );
        let people = deleted
            .counterparties
            .iter()
            .map(|(_, email)| email.to_lowercase())
            .collect::<BTreeSet<_>>();
        assert_eq!(people, set(&["alice@example.com", "jörg@example.com"]));

        assert_eq!(
            ids(&store, "SELECT provider_id FROM messages").await,
            set(&["kept", "other"])
        );
        assert_eq!(
            ids(&store, "SELECT id FROM context_briefings").await,
            set(&["brief-c", "brief-dave", "gist-c"])
        );
        assert_eq!(
            ids(&store, "SELECT thread_id FROM thread_summaries").await,
            set(&[&c])
        );
        assert_eq!(
            ids(&store, "SELECT id FROM decision_log").await,
            set(&["decision-mixed"])
        );
        assert_eq!(
            ids(&store, "SELECT id FROM contact_commitments").await,
            set(&["commit-kept"])
        );
        assert_eq!(
            ids(&store, "SELECT id FROM deliveries").await,
            set(&["delivery-mixed"])
        );
        assert_eq!(
            ids(&store, "SELECT message_id FROM delivery_messages").await,
            set(&[&kept_s])
        );
        assert_eq!(
            ids(&store, "SELECT thread_id FROM desk_dismissals").await,
            set(&[&a])
        );
        assert_eq!(
            ids(&store, "SELECT summary FROM event_log").await,
            set(&["kept"])
        );
        // Alice still has mail in thread A, but her summary drew on the
        // deleted message and is rebuilt; Dave's mail was untouched.
        assert_eq!(
            ids(&store, "SELECT email FROM contact_relationship_summary").await,
            set(&["dave@example.com"])
        );
    }

    #[tokio::test]
    async fn a_contact_whose_mail_is_all_gone_loses_what_was_built_about_them() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        inbound(
            &store,
            &account.id,
            "from-alice",
            &ThreadId::new(),
            "alice@example.com",
        )
        .await;
        inbound(
            &store,
            &account.id,
            "from-bob",
            &ThreadId::new(),
            "bob@example.com",
        )
        .await;
        store.refresh_contacts().await.unwrap();
        let acct = account.id.as_str();
        for email in ["alice@example.com", "bob@example.com"] {
            exec(
                &store,
                "INSERT INTO contact_relationship_summary (account_id, email, text, model,
                     computed_at, source_hash)
                 VALUES (?, ?, 'what they talk about', 'm', 0, 'h')",
                &[&acct, email],
            )
            .await;
            exec(
                &store,
                "INSERT INTO contact_style (account_id, email, computed_at, source_hash)
                 VALUES (?, ?, 0, 'h')",
                &[&acct, email],
            )
            .await;
        }

        store
            .delete_messages_and_derived(&account.id, &["from-alice".to_string()])
            .await
            .unwrap();
        // In the same second as the first refresh: the prune must not
        // depend on timestamps telling runs apart.
        store.refresh_contacts().await.unwrap();

        let bob = set(&["bob@example.com"]);
        assert_eq!(ids(&store, "SELECT email FROM contacts").await, bob);
        assert_eq!(
            ids(&store, "SELECT email FROM contact_relationship_summary").await,
            bob
        );
        assert_eq!(ids(&store, "SELECT email FROM contact_style").await, bob);
    }

    /// The cleanup outside SQLite is owed from the moment the delete
    /// commits, whatever happens to the sync pass after it.
    #[tokio::test]
    async fn the_delete_records_its_cleanup_until_it_is_cleared() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let gone = inbound(
            &store,
            &account.id,
            "gone",
            &ThreadId::new(),
            "a@example.com",
        )
        .await;
        inbound(
            &store,
            &account.id,
            "kept",
            &ThreadId::new(),
            "a@example.com",
        )
        .await;

        store
            .delete_messages_and_derived(&account.id, &["gone".to_string()])
            .await
            .unwrap();

        assert_eq!(
            store.list_pending_message_forgets(100).await.unwrap(),
            vec![gone.clone()]
        );
        store.clear_pending_message_forgets(&[gone]).await.unwrap();
        assert!(store
            .list_pending_message_forgets(100)
            .await
            .unwrap()
            .is_empty());
    }

    /// A cleanup that failed stays recorded, waits out a backoff, and is
    /// given up on after the attempt cap instead of retrying forever.
    #[tokio::test]
    async fn a_failed_cleanup_backs_off_and_stops_at_the_cap() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let gone = inbound(
            &store,
            &account.id,
            "gone",
            &ThreadId::new(),
            "a@example.com",
        )
        .await;
        store
            .delete_messages_and_derived(&account.id, &["gone".to_string()])
            .await
            .unwrap();
        let due_now = || async {
            sqlx::query("UPDATE pending_message_forgets SET retry_after = 0")
                .execute(store.writer())
                .await
                .unwrap();
        };

        assert_eq!(
            store
                .record_failed_message_forgets(std::slice::from_ref(&gone))
                .await
                .unwrap(),
            0
        );
        assert!(
            store
                .list_pending_message_forgets(10)
                .await
                .unwrap()
                .is_empty(),
            "a failed cleanup is retried only after its backoff"
        );
        due_now().await;
        assert_eq!(
            store.list_pending_message_forgets(10).await.unwrap(),
            vec![gone.clone()]
        );

        for _ in 1..MAX_FORGET_ATTEMPTS - 1 {
            store
                .record_failed_message_forgets(std::slice::from_ref(&gone))
                .await
                .unwrap();
        }
        assert_eq!(
            store.record_failed_message_forgets(&[gone]).await.unwrap(),
            1,
            "the last allowed attempt reports giving up"
        );
        due_now().await;
        assert!(store
            .list_pending_message_forgets(10)
            .await
            .unwrap()
            .is_empty());
    }

    /// The refresh nominates contacts from an aggregate snapshot. Mail that
    /// lands between the snapshot and the prune makes the contact live again,
    /// and the prune must see that mail, not the snapshot.
    #[tokio::test]
    async fn a_contact_whose_mail_arrived_after_the_snapshot_is_kept() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let acct = account.id.as_str();
        for email in ["bob@example.com", "carol@example.com"] {
            exec(
                &store,
                "INSERT INTO contacts (account_id, email, first_seen_at, last_seen_at,
                     refreshed_at)
                 VALUES (?, ?, 0, 0, 0)",
                &[&acct, email],
            )
            .await;
            exec(
                &store,
                "INSERT INTO contact_relationship_summary (account_id, email, text, model,
                     computed_at, source_hash)
                 VALUES (?, ?, 'summary', 'm', 0, 'h')",
                &[&acct, email],
            )
            .await;
        }
        // Both were missing from a snapshot; Bob's mail arrived after it.
        inbound(
            &store,
            &account.id,
            "late",
            &ThreadId::new(),
            "Bob@Example.com",
        )
        .await;

        let pruned = store
            .prune_contacts_without_mail(&[
                (acct.clone(), "bob@example.com".to_string()),
                (acct.clone(), "carol@example.com".to_string()),
            ])
            .await
            .unwrap();

        assert_eq!(pruned, 1);
        let bob = set(&["bob@example.com"]);
        assert_eq!(ids(&store, "SELECT email FROM contacts").await, bob);
        assert_eq!(
            ids(&store, "SELECT email FROM contact_relationship_summary").await,
            bob
        );
    }

    /// A model reading a thread with one live and one trashed message reads
    /// only the live one; a thread that is all Trash is an explicit choice
    /// and comes back whole.
    #[tokio::test]
    async fn the_model_view_of_a_thread_leaves_out_trash_and_spam() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mixed = ThreadId::new();
        let live = inbound(&store, &account.id, "live", &mixed, "a@example.com").await;
        let trashed = inbound(&store, &account.id, "trashed", &mixed, "b@example.com").await;
        store
            .move_to_trash(&trashed, mxr_core::types::EventSource::User)
            .await
            .unwrap();
        let all_trash = ThreadId::new();
        let only = inbound(&store, &account.id, "only", &all_trash, "c@example.com").await;
        store
            .move_to_trash(&only, mxr_core::types::EventSource::User)
            .await
            .unwrap();

        let ids_of = |envelopes: Vec<mxr_core::types::Envelope>| {
            envelopes
                .into_iter()
                .map(|envelope| envelope.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids_of(store.get_thread_envelopes_for_model(&mixed).await.unwrap()),
            vec![live]
        );
        assert_eq!(
            ids_of(
                store
                    .get_thread_envelopes_for_model(&all_trash)
                    .await
                    .unwrap()
            ),
            vec![only]
        );

        // The relationship summary's samples follow the same rule.
        let samples = store
            .recent_contact_messages(&account.id, "b@example.com", 10)
            .await
            .unwrap();
        assert!(
            samples.is_empty(),
            "trashed mail reached the summary samples"
        );
    }

    /// Every cache of model text over mail refuses a write that started
    /// before a delete of mail it read and lands after it: otherwise the
    /// write brings the deleted text back after the delete cleared it.
    #[tokio::test]
    async fn model_caches_refuse_writes_built_from_mail_deleted_mid_call() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let thread = ThreadId::new();
        let kept = inbound(&store, &account.id, "kept", &thread, "alice@example.com").await;
        let gone = inbound(&store, &account.id, "gone", &thread, "alice@example.com").await;
        store.refresh_contacts().await.unwrap();
        // The model calls start: they read the mail and the contact row.
        let contact_version: i64 = sqlx::query_scalar(
            "SELECT refreshed_at FROM contacts WHERE email = 'alice@example.com'",
        )
        .fetch_one(store.reader())
        .await
        .unwrap();
        store
            .delete_messages_and_derived(&account.id, &["gone".to_string()])
            .await
            .unwrap();

        let summary = crate::ThreadSummaryRecord {
            thread_id: thread.clone(),
            account_id: account.id.clone(),
            content_hash: "h".into(),
            text: "quotes the deleted mail".into(),
            model: "m".into(),
            generated_at: chrono::Utc::now(),
        };
        assert!(!store
            .upsert_thread_summary_if_sources_exist(&summary, &[kept.clone(), gone.clone()])
            .await
            .unwrap());
        let relationship = crate::ContactRelationshipSummaryRecord {
            account_id: account.id.clone(),
            email: "alice@example.com".into(),
            text: "quotes the deleted mail".into(),
            model: "m".into(),
            known_topics: vec![],
            computed_at: chrono::Utc::now(),
            source_hash: "h".into(),
            last_error: None,
        };
        assert!(!store
            .upsert_contact_relationship_summary_if_sources_exist(
                &relationship,
                &[kept.clone(), gone]
            )
            .await
            .unwrap());
        let recipient = crate::ContextBriefing {
            id: crate::new_briefing_id(),
            account_id: account.id.clone(),
            kind: crate::BriefingKind::Recipient,
            subject_key: "alice@example.com".into(),
            content_hash: "h".into(),
            body_markdown: "stats from before the delete".into(),
            citations: vec![],
            generated_at: chrono::Utc::now(),
        };
        assert!(!store
            .upsert_recipient_briefing_if_contact_unchanged(
                &recipient,
                "alice@example.com",
                Some(contact_version)
            )
            .await
            .unwrap());
        for table in [
            "thread_summaries",
            "contact_relationship_summary",
            "context_briefings",
        ] {
            let rows: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(store.reader())
                    .await
                    .unwrap();
            assert_eq!(rows, 0, "{table} took a write built from deleted mail");
        }

        // Built from mail that is all still here, the writes go through.
        assert!(store
            .upsert_thread_summary_if_sources_exist(&summary, std::slice::from_ref(&kept))
            .await
            .unwrap());
        assert!(store
            .upsert_contact_relationship_summary_if_sources_exist(&relationship, &[kept])
            .await
            .unwrap());
        let current: i64 = sqlx::query_scalar(
            "SELECT refreshed_at FROM contacts WHERE email = 'alice@example.com'",
        )
        .fetch_one(store.reader())
        .await
        .unwrap();
        assert!(store
            .upsert_recipient_briefing_if_contact_unchanged(
                &recipient,
                "alice@example.com",
                Some(current)
            )
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn deleting_ids_the_store_does_not_hold_changes_nothing() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        inbound(
            &store,
            &account.id,
            "kept",
            &ThreadId::new(),
            "alice@example.com",
        )
        .await;

        let deleted = store
            .delete_messages_and_derived(&account.id, &["never-synced".to_string()])
            .await
            .unwrap();

        assert_eq!(deleted, DeletedMessages::default());
        assert_eq!(
            ids(&store, "SELECT provider_id FROM messages").await,
            set(&["kept"])
        );
    }

    /// The delete runs these lookups while holding the writer; a full scan
    /// of the event log or a JSON parse of every decision there stalls every
    /// other write. (The commitments lookup is already covered by the
    /// table's unique index on the account.)
    #[tokio::test]
    async fn the_delete_lookups_use_an_index() {
        let store = Store::in_memory().await.unwrap();
        for (sql, index) in [
            (
                "EXPLAIN QUERY PLAN SELECT 1 FROM event_log WHERE message_id IN ('a', 'b')",
                "idx_event_log_message",
            ),
            (
                "EXPLAIN QUERY PLAN SELECT decision_id FROM decision_evidence
                 WHERE message_id IN ('a', 'b')",
                "idx_decision_evidence_message",
            ),
        ] {
            let plan = sqlx::query(sqlx::AssertSqlSafe(sql))
                .fetch_all(store.reader())
                .await
                .unwrap()
                .iter()
                .map(|row| row.get::<String, _>("detail"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(plan.contains(index), "{sql}\n{plan}");
        }
    }

    /// Fails when a table refers to messages (a foreign key, a message or
    /// thread id column, an evidence list) without either a cascading
    /// foreign key or an entry in `MESSAGE_DELETION_RULES`. A new table
    /// built from mail has to say what happens to it when the mail goes.
    #[tokio::test]
    async fn every_table_referring_to_messages_has_a_deletion_rule() {
        let store = Store::in_memory().await.unwrap();
        let tables = sqlx::query_scalar::<_, String>(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(store.reader())
        .await
        .unwrap();
        let rules = MESSAGE_DELETION_RULES
            .iter()
            .copied()
            .collect::<BTreeMap<_, _>>();

        let mut missing = Vec::new();
        for table in &tables {
            if table == "messages" {
                continue;
            }
            let cascades_from_messages = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT on_delete FROM pragma_foreign_key_list('{table}') WHERE \"table\" = 'messages'"
            )))
            .fetch_all(store.reader())
            .await
            .unwrap()
            .iter()
            .map(|row| row.get::<String, _>("on_delete"))
            .collect::<Vec<_>>();
            if let Some(action) = cascades_from_messages
                .iter()
                .find(|action| !matches!(action.as_str(), "CASCADE" | "SET NULL"))
            {
                missing.push(format!(
                    "{table}: foreign key to messages is ON DELETE {action}"
                ));
                continue;
            }
            if !cascades_from_messages.is_empty() {
                continue;
            }
            let columns = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(format!(
                "SELECT name FROM pragma_table_info('{table}')"
            )))
            .fetch_all(store.reader())
            .await
            .unwrap();
            let refers_to_messages = columns.iter().any(|column| {
                let column = column.to_ascii_lowercase();
                column == "thread_id" || column.contains("message_id") || column.contains("msg_id")
            });
            if refers_to_messages && !rules.contains_key(table.as_str()) {
                missing.push(format!("{table}: columns {columns:?}"));
            }
        }
        assert!(
            missing.is_empty(),
            "tables refer to messages with no deletion rule; add a cascading foreign key \
             or an entry in MESSAGE_DELETION_RULES:\n{}",
            missing.join("\n")
        );

        for table in rules.keys() {
            assert!(
                tables.iter().any(|name| name == table),
                "MESSAGE_DELETION_RULES names {table}, which is not in the schema"
            );
        }
    }
}
