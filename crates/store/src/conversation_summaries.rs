//! Generation-checked, rebuildable thread facts. The daemon owns their shape.
use crate::{encode_json, Store};
use chrono::{DateTime, Utc};
use mxr_core::{AccountId, ThreadId};

/// The two independent inputs a history synopsis observed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConversationRevision {
    pub thread_id: ThreadId,
    pub input: i64,
    pub view: i64,
}

/// A candidate thread's valid synopsis, or the tokens needed to rebuild it.
pub struct ConversationSummary<T> {
    pub revision: ConversationRevision,
    pub json: Option<T>,
}

/// A synopsis to persist only if its observed inputs have not changed.
#[derive(serde::Serialize)]
pub struct ConversationSummaryWrite {
    pub revision: ConversationRevision,
    pub valid_from: i64,
    pub valid_until: i64,
    pub json: String,
}

impl Store {
    async fn conversation_candidates(
        &self,
        account: &AccountId,
        context: &str,
    ) -> Result<String, sqlx::Error> {
        let now = Utc::now().timestamp();
        let (input, facet, cached): (i64, i64, Option<String>) = sqlx::query_as(
            "SELECT i.revision, f.revision, (SELECT s.thread_ids_json FROM conversation_candidate_sets s
             WHERE s.account_id = ?1 AND s.context = ?2 AND s.input_revision = i.revision
                 AND s.facet_revision = f.revision AND s.valid_from <= ?3 AND s.valid_until > ?3)
             FROM assessment_inputs i CROSS JOIN conversation_facet_clock f WHERE i.id = 1 AND f.id = 1"
        ).bind(account.as_str()).bind(context).bind(now).fetch_one(self.reader()).await?;
        if let Some(cached) = cached {
            return Ok(cached);
        }
        let sql = format!(
            "WITH active AS ({}) SELECT json_group_array(thread_id) FROM active",
            crate::desk::CONVERSATION_THREADS
        );
        let ids: String = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(account.as_str())
            .bind(0_i64)
            .bind(0_i64)
            .bind(now + 86_400)
            .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
            .fetch_one(self.reader())
            .await?;
        let next: Option<i64> = sqlx::query_scalar(
            "SELECT min(date) FROM messages WHERE account_id = ?1 AND date > ?2",
        )
        .bind(account.as_str())
        .bind(now + 86_400)
        .fetch_one(self.reader())
        .await?;
        // A late selector cannot publish a set observed before any source or
        // same-generation home edit. This stores only IDs, never display rows.
        sqlx::query(
            "INSERT INTO conversation_candidate_sets
            SELECT ?1, ?2, i.revision, f.revision, ?5, ?6, ?7
            FROM assessment_inputs i CROSS JOIN conversation_facet_clock f
            WHERE i.id = 1 AND f.id = 1 AND i.revision = ?3 AND f.revision = ?4
                AND EXISTS (SELECT 1 FROM accounts WHERE id = ?1)
            ON CONFLICT(account_id) DO UPDATE SET context = excluded.context,
                input_revision = excluded.input_revision, facet_revision = excluded.facet_revision,
                valid_from = excluded.valid_from, valid_until = excluded.valid_until,
                thread_ids_json = excluded.thread_ids_json",
        )
        .bind(account.as_str())
        .bind(context)
        .bind(input)
        .bind(facet)
        .bind(now)
        .bind(next.map_or(i64::MAX, |date| date - 86_400))
        .bind(&ids)
        .execute(self.writer())
        .await?;
        Ok(ids)
    }

    pub async fn conversation_summaries<T: serde::de::DeserializeOwned>(
        &self,
        account: &AccountId,
        context: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<ConversationSummary<T>>, sqlx::Error> {
        let started = std::time::Instant::now();
        let candidates = self.conversation_candidates(account, context).await?;
        let sql = "WITH active AS (SELECT value AS thread_id FROM json_each(?3)),
             revisions AS MATERIALIZED (
                 SELECT i.thread_id, i.effective_revision FROM active
                 CROSS JOIN assessment_effective_thread_inputs i
                     ON i.account_id = ?1 AND i.thread_id = active.thread_id
             )
             SELECT json_group_array(json_array(i.thread_id, i.effective_revision, COALESCE(v.revision, 0), json(
             CASE WHEN s.input_revision = i.effective_revision AND s.view_revision = COALESCE(v.revision, 0)
                 AND s.context = ?2 AND s.valid_from <= ?4 AND s.valid_until > ?4 THEN s.summary_json END)))
             FROM revisions i
             LEFT JOIN conversation_view_inputs v ON v.account_id = ?1 AND v.thread_id = i.thread_id
             LEFT JOIN conversation_summaries s ON s.account_id = ?1 AND s.thread_id = i.thread_id";
        // One typed JSON page avoids 20k SQLite-worker row handoffs and
        // decodes each synopsis once, rather than parsing an escaped JSON string.
        let page: String = sqlx::query_scalar(sql)
            .bind(account.as_str())
            .bind(context)
            .bind(candidates)
            .bind(now.timestamp())
            .fetch_one(self.reader())
            .await?;
        let rows: Vec<(ThreadId, i64, i64, Option<T>)> = crate::decode_json(&page)?;
        crate::trace_query("messages.conversation_summaries", started, rows.len());
        Ok(rows
            .into_iter()
            .map(|(thread, input, view, json)| ConversationSummary {
                revision: ConversationRevision {
                    thread_id: thread,
                    input,
                    view,
                },
                json,
            })
            .collect())
    }

    pub async fn save_conversation_summaries(
        &self,
        account: &AccountId,
        context: &str,
        summaries: &[ConversationSummaryWrite],
    ) -> Result<(), sqlx::Error> {
        if summaries.is_empty() {
            return Ok(());
        }
        // The SELECT and write share the writer transaction. A late builder
        // cannot publish facts from before a read, label, facet or input edit.
        sqlx::query("INSERT INTO conversation_summaries
            SELECT ?1, i.thread_id, i.effective_revision, COALESCE(v.revision, 0), ?2,
                json_extract(w.value, '$.valid_from'), json_extract(w.value, '$.valid_until'), json_extract(w.value, '$.json')
            FROM json_each(?3) w CROSS JOIN assessment_effective_thread_inputs i
                ON i.account_id = ?1 AND i.thread_id = json_extract(w.value, '$.revision.thread_id')
            LEFT JOIN conversation_view_inputs v ON v.account_id = ?1 AND v.thread_id = i.thread_id
            WHERE i.effective_revision = json_extract(w.value, '$.revision.input')
                AND COALESCE(v.revision, 0) = json_extract(w.value, '$.revision.view')
            ON CONFLICT(account_id, thread_id) DO UPDATE SET input_revision = excluded.input_revision,
                view_revision = excluded.view_revision, context = excluded.context,
                valid_from = excluded.valid_from, valid_until = excluded.valid_until, summary_json = excluded.summary_json")
            .bind(account.as_str()).bind(context).bind(encode_json(&summaries)?)
            .execute(self.writer()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use mxr_core::{MessageDirection, MessageFlags};

    #[tokio::test]
    async fn deleting_one_source_clears_the_whole_synopsis_and_rejects_its_late_writer() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let first = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        let mut second = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        second.thread_id = first.thread_id.clone();
        second.provider_id = format!("fake-{}", second.id);
        second.message_id_header = Some(format!("<{}@example.test>", second.id));
        for mail in [&first, &second] {
            store
                .upsert_envelope_with_direction(mail, MessageDirection::Outbound)
                .await
                .unwrap();
        }
        let now = Utc::now();
        let before = snapshot(&store, &account.id, now).await.revision;
        save(&store, &account.id, before.clone(), now).await;
        sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(first.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM conversation_summaries")
            .fetch_one(store.reader())
            .await
            .unwrap();
        assert_eq!(
            retained, 0,
            "even a partial thread delete erases derived text"
        );
        let sets: i64 = sqlx::query_scalar("SELECT count(*) FROM conversation_candidate_sets")
            .fetch_one(store.reader())
            .await
            .unwrap();
        assert_eq!(sets, 0);
        save(&store, &account.id, before, now).await;
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
    }

    #[tokio::test]
    async fn candidate_sets_never_freeze_previously_excluded_threads() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mail = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
            .await
            .unwrap();
        sqlx::query("INSERT INTO mail_assessments SELECT ?1, ?2, ?3, 'fixture', effective_revision, '{\"home\":\"updates\"}' FROM assessment_effective_thread_inputs WHERE account_id = ?2 AND thread_id = ?4")
            .bind(mail.id.as_str()).bind(account.id.as_str())
            .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION).bind(mail.thread_id.as_str())
            .execute(store.writer()).await.unwrap();
        let now = Utc::now();
        for _ in 0..2 {
            assert!(store
                .conversation_summaries::<String>(&account.id, "test-context", now)
                .await
                .unwrap()
                .is_empty());
        }
        sqlx::query("UPDATE mail_assessments SET assessment_json = '{\"home\":\"messages\"}' WHERE message_id = ?")
            .bind(mail.id.as_str()).execute(store.writer()).await.unwrap();
        assert_eq!(
            snapshot(&store, &account.id, now).await.revision.thread_id,
            mail.thread_id
        );
        sqlx::query("UPDATE mail_assessments SET assessment_json = '{\"home\":\"updates\"}' WHERE message_id = ?")
            .bind(mail.id.as_str()).execute(store.writer()).await.unwrap();
        assert!(store
            .conversation_summaries::<String>(&account.id, "test-context", now)
            .await
            .unwrap()
            .is_empty());
        sqlx::query("DELETE FROM mail_assessments WHERE message_id = ?")
            .bind(mail.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        assert_eq!(
            snapshot(&store, &account.id, now).await.revision.thread_id,
            mail.thread_id
        );
    }

    #[tokio::test]
    async fn inbox_edits_and_label_role_changes_retire_facts_and_account_cascade_is_safe() {
        use mxr_core::{Label, LabelId, LabelKind};
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mail = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Outbound)
            .await
            .unwrap();
        let mut label = Label {
            id: LabelId::new(),
            account_id: account.id.clone(),
            name: "Inbox".into(),
            kind: LabelKind::System,
            provider_id: "INBOX".into(),
            color: None,
            unread_count: 0,
            total_count: 0,
            role: None,
        };
        store.upsert_label(&label).await.unwrap();
        let now = Utc::now();
        let first = snapshot(&store, &account.id, now).await.revision;
        save(&store, &account.id, first.clone(), now).await;
        store
            .add_message_label(&mail.id, &label.id, mxr_core::EventSource::User)
            .await
            .unwrap();
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
        save(&store, &account.id, first, now).await;
        assert!(
            snapshot(&store, &account.id, now).await.json.is_none(),
            "pre-inbox builder is rejected"
        );
        let added = snapshot(&store, &account.id, now).await.revision;
        save(&store, &account.id, added, now).await;
        label.provider_id = "OTHER".into();
        store.upsert_label(&label).await.unwrap();
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
        let renamed = snapshot(&store, &account.id, now).await.revision;
        save(&store, &account.id, renamed, now).await;
        store
            .remove_message_label(&mail.id, &label.id, mxr_core::EventSource::User)
            .await
            .unwrap();
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
        store
            .add_message_label(&mail.id, &label.id, mxr_core::EventSource::User)
            .await
            .unwrap();
        let linked = snapshot(&store, &account.id, now).await.revision;
        save(&store, &account.id, linked, now).await;
        store.delete_label(&label.id).await.unwrap();
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
        label.id = LabelId::new();
        store.upsert_label(&label).await.unwrap();
        store
            .add_message_label(&mail.id, &label.id, mxr_core::EventSource::User)
            .await
            .unwrap();
        store.delete_account(&account.id).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM conversation_view_inputs")
            .fetch_one(store.reader())
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    async fn snapshot(
        store: &Store,
        account: &AccountId,
        now: DateTime<Utc>,
    ) -> ConversationSummary<String> {
        let mut rows = store
            .conversation_summaries::<String>(account, "test-context", now)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        rows.remove(0)
    }

    async fn save(
        store: &Store,
        account: &AccountId,
        revision: ConversationRevision,
        now: DateTime<Utc>,
    ) {
        store
            .save_conversation_summaries(
                account,
                "test-context",
                &[ConversationSummaryWrite {
                    revision,
                    valid_from: now.timestamp(),
                    valid_until: now.timestamp() + 100,
                    json: encode_json(&"synthetic thread facts").unwrap(),
                }],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn summaries_validate_context_clock_account_and_both_input_tokens() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mut mail = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Outbound)
            .await
            .unwrap();
        let now = Utc::now();
        let first = snapshot(&store, &account.id, now).await;
        assert!(first.json.is_none());
        save(&store, &account.id, first.revision.clone(), now).await;
        assert_eq!(
            snapshot(&store, &account.id, now).await.json.as_deref(),
            Some("synthetic thread facts")
        );
        assert!(store
            .conversation_summaries::<String>(&account.id, "other-context", now)
            .await
            .unwrap()[0]
            .json
            .is_none());
        assert!(
            snapshot(&store, &account.id, now - chrono::Duration::seconds(1))
                .await
                .json
                .is_none()
        );
        assert!(
            snapshot(&store, &account.id, now + chrono::Duration::seconds(100))
                .await
                .json
                .is_none()
        );
        assert!(store
            .conversation_summaries::<String>(&AccountId::new(), "test-context", now)
            .await
            .unwrap()
            .is_empty());

        mail.flags |= MessageFlags::READ;
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Outbound)
            .await
            .unwrap();
        let read = snapshot(&store, &account.id, now).await;
        assert!(read.json.is_none());
        assert_eq!(
            first.revision.input, read.revision.input,
            "read flags are not classifier inputs"
        );
        assert!(read.revision.view > first.revision.view);
        // A builder that lost the race to the read edit must not publish.
        save(&store, &account.id, first.revision, now).await;
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
        save(&store, &account.id, read.revision, now).await;
        assert!(snapshot(&store, &account.id, now).await.json.is_some());

        mail.subject.push_str(" changed");
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Outbound)
            .await
            .unwrap();
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
    }

    #[tokio::test]
    async fn facet_writes_at_the_same_generation_retire_facts_and_late_builds() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mail = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Outbound)
            .await
            .unwrap();
        let now = Utc::now();
        let before = snapshot(&store, &account.id, now).await.revision;
        save(&store, &account.id, before.clone(), now).await;
        sqlx::query(
            "INSERT INTO mail_assessments VALUES(?1, ?2, 'test-policy', 'fingerprint', ?3, '{}')",
        )
        .bind(mail.id.as_str())
        .bind(account.id.as_str())
        .bind(before.input)
        .execute(store.writer())
        .await
        .unwrap();
        let changed = snapshot(&store, &account.id, now).await;
        assert!(changed.json.is_none());
        assert_eq!(changed.revision.input, before.input);
        assert!(changed.revision.view > before.view);
        save(&store, &account.id, before, now).await;
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
        save(&store, &account.id, changed.revision, now).await;
        sqlx::query("DELETE FROM mail_assessments WHERE message_id = ?")
            .bind(mail.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        assert!(snapshot(&store, &account.id, now).await.json.is_none());
    }
}
