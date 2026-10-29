//! A legacy Gmail thread id can be held by two accounts. Every store read and
//! write on the thread-action paths must stay on the account a row names.

use super::*;
use crate::{DeskDismissal, ModeDoneMark, ThreadSummaryRecord};

struct Shared {
    store: Store,
    a: AccountId,
    b: AccountId,
    thread: ThreadId,
    a_messages: Vec<MessageId>,
    b_messages: Vec<MessageId>,
}

async fn shared() -> Shared {
    let store = Store::in_memory().await.unwrap();
    let first = test_account();
    let mut second = test_account();
    second.id = AccountId::new();
    second.email = "second@example.com".into();
    second.name = "Second".into();
    store.insert_account(&first).await.unwrap();
    store.insert_account(&second).await.unwrap();
    let thread = ThreadId::new();
    let base = chrono::Utc::now();
    let mut a_messages = Vec::new();
    let mut b_messages = Vec::new();
    for (account, provider, hours) in [
        (&first.id, "a-1", 3),
        (&first.id, "a-2", 2),
        (&second.id, "b-1", 1),
    ] {
        let mut env = test_envelope(account);
        env.id = MessageId::new();
        env.thread_id = thread.clone();
        env.provider_id = provider.to_string();
        env.date = base - chrono::Duration::hours(hours);
        store.upsert_envelope(&env).await.unwrap();
        if account == &first.id {
            a_messages.push(env.id);
        } else {
            b_messages.push(env.id);
        }
    }
    Shared {
        store,
        a: first.id,
        b: second.id,
        thread,
        a_messages,
        b_messages,
    }
}

fn ids(envelopes: &[Envelope]) -> Vec<MessageId> {
    let mut ids: Vec<_> = envelopes.iter().map(|e| e.id.clone()).collect();
    ids.sort_by_key(MessageId::as_str);
    ids
}

fn sorted(mut ids: Vec<MessageId>) -> Vec<MessageId> {
    ids.sort_by_key(MessageId::as_str);
    ids
}

#[tokio::test]
async fn a_thread_reads_only_the_named_accounts_messages() {
    let s = shared().await;
    let a = s
        .store
        .get_account_thread_envelopes(&s.a, &s.thread)
        .await
        .unwrap();
    assert_eq!(ids(&a), sorted(s.a_messages.clone()));
    let b = s
        .store
        .get_thread_envelopes_for_model_in(&s.thread, Some(&s.b))
        .await
        .unwrap();
    assert_eq!(ids(&b), sorted(s.b_messages.clone()));
    // Older clients name no account and keep reading both.
    let both = s
        .store
        .get_thread_envelopes_in(&s.thread, None)
        .await
        .unwrap();
    assert_eq!(both.len(), 3);
}

#[tokio::test]
async fn two_accounts_keep_their_own_summary_of_a_shared_thread() {
    let s = shared().await;
    for (account, text) in [(&s.a, "about a"), (&s.b, "about b")] {
        s.store
            .upsert_thread_summary(&ThreadSummaryRecord {
                thread_id: s.thread.clone(),
                account_id: account.clone(),
                content_hash: "h".into(),
                text: text.into(),
                model: "m".into(),
                generated_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
    }
    let a = s
        .store
        .get_thread_summary(&s.a, &s.thread)
        .await
        .unwrap()
        .unwrap();
    let b = s
        .store
        .get_thread_summary(&s.b, &s.thread)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((a.text.as_str(), b.text.as_str()), ("about a", "about b"));
}

#[tokio::test]
async fn thread_summaries_are_keyed_by_account_and_thread() {
    let s = shared().await;
    let key: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('thread_summaries') WHERE pk > 0 ORDER BY pk",
    )
    .fetch_all(s.store.reader())
    .await
    .unwrap();
    assert_eq!(key, vec!["account_id", "thread_id"]);
}

async fn dismissed(s: &Shared) -> Vec<AccountId> {
    let mut accounts: Vec<String> =
        sqlx::query_scalar("SELECT account_id FROM desk_dismissals WHERE thread_id = ?")
            .bind(s.thread.as_str())
            .fetch_all(s.store.reader())
            .await
            .unwrap();
    accounts.sort();
    accounts
        .iter()
        .map(|a| a.parse::<AccountId>().unwrap())
        .collect()
}

#[tokio::test]
async fn dismiss_and_restore_act_on_the_named_account_only() {
    let s = shared().await;
    let a_only = [(s.thread.clone(), Some(s.a.clone()))];

    // The preview selects exactly what the write acts on.
    let preview = s.store.dismiss_desk_threads(&a_only, true).await.unwrap();
    assert_eq!(preview, vec![(s.a.clone(), s.thread.clone())]);
    assert!(dismissed(&s).await.is_empty(), "a dry run writes nothing");
    let written = s.store.dismiss_desk_threads(&a_only, false).await.unwrap();
    assert_eq!(written, preview);
    assert_eq!(dismissed(&s).await, vec![s.a.clone()]);

    // A bare id keeps dismissing every account that holds it.
    let bare = [(s.thread.clone(), None)];
    s.store.dismiss_desk_threads(&bare, false).await.unwrap();
    assert_eq!(dismissed(&s).await.len(), 2);

    let restored = s
        .store
        .restore_desk_threads(&[(s.thread.clone(), Some(s.b.clone()))])
        .await
        .unwrap();
    assert_eq!(restored, 1);
    assert_eq!(dismissed(&s).await, vec![s.a.clone()]);
}

#[tokio::test]
async fn clearing_a_mode_done_mark_leaves_the_other_accounts_mark() {
    let s = shared().await;
    let mark = |account: &AccountId| ModeDoneMark {
        account_id: account.clone(),
        thread_id: s.thread.clone(),
        mode: "messages".to_string(),
        through: DeskDismissal {
            through_date: 7,
            through_id: uuid::Uuid::now_v7(),
            through_count: 1,
            covered: None,
        },
    };
    s.store
        .mark_mode_done(&[mark(&s.a), mark(&s.b)])
        .await
        .unwrap();
    let cleared = s
        .store
        .clear_mode_done(&[(s.thread.clone(), Some(s.a.clone()))], "messages")
        .await
        .unwrap();
    assert_eq!(cleared, 1);
    assert!(s
        .store
        .mode_done_marks(&s.a, "messages")
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        s.store
            .mode_done_marks(&s.b, "messages")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn previews_describe_each_accounts_copy_on_its_own() {
    let s = shared().await;
    let previews = s
        .store
        .thread_previews(&[(s.thread.clone(), None)])
        .await
        .unwrap();
    assert_eq!(previews.len(), 2, "a bare id previews each account's copy");
    let count = |account: &AccountId| {
        previews
            .iter()
            .find(|p| &p.account_id == account)
            .unwrap()
            .message_count
    };
    assert_eq!((count(&s.a), count(&s.b)), (2, 1));

    let only_b = s
        .store
        .thread_previews(&[(s.thread.clone(), Some(s.b.clone()))])
        .await
        .unwrap();
    assert_eq!(only_b.len(), 1);
    assert_eq!(only_b[0].account_id, s.b);
    assert_eq!(only_b[0].message_count, 1);
}
