//! One legacy Gmail thread id held by two accounts. A request that names a
//! row's (thread, account) pair acts on, previews and reads that account's
//! copy only; the other account's messages are never touched.

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::{Duration, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection};
use mxr_protocol::{ModeKindData, ThreadAccountRef};

struct Shared {
    fx: Fixture,
    thread: ThreadId,
    a: AccountId,
    b: AccountId,
    /// The question each account received last (what a reply would answer).
    a_answer: Envelope,
    b_answer: Envelope,
}

/// Maya wrote to you in both accounts, in a conversation with one id.
async fn shared() -> Shared {
    let mut config = mxr_config::MxrConfig::default();
    // Done then only records its mark; archiving would need account B's provider.
    config.modes.archive_on_last_done = false;
    let fx = Fixture::with_config(config).await;
    let b = AccountId::new();
    fx.state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(b.clone()))
        .await
        .unwrap();
    let thread = ThreadId::new();
    let mine = fx
        .attention_message(&thread, ME, "maya@example.com", Duration::days(3), None)
        .await;
    let a_answer = fx
        .attention_message(
            &thread,
            "maya@example.com",
            ME,
            Duration::hours(3),
            mine.message_id_header.as_deref(),
        )
        .await;
    let mut b_answer = Envelope {
        id: MessageId::new(),
        account_id: b.clone(),
        provider_id: "shared-b-answer".into(),
        ..a_answer.clone()
    };
    b_answer.message_id_header = Some("<b-answer@example.com>".into());
    fx.store_envelope(&b_answer, MessageDirection::Inbound)
        .await;
    Shared {
        a: fx.account.clone(),
        fx,
        thread,
        b,
        a_answer,
        b_answer,
    }
}

fn pair(s: &Shared, account: &AccountId) -> ThreadAccountRef {
    ThreadAccountRef {
        thread_id: s.thread.clone(),
        account_id: account.clone(),
    }
}

#[tokio::test]
async fn done_on_one_accounts_row_marks_only_that_account() {
    let s = shared().await;
    let run = |dry_run| Request::SetModeDone {
        thread_ids: vec![],
        thread_accounts: vec![pair(&s, &s.b)],
        mode: ModeKindData::Messages,
        dry_run,
        todo_ids: vec![],
        sender: None,
    };
    let ResponseData::ModeDone { items, .. } = request(&s.fx, run(true)).await else {
        std::panic::resume_unwind(Box::new("expected ModeDone"))
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].account_id.as_ref(), Some(&s.b));
    assert!(items[0].error.is_none(), "{:?}", items[0].error);
    let store = &s.fx.state.store;
    assert!(store
        .mode_done_marks(&s.b, "messages")
        .await
        .unwrap()
        .is_empty());

    let ResponseData::ModeDone { items, .. } = request(&s.fx, run(false)).await else {
        std::panic::resume_unwind(Box::new("expected ModeDone"))
    };
    assert!(items[0].marked);
    assert_eq!(
        store.mode_done_marks(&s.b, "messages").await.unwrap().len(),
        1
    );
    assert!(
        store
            .mode_done_marks(&s.a, "messages")
            .await
            .unwrap()
            .is_empty(),
        "the other account's row is not done"
    );
}

#[tokio::test]
async fn deferring_one_accounts_row_flags_only_that_accounts_message() {
    let s = shared().await;
    let until = Utc::now() + Duration::days(2);
    let defer = |account: &AccountId, dry_run| Request::DeferThreads {
        thread_ids: vec![],
        thread_accounts: vec![pair(&s, account)],
        until,
        dry_run,
    };
    let ResponseData::ThreadsDeferred { items, .. } = request(&s.fx, defer(&s.b, true)).await
    else {
        std::panic::resume_unwind(Box::new("expected ThreadsDeferred"))
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].message_id.as_ref(), Some(&s.b_answer.id));

    let ResponseData::ThreadsDeferred { items, .. } = request(&s.fx, defer(&s.b, false)).await
    else {
        std::panic::resume_unwind(Box::new("expected ThreadsDeferred"))
    };
    assert_eq!(items[0].message_id.as_ref(), Some(&s.b_answer.id));
    let flagged =
        s.fx.state
            .store
            .list_reply_later(until + Duration::minutes(1))
            .await
            .unwrap();
    assert_eq!(flagged, vec![s.b_answer.id.clone()]);
    assert!(!flagged.contains(&s.a_answer.id));
}

#[tokio::test]
async fn dismissing_and_restoring_one_accounts_row_leaves_the_other() {
    let s = shared().await;
    let ResponseData::DeskThreadsDismissed { threads, .. } = request(
        &s.fx,
        Request::DismissDeskThreads {
            thread_ids: vec![],
            thread_accounts: vec![pair(&s, &s.a)],
            dry_run: false,
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected DeskThreadsDismissed"))
    };
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].account_id, s.a);

    // The other account's row, dismissed too, then only A's restored.
    request(
        &s.fx,
        Request::DismissDeskThreads {
            thread_ids: vec![],
            thread_accounts: vec![pair(&s, &s.b)],
            dry_run: false,
        },
    )
    .await;
    let ResponseData::DeskThreadsRestored { restored } = request(
        &s.fx,
        Request::RestoreDeskThreads {
            thread_ids: vec![],
            thread_accounts: vec![pair(&s, &s.a)],
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected DeskThreadsRestored"))
    };
    assert_eq!(restored, 1);
    let left: Vec<String> = sqlx::query_scalar("SELECT account_id FROM desk_dismissals")
        .fetch_all(s.fx.state.store.reader())
        .await
        .unwrap();
    assert_eq!(left, vec![s.b.as_str()]);
}

#[tokio::test]
async fn previews_and_membership_answer_for_the_named_account() {
    let s = shared().await;
    let ResponseData::ThreadPreviews { previews } = request(
        &s.fx,
        Request::GetThreadPreviews {
            thread_ids: vec![],
            thread_accounts: vec![pair(&s, &s.b)],
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected ThreadPreviews"))
    };
    assert_eq!(previews.len(), 1);
    assert_eq!(previews[0].account_id.as_ref(), Some(&s.b));
    assert_eq!(previews[0].message_count, 1, "B holds one message");

    let ResponseData::ThreadPreviews { previews } = request(
        &s.fx,
        Request::GetThreadPreviews {
            thread_ids: vec![s.thread.clone()],
            thread_accounts: vec![],
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected ThreadPreviews"))
    };
    assert_eq!(previews.len(), 2, "a bare id previews each account's copy");

    let ResponseData::ModeMembership { threads } = request(
        &s.fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: None,
            thread_ids: vec![],
            thread_accounts: vec![pair(&s, &s.b), pair(&s, &s.a)],
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected ModeMembership"))
    };
    let accounts: Vec<_> = threads.iter().map(|t| t.account_id.clone()).collect();
    assert_eq!(
        accounts,
        vec![s.b.clone(), s.a.clone()],
        "request order, per account"
    );
}

#[tokio::test]
async fn the_thread_reads_stay_on_the_named_account() {
    let s = shared().await;
    let ResponseData::ThreadContext { context } = request(
        &s.fx,
        Request::GetThreadContext {
            thread_id: s.thread.clone(),
            account_id: Some(s.b.clone()),
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected ThreadContext"))
    };
    assert_eq!(context.account_id, s.b);

    let ResponseData::ExportResult { content } = request(
        &s.fx,
        Request::ExportThread {
            thread_id: s.thread.clone(),
            format: mxr_core::types::ExportFormat::Json,
            account_id: Some(s.b.clone()),
        },
    )
    .await
    else {
        std::panic::resume_unwind(Box::new("expected ExportResult"))
    };
    assert!(content.contains(&s.b_answer.id.to_string()));
    assert!(!content.contains(&s.a_answer.id.to_string()));
}
