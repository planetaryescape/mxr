//! Listing and counting a system mailbox (Inbox, Sent, ...) across a set of
//! accounts, for the "All accounts" views.
//!
//! The single-label queries (`list_threads`, `list_envelopes_by_label`) stay
//! the path for an explicit label id. These match labels by role instead:
//! every System label in the account set whose name maps to the role through
//! `Role::system_label_names`. They are unchecked queries so they need no
//! `.sqlx` cache entry.

use crate::message::{future_date_cutoff_timestamp, sqlite_row_to_envelope};
use crate::{decode_id, trace_query};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Envelope, Role, SortOrder, Thread};
use sqlx::Row;

fn json_array(values: impl IntoIterator<Item = impl AsRef<str>>) -> Result<String, sqlx::Error> {
    serde_json::to_string(
        &values
            .into_iter()
            .map(|value| value.as_ref().to_string())
            .collect::<Vec<_>>(),
    )
    .map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

fn account_ids_json(account_ids: &[AccountId]) -> Result<String, sqlx::Error> {
    json_array(account_ids.iter().map(AccountId::as_str))
}

fn label_names_json(role: Role) -> Result<String, sqlx::Error> {
    json_array(role.system_label_names().iter().copied())
}

/// Messages that carry a System label of the role in one of the accounts
/// (`?1` accounts, `?2` label names, `?3` = 1 for All Mail, which has no
/// label to match: every message of the accounts counts). A macro so the
/// final SQL stays a literal.
macro_rules! role_filter {
    () => {
        "(?3 = 1 OR m.id IN (
            SELECT ml.message_id FROM message_labels ml
            JOIN labels l ON l.id = ml.label_id
            WHERE l.kind = 'system'
              AND l.account_id IN (SELECT value FROM json_each(?1))
              AND lower(l.name) IN (SELECT value FROM json_each(?2))
        ))"
    };
}

/// Thread id breaks date ties so paging never depends on storage order.
macro_rules! thread_ids_sql {
    ($direction:literal) => {
        concat!(
            "SELECT m.thread_id AS thread_id
             FROM messages m
             WHERE m.account_id IN (SELECT value FROM json_each(?1))
               AND ",
            role_filter!(),
            "
             GROUP BY m.thread_id
             ORDER BY MAX(CASE WHEN m.date > ?4 THEN 0 ELSE m.date END) ",
            $direction,
            ", m.thread_id ",
            $direction,
            "
             LIMIT ?5 OFFSET ?6"
        )
    };
}

impl super::Store {
    /// Paginated threads in a system mailbox across `account_ids`, ordered
    /// like `list_threads` (newest first, or oldest first for `DateAsc`).
    pub async fn list_threads_by_role(
        &self,
        account_ids: &[AccountId],
        role: Role,
        limit: u32,
        offset: u32,
        sort: SortOrder,
    ) -> Result<Vec<Thread>, sqlx::Error> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = if matches!(sort, SortOrder::DateAsc) {
            thread_ids_sql!("ASC")
        } else {
            thread_ids_sql!("DESC")
        };
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(sql)
            .bind(account_ids_json(account_ids)?)
            .bind(label_names_json(role)?)
            .bind(i64::from(role == Role::AllMail))
            .bind(future_date_cutoff_timestamp())
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(self.reader())
            .await?;
        trace_query(
            "system_mailbox.list_threads_by_role",
            started_at,
            rows.len(),
        );

        let thread_ids = rows
            .iter()
            .map(|row| decode_id::<ThreadId>(&row.get::<String, _>("thread_id")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut threads = self.get_threads_batch(&thread_ids).await?;
        threads.sort_by_key(|thread| {
            thread_ids
                .iter()
                .position(|id| id == &thread.id)
                .unwrap_or(usize::MAX)
        });
        Ok(threads)
    }

    /// Paginated messages in a system mailbox across `account_ids`, newest
    /// first, like `list_envelopes_by_label`.
    pub async fn list_envelopes_by_role(
        &self,
        account_ids: &[AccountId],
        role: Role,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(concat!(
            "SELECT
                m.id, m.account_id, m.provider_id, m.thread_id, m.message_id_header,
                m.in_reply_to, m.reference_headers, m.from_name, m.from_email,
                m.to_addrs, m.cc_addrs, m.bcc_addrs, m.subject, m.date, m.flags,
                m.snippet, m.has_attachments, m.size_bytes, m.unsubscribe_method,
                COALESCE((
                    SELECT GROUP_CONCAT(labels.provider_id, char(31))
                    FROM message_labels
                    JOIN labels ON labels.id = message_labels.label_id
                    WHERE message_labels.message_id = m.id
                ), '') AS label_provider_ids
             FROM messages m
             WHERE m.account_id IN (SELECT value FROM json_each(?1))
               AND ",
            role_filter!(),
            "
             ORDER BY CASE WHEN m.date > ?4 THEN 0 ELSE m.date END DESC, m.id DESC
             LIMIT ?5 OFFSET ?6"
        ))
        .bind(account_ids_json(account_ids)?)
        .bind(label_names_json(role)?)
        .bind(i64::from(role == Role::AllMail))
        .bind(future_date_cutoff_timestamp())
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "system_mailbox.list_envelopes_by_role",
            started_at,
            rows.len(),
        );

        let mut envelopes = rows
            .iter()
            .map(sqlite_row_to_envelope)
            .collect::<Result<Vec<_>, _>>()?;
        self.hydrate_link_metadata(&mut envelopes).await?;
        self.hydrate_envelope_keywords(&mut envelopes).await?;
        Ok(envelopes)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use chrono::{Duration, Utc};
    use mxr_core::id::{AccountId, LabelId, ThreadId};
    use mxr_core::types::{EventSource, Label, LabelKind, Role, SortOrder};
    use mxr_core::Account;

    async fn account(store: &Store) -> Account {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        account
    }

    async fn label(store: &Store, account_id: &AccountId, name: &str, kind: LabelKind) -> Label {
        let label = Label {
            id: LabelId::new(),
            account_id: account_id.clone(),
            name: name.to_string(),
            kind,
            color: None,
            provider_id: name.to_string(),
            unread_count: 0,
            total_count: 0,
            role: None,
        };
        store.upsert_label(&label).await.unwrap();
        label
    }

    /// One single-message thread in `account_id`, `minutes_ago` old, carrying `labels`.
    async fn message(
        store: &Store,
        account_id: &AccountId,
        minutes_ago: i64,
        labels: &[&Label],
        thread_id: Option<ThreadId>,
    ) -> (ThreadId, mxr_core::id::MessageId) {
        let thread_id = thread_id.unwrap_or_else(ThreadId::new);
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .build();
        env.provider_id = format!("p-{}", env.id);
        env.thread_id = thread_id.clone();
        env.date = Utc::now() - Duration::minutes(minutes_ago);
        store.upsert_envelope(&env).await.unwrap();
        let ids = labels.iter().map(|l| l.id.clone()).collect::<Vec<_>>();
        store
            .set_message_labels(&env.id, &ids, EventSource::Sync)
            .await
            .unwrap();
        (thread_id, env.id)
    }

    struct TwoAccounts {
        store: Store,
        a: Account,
        b: Account,
        a_inbox: Label,
        b_inbox: Label,
        a_sent: Label,
    }

    async fn two_accounts() -> TwoAccounts {
        let store = Store::in_memory().await.unwrap();
        let a = account(&store).await;
        let b = account(&store).await;
        // Gmail-style names on one account, fake-style on the other: both map to Inbox.
        let a_inbox = label(&store, &a.id, "INBOX", LabelKind::System).await;
        let b_inbox = label(&store, &b.id, "Inbox", LabelKind::System).await;
        let a_sent = label(&store, &a.id, "SENT", LabelKind::System).await;
        TwoAccounts {
            store,
            a,
            b,
            a_inbox,
            b_inbox,
            a_sent,
        }
    }

    #[tokio::test]
    async fn threads_by_role_merge_accounts_newest_first_and_page() {
        let t = two_accounts().await;
        // Interleave by age so a per-account concatenation would be wrong.
        let (a1, _) = message(&t.store, &t.a.id, 10, &[&t.a_inbox], None).await;
        let (b1, _) = message(&t.store, &t.b.id, 20, &[&t.b_inbox], None).await;
        let (a2, _) = message(&t.store, &t.a.id, 30, &[&t.a_inbox], None).await;
        let (b2, _) = message(&t.store, &t.b.id, 40, &[&t.b_inbox], None).await;
        let both = [t.a.id.clone(), t.b.id.clone()];

        let all = t
            .store
            .list_threads_by_role(&both, Role::Inbox, 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        let ids = all.iter().map(|th| th.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids, vec![a1, b1, a2.clone(), b2.clone()]);

        let page2 = t
            .store
            .list_threads_by_role(&both, Role::Inbox, 2, 2, SortOrder::DateDesc)
            .await
            .unwrap();
        assert_eq!(
            page2.iter().map(|th| th.id.clone()).collect::<Vec<_>>(),
            vec![a2.clone(), b2.clone()]
        );

        let asc = t
            .store
            .list_threads_by_role(&both, Role::Inbox, 2, 0, SortOrder::DateAsc)
            .await
            .unwrap();
        assert_eq!(
            asc.iter().map(|th| th.id.clone()).collect::<Vec<_>>(),
            vec![b2, a2]
        );
    }

    #[tokio::test]
    async fn threads_by_role_respect_the_account_set() {
        let t = two_accounts().await;
        let (a1, _) = message(&t.store, &t.a.id, 10, &[&t.a_inbox], None).await;
        message(&t.store, &t.b.id, 20, &[&t.b_inbox], None).await;

        let only_a = t
            .store
            .list_threads_by_role(
                std::slice::from_ref(&t.a.id),
                Role::Inbox,
                10,
                0,
                SortOrder::DateDesc,
            )
            .await
            .unwrap();
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].id, a1);

        let none = t
            .store
            .list_threads_by_role(&[], Role::Inbox, 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn role_matches_only_system_labels_of_that_mailbox() {
        let t = two_accounts().await;
        // A user label and a folder that happen to be called "Inbox", and a
        // different system mailbox.
        let fake_user = label(&t.store, &t.b.id, "inbox", LabelKind::User).await;
        let fake_folder = label(&t.store, &t.a.id, "Inbox", LabelKind::Folder).await;
        let (real, _) = message(&t.store, &t.a.id, 1, &[&t.a_inbox], None).await;
        message(&t.store, &t.b.id, 2, &[&fake_user], None).await;
        message(&t.store, &t.a.id, 3, &[&fake_folder], None).await;
        message(&t.store, &t.a.id, 4, &[&t.a_sent], None).await;
        let both = [t.a.id.clone(), t.b.id.clone()];

        let inbox = t
            .store
            .list_threads_by_role(&both, Role::Inbox, 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, real);
        let envs = t
            .store
            .list_envelopes_by_role(&both, Role::Inbox, 10, 0)
            .await
            .unwrap();
        assert_eq!(envs.len(), 1);
    }

    #[tokio::test]
    async fn equal_dates_page_without_repeats_or_gaps() {
        let t = two_accounts().await;
        let when = Utc::now() - Duration::hours(1);
        for i in 0..6 {
            let (acct, lbl) = if i % 2 == 0 {
                (&t.a.id, &t.a_inbox)
            } else {
                (&t.b.id, &t.b_inbox)
            };
            let mut env = TestEnvelopeBuilder::new().account_id(acct.clone()).build();
            env.provider_id = format!("tie-{i}");
            env.date = when;
            t.store.upsert_envelope(&env).await.unwrap();
            t.store
                .set_message_labels(&env.id, &[lbl.id.clone()], EventSource::Sync)
                .await
                .unwrap();
        }
        let both = [t.a.id.clone(), t.b.id.clone()];
        let mut seen = Vec::new();
        for offset in [0, 2, 4] {
            let page = t
                .store
                .list_threads_by_role(&both, Role::Inbox, 2, offset, SortOrder::DateDesc)
                .await
                .unwrap();
            assert_eq!(page.len(), 2);
            seen.extend(page.into_iter().map(|th| th.id));
        }
        let mut unique = seen.clone();
        unique.sort_by_key(|id| id.as_str().clone());
        unique.dedup();
        assert_eq!(
            unique.len(),
            6,
            "paged threads repeated or skipped: {seen:?}"
        );
    }

    #[tokio::test]
    async fn envelopes_by_role_merge_accounts_and_leave_the_label_path_alone() {
        let t = two_accounts().await;
        let (_, a_msg) = message(&t.store, &t.a.id, 10, &[&t.a_inbox], None).await;
        let (_, b_msg) = message(&t.store, &t.b.id, 20, &[&t.b_inbox], None).await;
        message(&t.store, &t.a.id, 30, &[&t.a_sent], None).await;
        let both = [t.a.id.clone(), t.b.id.clone()];

        let merged = t
            .store
            .list_envelopes_by_role(&both, Role::Inbox, 10, 0)
            .await
            .unwrap();
        assert_eq!(
            merged.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            vec![a_msg.clone(), b_msg]
        );
        let paged = t
            .store
            .list_envelopes_by_role(&both, Role::Inbox, 1, 1)
            .await
            .unwrap();
        assert_eq!(paged.len(), 1);
        assert_ne!(paged[0].id, a_msg);

        // The single-id path still sees one account only.
        let single = t
            .store
            .list_envelopes_by_label(&t.a_inbox.id, 10, 0)
            .await
            .unwrap();
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].id, a_msg);
        let single_threads = t
            .store
            .list_threads(None, Some(&t.b_inbox.id), 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert_eq!(single_threads.len(), 1);
        assert_eq!(single_threads[0].account_id, t.b.id);
    }

    #[tokio::test]
    async fn all_mail_role_lists_every_message_of_the_accounts() {
        let t = two_accounts().await;
        let outsider = account(&t.store).await;
        message(&t.store, &t.a.id, 1, &[&t.a_inbox], None).await;
        message(&t.store, &t.b.id, 2, &[], None).await;
        message(&t.store, &outsider.id, 3, &[], None).await;
        let both = [t.a.id.clone(), t.b.id.clone()];

        let envs = t
            .store
            .list_envelopes_by_role(&both, Role::AllMail, 10, 0)
            .await
            .unwrap();
        assert_eq!(envs.len(), 2);
        let threads = t
            .store
            .list_threads_by_role(&both, Role::AllMail, 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert_eq!(threads.len(), 2);
    }

    #[tokio::test]
    async fn a_thread_id_shared_across_accounts_is_listed_once() {
        let t = two_accounts().await;
        // Legacy Gmail thread ids are not account-scoped, so two accounts can
        // hold the same id. `get_threads_batch` keys on it, so the listing
        // returns the thread once rather than twice.
        let shared = ThreadId::new();
        message(&t.store, &t.a.id, 10, &[&t.a_inbox], Some(shared.clone())).await;
        message(&t.store, &t.b.id, 20, &[&t.b_inbox], Some(shared.clone())).await;
        let both = [t.a.id.clone(), t.b.id.clone()];
        let threads = t
            .store
            .list_threads_by_role(&both, Role::Inbox, 10, 0, SortOrder::DateDesc)
            .await
            .unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, shared);
    }
}
