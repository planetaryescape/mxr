//! Deleting messages together with everything derived from them.
//!
//! The invariant: deleting an email deletes everything derived from it.
//! Most per-message tables reach it through `ON DELETE CASCADE` on
//! `messages(id)`. The tables below hold text built from mail but are keyed
//! by thread, contact or a JSON list of ids, which a cascade cannot follow,
//! so [`Store::delete_messages_and_derived`] clears them in the same
//! transaction as the message rows. [`MESSAGE_DELETION_RULES`] records the
//! rule of every table that refers to messages without a cascade; the
//! schema test fails for a new table that has neither.

use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::Row;

use crate::decode_id;

/// How a table that refers to messages without a cascading foreign key
/// follows a message delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDeletionRule {
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
pub const MESSAGE_DELETION_RULES: &[(&str, MessageDeletionRule)] = &[
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
    (
        "contact_relationship_summary",
        MessageDeletionRule::ClearedWithContact,
    ),
    ("contact_style", MessageDeletionRule::ClearedWithContact),
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
];

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
        // Read before the transaction: the counterparties come from the rows
        // about to be deleted, and the lookup runs on the reader pool.
        let preview = self
            .message_ids_by_provider_ids(account_id, provider_ids)
            .await?;
        if preview.is_empty() {
            return Ok(DeletedMessages::default());
        }
        let counterparties = self.relationship_contacts_for_messages(&preview).await?;

        let account = account_id.as_str();
        let mut tx = self.writer().begin().await?;
        for statement in [
            "DROP TABLE IF EXISTS temp.mxr_deleting",
            "DROP TABLE IF EXISTS temp.mxr_deleting_people",
            "DROP TABLE IF EXISTS temp.mxr_deleting_deliveries",
            "DROP TABLE IF EXISTS temp.mxr_deleting_decisions",
            "CREATE TEMP TABLE mxr_deleting (id TEXT PRIMARY KEY, thread_id TEXT NOT NULL)",
            "CREATE TEMP TABLE mxr_deleting_people (email TEXT PRIMARY KEY)",
        ] {
            sqlx::query(statement).execute(&mut *tx).await?;
        }
        // Resolved again inside the transaction, so what is deleted is
        // exactly what the database holds now.
        for chunk in provider_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "INSERT OR IGNORE INTO temp.mxr_deleting (id, thread_id)
                 SELECT id, thread_id FROM messages
                 WHERE account_id = ? AND provider_id IN ({})",
                placeholders(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind(&account);
            for provider_id in chunk {
                query = query.bind(provider_id);
            }
            query.execute(&mut *tx).await?;
        }
        for (_, email) in &counterparties {
            sqlx::query("INSERT OR IGNORE INTO temp.mxr_deleting_people (email) VALUES (lower(?))")
                .bind(email)
                .execute(&mut *tx)
                .await?;
        }

        let deleted = sqlx::query("SELECT id, thread_id FROM temp.mxr_deleting ORDER BY id")
            .fetch_all(&mut *tx)
            .await?;
        let mut message_ids = Vec::with_capacity(deleted.len());
        let mut thread_ids = Vec::new();
        for row in &deleted {
            message_ids.push(decode_id::<MessageId>(&row.get::<String, _>("id"))?);
            let thread_id = decode_id::<ThreadId>(&row.get::<String, _>("thread_id"))?;
            if !thread_ids.contains(&thread_id) {
                thread_ids.push(thread_id);
            }
        }

        // Statements bind `?1` to the account. Order matters: the
        // provenance and evidence lookups read rows the message delete
        // cascades away, so they are captured first and checked after.
        for statement in [
            // Thread gists and summaries can quote any message of the
            // thread, so a partial delete drops them too.
            "DELETE FROM context_briefings WHERE account_id = ?1 AND kind = 'thread'
               AND subject_key IN (SELECT thread_id FROM temp.mxr_deleting)",
            "DELETE FROM context_briefings WHERE account_id = ?1 AND kind = 'recipient'
               AND subject_key IN (SELECT email FROM temp.mxr_deleting_people)",
            "DELETE FROM thread_summaries WHERE account_id = ?1
               AND thread_id IN (SELECT thread_id FROM temp.mxr_deleting)",
            "DELETE FROM contact_commitments WHERE account_id = ?1
               AND evidence_msg_id IN (SELECT id FROM temp.mxr_deleting)",
            "DELETE FROM event_log WHERE message_id IN (SELECT id FROM temp.mxr_deleting)",
            "CREATE TEMP TABLE mxr_deleting_deliveries AS
               SELECT DISTINCT delivery_id AS id FROM delivery_messages
               WHERE message_id IN (SELECT id FROM temp.mxr_deleting)",
            "CREATE TEMP TABLE mxr_deleting_decisions AS
               SELECT DISTINCT decision_log.id AS id
               FROM decision_log, json_each(decision_log.evidence_msg_ids) AS evidence
               WHERE decision_log.account_id = ?1
                 AND evidence.value IN (SELECT id FROM temp.mxr_deleting)",
            "DELETE FROM messages WHERE id IN (SELECT id FROM temp.mxr_deleting)",
            "DELETE FROM deliveries
               WHERE id IN (SELECT id FROM temp.mxr_deleting_deliveries)
                 AND NOT EXISTS (
                     SELECT 1 FROM delivery_messages
                     WHERE delivery_messages.delivery_id = deliveries.id)",
            // A decision survives while any of its evidence is still here.
            "DELETE FROM decision_log
               WHERE id IN (SELECT id FROM temp.mxr_deleting_decisions)
                 AND NOT EXISTS (
                     SELECT 1 FROM json_each(decision_log.evidence_msg_ids) AS evidence
                     JOIN messages ON messages.id = evidence.value)",
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

    async fn message_ids_by_provider_ids(
        &self,
        account_id: &AccountId,
        provider_ids: &[String],
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let mut ids = Vec::new();
        for chunk in provider_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "SELECT id FROM messages WHERE account_id = ? AND provider_id IN ({})",
                placeholders(chunk.len())
            );
            let mut query =
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind(account_id.as_str());
            for provider_id in chunk {
                query = query.bind(provider_id);
            }
            for row in query.fetch_all(self.reader()).await? {
                ids.push(decode_id(&row.get::<String, _>("id"))?);
            }
        }
        Ok(ids)
    }
}

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use mxr_core::types::{Address, MessageDirection};
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
        let gone_b = inbound(
            &store,
            &account.id,
            "gone-b",
            &thread_b,
            "Carol@Example.com",
        )
        .await;
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
            ("brief-alice", "recipient", "alice@example.com"),
            ("brief-carol", "recipient", "carol@example.com"),
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
            ("decision-gone", a.clone(), format!(r#"["{gone_s}"]"#)),
            (
                "decision-mixed",
                a.clone(),
                format!(r#"["{gone_s}","{kept_s}"]"#),
            ),
            ("decision-b", b.clone(), format!(r#"["{gone_b_s}"]"#)),
        ] {
            exec(
                &store,
                "INSERT INTO decision_log (id, account_id, thread_id, decision, evidence_msg_ids,
                     extracted_at, source_hash)
                 VALUES (?, ?, ?, 'ship friday', ?, 0, 'h')",
                &[id, &acct, &thread, &evidence],
            )
            .await;
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
            .map(|(_, email)| email.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        assert_eq!(people, set(&["alice@example.com", "carol@example.com"]));

        assert_eq!(
            ids(&store, "SELECT provider_id FROM messages").await,
            set(&["kept", "other"])
        );
        assert_eq!(
            ids(&store, "SELECT id FROM context_briefings").await,
            set(&["brief-c", "brief-dave"])
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
