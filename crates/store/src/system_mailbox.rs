//! Listing and counting a system mailbox (Inbox, Sent, ...) across a set of
//! accounts, for the "All accounts" views.
//!
//! The single-label queries (`list_threads`, `list_envelopes_by_label`) stay
//! the path for an explicit label id. These match labels by role instead:
//! every System label in the account set whose name maps to the role through
//! `Role::system_label_names`. They are unchecked queries so they need no
//! `.sqlx` cache entry.

use crate::message::{future_date_cutoff_timestamp, sqlite_row_to_envelope};
use crate::{decode_id, decode_timestamp, trace_query};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, Role, SortOrder, Thread};
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

/// How a role selects messages (`?3`): by System label (0), every message
/// of the accounts for All Mail (1), or the starred flag for Starred (2).
/// IMAP keeps `\Flagged` only as `MessageFlags::STARRED` with no label, so a
/// label match would miss those accounts. Gmail sync sets the same flag from
/// the STARRED label, and a local star or unstar changes only the flag, so the
/// flag is the one truth for every provider.
fn role_mode(role: Role) -> i64 {
    match role {
        Role::AllMail => 1,
        Role::Starred => 2,
        _ => 0,
    }
}

/// Messages that belong to the role in one of the accounts (`?1` accounts,
/// `?2` label names, `?3` the mode above). A macro so the final SQL stays a
/// literal.
macro_rules! role_filter {
    () => {
        "(?3 = 1 OR (?3 = 2 AND (m.flags & 2) != 0) OR (?3 = 0 AND m.id IN (
            SELECT ml.message_id FROM message_labels ml
            JOIN labels l ON l.id = ml.label_id
            WHERE l.kind = 'system'
              AND l.account_id IN (SELECT value FROM json_each(?1))
              AND lower(l.name) IN (SELECT value FROM json_each(?2))
        )))"
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
    /// Hydrate threads from the messages of `account_ids` only. A legacy
    /// thread id can span accounts, and `get_threads_batch` aggregates over
    /// all of them, which would leak a disabled or out-of-scope account's
    /// subject, snippet and counts into the row.
    pub async fn hydrate_threads_in_accounts(
        &self,
        thread_ids: &[ThreadId],
        account_ids: &[AccountId],
    ) -> Result<Vec<Thread>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let threads_json = json_array(thread_ids.iter().map(ThreadId::as_str))?;
        let accounts_json = account_ids_json(account_ids)?;
        let cutoff = future_date_cutoff_timestamp();

        let aggregates = sqlx::query(
            "SELECT thread_id, account_id,
                    MIN(subject) AS subject,
                    COUNT(*) AS message_count,
                    SUM(CASE WHEN (flags & 1) = 0 THEN 1 ELSE 0 END) AS unread_count,
                    MAX(CASE WHEN date > ?3 THEN 0 ELSE date END) AS latest_date,
                    snippet
             FROM messages
             WHERE thread_id IN (SELECT value FROM json_each(?1))
               AND account_id IN (SELECT value FROM json_each(?2))
             GROUP BY thread_id",
        )
        .bind(&threads_json)
        .bind(&accounts_json)
        .bind(cutoff)
        .fetch_all(self.reader())
        .await?;

        let participants = sqlx::query(
            "SELECT DISTINCT thread_id, from_name, from_email
             FROM messages
             WHERE thread_id IN (SELECT value FROM json_each(?1))
               AND account_id IN (SELECT value FROM json_each(?2))",
        )
        .bind(&threads_json)
        .bind(&accounts_json)
        .fetch_all(self.reader())
        .await?;
        let mut participants_by_thread: std::collections::HashMap<String, Vec<Address>> =
            std::collections::HashMap::new();
        for row in participants {
            participants_by_thread
                .entry(row.get::<String, _>("thread_id"))
                .or_default()
                .push(Address {
                    name: row.get::<Option<String>, _>("from_name"),
                    email: row.get::<String, _>("from_email"),
                });
        }

        let members = sqlx::query(
            "SELECT thread_id, id FROM messages
             WHERE thread_id IN (SELECT value FROM json_each(?1))
               AND account_id IN (SELECT value FROM json_each(?2))
             ORDER BY thread_id ASC,
                      CASE WHEN date > ?3 THEN 0 ELSE date END ASC,
                      id ASC",
        )
        .bind(&threads_json)
        .bind(&accounts_json)
        .bind(cutoff)
        .fetch_all(self.reader())
        .await?;
        let mut message_ids_by_thread: std::collections::HashMap<String, Vec<MessageId>> =
            std::collections::HashMap::new();
        for row in members {
            message_ids_by_thread
                .entry(row.get::<String, _>("thread_id"))
                .or_default()
                .push(decode_id(&row.get::<String, _>("id"))?);
        }

        aggregates
            .into_iter()
            .map(|row| {
                let thread_id: String = row.get("thread_id");
                Ok(Thread {
                    id: decode_id(&thread_id)?,
                    account_id: decode_id(&row.get::<String, _>("account_id"))?,
                    subject: row.get("subject"),
                    participants: participants_by_thread
                        .remove(&thread_id)
                        .unwrap_or_default(),
                    message_count: row.get::<i64, _>("message_count") as u32,
                    unread_count: row.get::<i64, _>("unread_count") as u32,
                    latest_date: decode_timestamp(row.get::<i64, _>("latest_date"))?,
                    snippet: row.get("snippet"),
                    message_ids: message_ids_by_thread.remove(&thread_id).unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Unread and total per system mailbox, summed over `account_ids`.
    /// Label-backed mailboxes add up the stored label counts, which is what
    /// each account's sidebar already shows. All Mail and Starred have no
    /// label to count on every provider (an account may have no All Mail
    /// label; IMAP stars are a flag), so they are counted from the messages.
    pub async fn system_mailbox_counts(
        &self,
        account_ids: &[AccountId],
    ) -> Result<Vec<(Role, u32, u32)>, sqlx::Error> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let accounts_json = account_ids_json(account_ids)?;
        let mut counts: std::collections::HashMap<Role, (u32, u32)> =
            std::collections::HashMap::new();

        let labels = sqlx::query(
            "SELECT name, unread_count, total_count FROM labels
             WHERE kind = 'system'
               AND account_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(&accounts_json)
        .fetch_all(self.reader())
        .await?;
        for row in labels {
            let name: String = row.get("name");
            let Some(role) = Role::from_system_label_name(&name) else {
                continue;
            };
            if matches!(role, Role::AllMail | Role::Starred) {
                continue;
            }
            let entry = counts.entry(role).or_default();
            entry.0 += row.get::<i64, _>("unread_count") as u32;
            entry.1 += row.get::<i64, _>("total_count") as u32;
        }

        for role in [Role::AllMail, Role::Starred] {
            let started_at = std::time::Instant::now();
            let row = sqlx::query(concat!(
                "SELECT COUNT(*) AS total,
                        COALESCE(SUM(CASE WHEN (m.flags & 1) = 0 THEN 1 ELSE 0 END), 0) AS unread
                 FROM messages m
                 WHERE m.account_id IN (SELECT value FROM json_each(?1))
                   AND ",
                role_filter!()
            ))
            .bind(&accounts_json)
            .bind(label_names_json(role)?)
            .bind(role_mode(role))
            .fetch_one(self.reader())
            .await?;
            trace_query("system_mailbox.count", started_at, 1);
            counts.insert(
                role,
                (
                    row.get::<i64, _>("unread") as u32,
                    row.get::<i64, _>("total") as u32,
                ),
            );
        }

        let mut counts = counts
            .into_iter()
            .map(|(role, (unread, total))| (role, unread, total))
            .collect::<Vec<_>>();
        counts.sort_by_key(|(role, _, _)| role.system_mailbox_wire_name());
        Ok(counts)
    }

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
            .bind(role_mode(role))
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
        let mut threads = self
            .hydrate_threads_in_accounts(&thread_ids, account_ids)
            .await?;
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
        .bind(role_mode(role))
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
    use mxr_core::types::{EventSource, Label, LabelKind, MessageFlags, Role, SortOrder};
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

    /// A message with `flags`, in `account_id`, carrying `labels`.
    async fn flagged(
        store: &Store,
        account_id: &AccountId,
        flags: MessageFlags,
        labels: &[&Label],
        subject: &str,
    ) -> ThreadId {
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .flags(flags)
            .build();
        env.provider_id = format!("flagged-{}", env.id);
        env.subject = subject.to_string();
        env.snippet = format!("snippet of {subject}");
        store.upsert_envelope(&env).await.unwrap();
        let ids = labels.iter().map(|l| l.id.clone()).collect::<Vec<_>>();
        store
            .set_message_labels(&env.id, &ids, EventSource::Sync)
            .await
            .unwrap();
        env.thread_id
    }

    #[tokio::test]
    async fn starred_is_the_flag_for_gmail_and_imap_alike_and_unstar_removes_the_row() {
        // Gmail-shaped account: the STARRED label and, from sync, the flag.
        // IMAP-shaped account: \Flagged is only the flag, no STARRED label.
        let store = Store::in_memory().await.unwrap();
        let gmail = account(&store).await;
        let imap = account(&store).await;
        let gmail_starred = label(&store, &gmail.id, "STARRED", LabelKind::System).await;
        let gmail_thread = flagged(
            &store,
            &gmail.id,
            MessageFlags::STARRED,
            &[&gmail_starred],
            "gmail starred",
        )
        .await;
        let imap_thread =
            flagged(&store, &imap.id, MessageFlags::STARRED, &[], "imap flagged").await;
        flagged(&store, &imap.id, MessageFlags::empty(), &[], "imap plain").await;
        let both = [gmail.id.clone(), imap.id.clone()];
        let listed = || async {
            let threads = store
                .list_threads_by_role(&both, Role::Starred, 10, 0, SortOrder::DateDesc)
                .await
                .unwrap();
            let mut ids = threads.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
            ids.sort_by_key(|id| id.as_str().clone());
            ids
        };
        let mut want = vec![gmail_thread.clone(), imap_thread.clone()];
        want.sort_by_key(|id| id.as_str().clone());
        assert_eq!(listed().await, want);
        assert_eq!(
            store
                .list_envelopes_by_role(&both, Role::Starred, 10, 0)
                .await
                .unwrap()
                .len(),
            2
        );
        let counts = store.system_mailbox_counts(&both).await.unwrap();
        let starred = counts.iter().find(|(r, _, _)| *r == Role::Starred).unwrap();
        assert_eq!((starred.1, starred.2), (2, 2), "unread, total");

        // A local unstar clears only the flag. The Gmail message still has
        // its STARRED label association until the next sync, and must leave
        // the merged Starred list anyway.
        for thread in [&gmail_thread, &imap_thread] {
            let message = store.get_thread_envelopes(thread).await.unwrap().remove(0);
            store
                .set_starred(&message.id, false, EventSource::User)
                .await
                .unwrap();
        }
        assert!(listed().await.is_empty());
        let counts = store.system_mailbox_counts(&both).await.unwrap();
        let starred = counts.iter().find(|(r, _, _)| *r == Role::Starred).unwrap();
        assert_eq!(starred.2, 0);
    }

    #[tokio::test]
    async fn counts_cover_accounts_without_an_all_mail_label() {
        let t = two_accounts().await;
        t.store
            .update_label_counts(&t.a_inbox.id, 2, 5)
            .await
            .unwrap();
        t.store
            .update_label_counts(&t.b_inbox.id, 1, 7)
            .await
            .unwrap();
        // Only account A has an All Mail label (and its stored count is stale).
        let all = label(&t.store, &t.a.id, "ALL", LabelKind::System).await;
        t.store.update_label_counts(&all.id, 0, 1).await.unwrap();
        for _ in 0..3 {
            message(&t.store, &t.a.id, 5, &[&t.a_inbox], None).await;
        }
        for _ in 0..4 {
            message(&t.store, &t.b.id, 5, &[&t.b_inbox], None).await;
        }
        let both = [t.a.id.clone(), t.b.id.clone()];

        let counts = t.store.system_mailbox_counts(&both).await.unwrap();
        let get = |role| counts.iter().find(|(r, _, _)| *r == role).copied().unwrap();
        assert_eq!((get(Role::Inbox).1, get(Role::Inbox).2), (3, 12));
        // Every message of both accounts, whatever labels exist.
        assert_eq!(get(Role::AllMail).2, 7);
        // One account only counts that account.
        let only_b = t
            .store
            .system_mailbox_counts(std::slice::from_ref(&t.b.id))
            .await
            .unwrap();
        let b_all = only_b.iter().find(|(r, _, _)| *r == Role::AllMail).unwrap();
        assert_eq!(b_all.2, 4);
        assert!(t.store.system_mailbox_counts(&[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_shared_thread_id_never_leaks_an_out_of_scope_account() {
        let t = two_accounts().await;
        let outsider = account(&t.store).await;
        let outsider_inbox = label(&t.store, &outsider.id, "INBOX", LabelKind::System).await;
        let shared = ThreadId::new();
        let mut visible = TestEnvelopeBuilder::new()
            .account_id(t.a.id.clone())
            .build();
        visible.thread_id = shared.clone();
        visible.provider_id = "visible".into();
        visible.subject = "Visible subject".into();
        visible.snippet = "visible snippet".into();
        visible.date = Utc::now() - Duration::minutes(10);
        t.store.upsert_envelope(&visible).await.unwrap();
        t.store
            .set_message_labels(&visible.id, &[t.a_inbox.id.clone()], EventSource::Sync)
            .await
            .unwrap();
        let mut hidden = TestEnvelopeBuilder::new()
            .account_id(outsider.id.clone())
            .build();
        hidden.thread_id = shared.clone();
        hidden.provider_id = "hidden".into();
        hidden.subject = "Aaa hidden subject".into();
        hidden.snippet = "hidden snippet".into();
        hidden.from = mxr_core::types::Address {
            name: None,
            email: "secret@outsider.example".into(),
        };
        hidden.date = Utc::now();
        t.store.upsert_envelope(&hidden).await.unwrap();
        t.store
            .set_message_labels(&hidden.id, &[outsider_inbox.id.clone()], EventSource::Sync)
            .await
            .unwrap();

        let threads = t
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
        assert_eq!(threads.len(), 1);
        let thread = &threads[0];
        assert_eq!(thread.subject, "Visible subject");
        assert_eq!(thread.snippet, "visible snippet");
        assert_eq!(thread.account_id, t.a.id);
        assert_eq!(thread.message_count, 1);
        assert_eq!(thread.message_ids, vec![visible.id.clone()]);
        assert!(thread
            .participants
            .iter()
            .all(|p| p.email != "secret@outsider.example"));
    }
}
