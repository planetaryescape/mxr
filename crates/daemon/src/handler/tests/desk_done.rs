//! The desk's Done (`ResolveDeskItems`) per lane, its preview, its undo,
//! and what brings a conversation back.

use super::desk::{request, thread_ids, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, EventSource, MessageDirection, MessageFlags};
use mxr_protocol::{DeskDoneItemData, DeskDoneOutcomeData, DeskLaneKind};
use mxr_store::CommitmentStatus;

fn item(thread: &ThreadId, lane: Option<DeskLaneKind>) -> DeskDoneItemData {
    DeskDoneItemData {
        thread_id: thread.clone(),
        lane,
        commitment_id: None,
    }
}

async fn done(
    fx: &Fixture,
    items: Vec<DeskDoneItemData>,
    dry_run: bool,
) -> (Vec<DeskDoneOutcomeData>, Option<String>) {
    match request(fx, Request::ResolveDeskItems { items, dry_run }).await {
        ResponseData::DeskItemsResolved {
            items,
            dry_run: echoed,
            mutation_id,
            undo_unavailable,
        } => {
            assert_eq!(echoed, dry_run);
            assert!(!undo_unavailable);
            (items, mutation_id)
        }
        other => panic!("expected DeskItemsResolved, got {other:?}"),
    }
}

async fn undo(fx: &Fixture, mutation_id: String) {
    assert!(matches!(
        request(fx, Request::UndoMutation { mutation_id }).await,
        ResponseData::Ack
    ));
}

async fn envelope(fx: &Fixture, id: &MessageId) -> Envelope {
    fx.state.store.get_envelope(id).await.unwrap().unwrap()
}

fn in_inbox(envelope: &Envelope) -> bool {
    envelope.label_provider_ids.iter().any(|l| l == "INBOX")
}

fn read(envelope: &Envelope) -> bool {
    envelope.flags.contains(MessageFlags::READ)
}

async fn lanes(fx: &Fixture) -> [Vec<ThreadId>; 4] {
    match fx.desk(25).await {
        ResponseData::Desk {
            owed,
            due,
            waiting,
            people_new,
            ..
        } => [
            thread_ids(&owed),
            thread_ids(&due),
            thread_ids(&waiting),
            thread_ids(&people_new),
        ],
        other => panic!("expected a desk, got {other:?}"),
    }
}

/// You wrote Maya; she answered twice, the first already read.
async fn owed_thread(fx: &Fixture) -> (ThreadId, Envelope, Envelope) {
    let thread = ThreadId::new();
    let mine = fx
        .message(&thread, ME, "maya@example.com", Duration::days(2), None)
        .await;
    let mut seen = fx
        .message(
            &thread,
            "maya@example.com",
            ME,
            Duration::days(1),
            mine.message_id_header.as_deref(),
        )
        .await;
    seen.flags |= MessageFlags::READ;
    fx.store_envelope(&seen, MessageDirection::Inbound).await;
    let fresh = fx
        .message(
            &thread,
            "maya@example.com",
            ME,
            Duration::hours(3),
            seen.message_id_header.as_deref(),
        )
        .await;
    (thread, seen, fresh)
}

#[tokio::test]
async fn done_on_an_owed_thread_archives_marks_read_and_undo_restores_each_message() {
    let fx = Fixture::new().await;
    let (thread, seen, fresh) = owed_thread(&fx).await;
    assert_eq!(lanes(&fx).await[0], vec![thread.clone()]);

    // The preview says exactly what the run does and changes nothing.
    let (preview, no_undo) = done(&fx, vec![item(&thread, Some(DeskLaneKind::Owed))], true).await;
    assert!(no_undo.is_none());
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].lane, DeskLaneKind::Owed);
    assert_eq!(preview[0].archived, 2);
    assert_eq!(preview[0].marked_read, 1);
    assert!(preview[0].dismissed);
    assert_eq!(preview[0].account_id.as_ref(), Some(&fx.account));
    assert!(preview[0].error.is_none());
    assert_eq!(lanes(&fx).await[0], vec![thread.clone()]);
    assert!(!read(&envelope(&fx, &fresh.id).await));

    let (outcome, mutation_id) =
        done(&fx, vec![item(&thread, Some(DeskLaneKind::Owed))], false).await;
    assert_eq!(outcome, preview, "the run matches its preview");
    let mutation_id = mutation_id.expect("an undo id");
    for id in [&seen.id, &fresh.id] {
        let after = envelope(&fx, id).await;
        assert!(!in_inbox(&after), "archived");
        assert!(read(&after), "marked read");
    }
    assert!(lanes(&fx).await.iter().all(Vec::is_empty), "off the desk");

    // Put back in the inbox by hand, it still stays off the desk: nobody
    // wrote since it was put away.
    fx.state
        .store
        .set_message_labels(
            &fresh.id,
            std::slice::from_ref(&fx.inbox),
            EventSource::User,
        )
        .await
        .unwrap();
    assert!(lanes(&fx).await.iter().all(Vec::is_empty));
    fx.state
        .store
        .set_message_labels(&fresh.id, &[], EventSource::User)
        .await
        .unwrap();

    // Undo brings it back exactly as it was: both in the inbox, the read
    // one read and the unread one unread, and owed again.
    undo(&fx, mutation_id).await;
    let seen_after = envelope(&fx, &seen.id).await;
    let fresh_after = envelope(&fx, &fresh.id).await;
    assert!(in_inbox(&seen_after) && in_inbox(&fresh_after));
    assert!(read(&seen_after));
    assert!(!read(&fresh_after));
    assert_eq!(lanes(&fx).await[0], vec![thread]);
}

#[tokio::test]
async fn a_new_message_brings_a_done_thread_back() {
    let fx = Fixture::new().await;
    let (thread, _, fresh) = owed_thread(&fx).await;
    done(&fx, vec![item(&thread, Some(DeskLaneKind::Owed))], false).await;
    assert!(lanes(&fx).await[0].is_empty());

    fx.message(
        &thread,
        "maya@example.com",
        ME,
        Duration::minutes(5),
        fresh.message_id_header.as_deref(),
    )
    .await;
    assert_eq!(lanes(&fx).await[0], vec![thread]);
}

#[tokio::test]
async fn done_on_new_from_people_puts_it_away() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let hello = fx
        .message(&thread, "iris@example.com", ME, Duration::hours(2), None)
        .await;
    assert_eq!(lanes(&fx).await[3], vec![thread.clone()]);

    let (outcome, _) = done(
        &fx,
        vec![item(&thread, Some(DeskLaneKind::PeopleNew))],
        false,
    )
    .await;
    assert_eq!((outcome[0].archived, outcome[0].marked_read), (1, 1));
    let after = envelope(&fx, &hello.id).await;
    assert!(!in_inbox(&after) && read(&after));
    assert!(lanes(&fx).await.iter().all(Vec::is_empty));
}

#[tokio::test]
async fn done_on_waiting_marks_read_without_archiving_and_undo_restores_the_prior_dismissal() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let question = fx
        .message(&thread, "jon@example.com", ME, Duration::days(5), None)
        .await;
    let mine = fx
        .message(
            &thread,
            ME,
            "jon@example.com",
            Duration::days(4),
            question.message_id_header.as_deref(),
        )
        .await;
    // Done waiting once, then a follow-up of yours made it wait again.
    request(
        &fx,
        Request::DismissDeskThreads {
            thread_ids: vec![thread.clone()],
            dry_run: false,
        },
    )
    .await;
    fx.message(
        &thread,
        ME,
        "jon@example.com",
        Duration::days(1),
        mine.message_id_header.as_deref(),
    )
    .await;
    assert_eq!(lanes(&fx).await[2], vec![thread.clone()]);
    let pair = [(fx.account.clone(), thread.clone())];
    let before = fx.state.store.desk_dismissal_priors(&pair).await.unwrap();
    assert!(before[0].prior.is_some());

    // No lane named: you wrote last, so it is Waiting on.
    let (outcome, mutation_id) = done(&fx, vec![item(&thread, None)], false).await;
    assert_eq!(outcome[0].lane, DeskLaneKind::Waiting);
    assert_eq!((outcome[0].archived, outcome[0].marked_read), (0, 1));
    assert!(outcome[0].dismissed);
    let after = envelope(&fx, &question.id).await;
    assert!(in_inbox(&after), "waiting is never archived");
    assert!(read(&after));
    assert!(lanes(&fx).await.iter().all(Vec::is_empty));

    undo(&fx, mutation_id.unwrap()).await;
    assert!(!read(&envelope(&fx, &question.id).await));
    assert_eq!(
        fx.state.store.desk_dismissal_priors(&pair).await.unwrap(),
        before,
        "the earlier dismissal is back as it was"
    );
    assert_eq!(lanes(&fx).await[2], vec![thread]);
}

#[tokio::test]
async fn done_on_a_promise_resolves_it_marks_its_thread_read_and_undo_reopens_it() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let ask = fx
        .message(&thread, "nora@example.com", ME, Duration::days(3), None)
        .await;
    let promise = fx
        .message(
            &thread,
            ME,
            "nora@example.com",
            Duration::days(2),
            ask.message_id_header.as_deref(),
        )
        .await;
    fx.promise(&thread, &promise.id, Duration::days(1)).await;
    let commitment = format!("c-{thread}");
    assert_eq!(lanes(&fx).await[1], vec![thread.clone()]);

    // Due without its promise is refused, item by item.
    let (refused, none) = done(&fx, vec![item(&thread, Some(DeskLaneKind::Due))], false).await;
    assert!(refused[0]
        .error
        .as_deref()
        .unwrap()
        .contains("commitment id"));
    assert!(none.is_none());

    let due = DeskDoneItemData {
        thread_id: thread.clone(),
        lane: Some(DeskLaneKind::Due),
        commitment_id: Some(commitment.clone()),
    };
    let (outcome, mutation_id) = done(&fx, vec![due], false).await;
    assert_eq!(
        outcome[0].resolved_commitment_id.as_deref(),
        Some(&*commitment)
    );
    assert_eq!((outcome[0].archived, outcome[0].marked_read), (0, 1));
    assert!(outcome[0].dismissed, "put away like every lane");
    let status = |fx: &Fixture| {
        let store = fx.state.store.clone();
        let commitment = commitment.clone();
        async move {
            store
                .get_contact_commitment(&commitment)
                .await
                .unwrap()
                .unwrap()
                .status
        }
    };
    assert_eq!(status(&fx).await, CommitmentStatus::Resolved);
    let after = envelope(&fx, &ask.id).await;
    assert!(read(&after));
    assert!(in_inbox(&after), "a promise's thread is not archived");
    // You wrote last, so without the dismissal it would show under
    // Waiting on the moment the promise was kept.
    assert!(lanes(&fx).await.iter().all(Vec::is_empty), "off the desk");

    undo(&fx, mutation_id.unwrap()).await;
    assert_eq!(status(&fx).await, CommitmentStatus::Open);
    assert!(!read(&envelope(&fx, &ask.id).await));
    assert_eq!(lanes(&fx).await[1], vec![thread]);
}

#[tokio::test]
async fn an_item_that_cannot_change_stays_on_the_desk_for_a_retry() {
    let fx = Fixture::new().await;
    let (thread, template, _) = owed_thread(&fx).await;

    // A conversation in an account with no provider right now.
    let offline = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Offline".into(),
        email: "offline@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&offline).await.unwrap();
    let stuck_thread = ThreadId::new();
    let mut stuck = template.clone();
    stuck.id = MessageId::new();
    stuck.account_id = offline.id.clone();
    stuck.thread_id = stuck_thread.clone();
    stuck.provider_id = format!("offline-{}", stuck.id);
    stuck.message_id_header = Some(format!("<{}@example.com>", stuck.id));
    stuck.flags = MessageFlags::empty();
    fx.state
        .store
        .upsert_envelope_with_direction(&stuck, MessageDirection::Inbound)
        .await
        .unwrap();

    let unknown = ThreadId::new();
    let (outcome, mutation_id) = done(
        &fx,
        vec![
            item(&thread, Some(DeskLaneKind::Owed)),
            item(&stuck_thread, Some(DeskLaneKind::PeopleNew)),
            item(&unknown, None),
        ],
        false,
    )
    .await;
    assert!(outcome[0].error.is_none());
    assert!(outcome[1]
        .error
        .as_deref()
        .unwrap()
        .contains("run Done again"));
    assert_eq!(outcome[2].error.as_deref(), Some("conversation not found"));
    assert!(mutation_id.is_some(), "what did change can be undone");
    assert!(!lanes(&fx).await[0].contains(&thread));
    let pair = [(offline.id.clone(), stuck_thread.clone())];
    assert!(
        fx.state.store.desk_dismissal_priors(&pair).await.unwrap()[0]
            .prior
            .is_none(),
        "not dismissed, so its row stays"
    );
}

fn promise_record(
    id: &str,
    account: &mxr_core::AccountId,
    thread: &ThreadId,
    evidence: &MessageId,
) -> mxr_store::ContactCommitmentRecord {
    mxr_store::ContactCommitmentRecord {
        id: id.into(),
        account_id: account.clone(),
        email: "nora@example.com".into(),
        thread_id: thread.clone(),
        direction: mxr_store::CommitmentDirection::Yours,
        status: CommitmentStatus::Open,
        who_owes: "you".into(),
        what: format!("promise {id}"),
        by_when: Some(chrono::Utc::now() + Duration::days(1)),
        evidence_msg_id: evidence.clone(),
        extracted_at: chrono::Utc::now(),
        resolved_at: None,
    }
}

#[tokio::test]
async fn keeping_one_promise_leaves_the_other_promises_on_that_thread_under_due() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mine = fx
        .message(&thread, ME, "nora@example.com", Duration::days(2), None)
        .await;
    for id in ["first", "second"] {
        fx.state
            .store
            .upsert_contact_commitment(&promise_record(id, &fx.account, &thread, &mine.id))
            .await
            .unwrap();
    }
    let due = DeskDoneItemData {
        thread_id: thread.clone(),
        lane: Some(DeskLaneKind::Due),
        commitment_id: Some("first".into()),
    };
    let (outcome, _) = done(&fx, vec![due], false).await;
    assert!(outcome[0].error.is_none());
    let [owed, due, waiting, people_new] = lanes(&fx).await;
    assert_eq!(due, vec![thread], "the other promise still stands");
    assert!(owed.is_empty() && waiting.is_empty() && people_new.is_empty());
}

#[tokio::test]
async fn a_promise_from_another_account_is_refused() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mine = fx
        .message(&thread, ME, "nora@example.com", Duration::days(2), None)
        .await;
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Other".into(),
        email: "other@example.com".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&other).await.unwrap();
    // Same conversation id, recorded against the other account.
    fx.state
        .store
        .upsert_contact_commitment(&promise_record("theirs", &other.id, &thread, &mine.id))
        .await
        .unwrap();
    let item = DeskDoneItemData {
        thread_id: thread.clone(),
        lane: Some(DeskLaneKind::Due),
        commitment_id: Some("theirs".into()),
    };
    let (outcome, mutation_id) = done(&fx, vec![item], false).await;
    assert_eq!(
        outcome[0].error.as_deref(),
        Some("that promise belongs to another conversation")
    );
    assert!(mutation_id.is_none());
    let record = fx
        .state
        .store
        .get_contact_commitment("theirs")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.status, CommitmentStatus::Open);
    // Nor can a status change reach across accounts.
    assert!(!fx
        .state
        .store
        .resolve_account_commitment(&fx.account, "theirs")
        .await
        .unwrap());
}

#[tokio::test]
async fn the_same_conversation_twice_is_done_once() {
    let fx = Fixture::new().await;
    let (thread, seen, fresh) = owed_thread(&fx).await;
    let (outcome, mutation_id) = done(
        &fx,
        vec![
            item(&thread, Some(DeskLaneKind::Owed)),
            item(&thread, Some(DeskLaneKind::Waiting)),
        ],
        false,
    )
    .await;
    assert!(outcome[0].error.is_none());
    assert_eq!(
        outcome[1].error.as_deref(),
        Some("this conversation is already in the request")
    );
    let entry = fx
        .state
        .store
        .read_undo_entry(&mutation_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    let mut ids: Vec<_> = entry
        .snapshots
        .iter()
        .map(|s| s.message_id.clone())
        .collect();
    ids.sort_by_key(MessageId::as_str);
    let mut want = vec![seen.id.clone(), fresh.id.clone()];
    want.sort_by_key(MessageId::as_str);
    assert_eq!(ids, want, "one snapshot per message");
}

#[tokio::test]
async fn a_half_done_archive_is_undone_to_the_original_state() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mine = fx
        .message(&thread, ME, "maya@example.com", Duration::days(2), None)
        .await;
    let fresh = fx
        .message(
            &thread,
            "maya@example.com",
            ME,
            Duration::hours(3),
            mine.message_id_header.as_deref(),
        )
        .await;
    assert!(!read(&envelope(&fx, &fresh.id).await));

    // Marking read works, taking it out of the inbox fails.
    fx.fake.fail_label_changes(true);
    let (outcome, mutation_id) =
        done(&fx, vec![item(&thread, Some(DeskLaneKind::Owed))], false).await;
    assert!(outcome[0].error.is_some());
    let half = envelope(&fx, &fresh.id).await;
    assert!(read(&half) && in_inbox(&half), "read, still in the inbox");
    assert_eq!(
        lanes(&fx).await[0],
        vec![thread.clone()],
        "still on the desk"
    );

    // What it did change has its own undo, back to the original.
    fx.fake.fail_label_changes(false);
    undo(&fx, mutation_id.expect("the half-done change is undoable")).await;
    let back = envelope(&fx, &fresh.id).await;
    assert!(in_inbox(&back));
    assert!(!read(&back), "unread, as it was");

    // A retry is a new Done with its own snapshot.
    let (outcome, _) = done(&fx, vec![item(&thread, Some(DeskLaneKind::Owed))], false).await;
    assert!(outcome[0].error.is_none());
}

#[tokio::test]
async fn done_takes_the_conversation_out_of_reply_later_and_undo_puts_it_back() {
    let fx = Fixture::new().await;
    let (thread, _, fresh) = owed_thread(&fx).await;
    let flagged_at = chrono::Utc::now() - Duration::days(1);
    fx.state
        .store
        .set_reply_later(&fresh.id, flagged_at)
        .await
        .unwrap();

    let (outcome, mutation_id) = done(&fx, vec![item(&thread, None)], false).await;
    assert_eq!(outcome[0].reply_later_cleared, 1);
    assert!(!fx.state.store.is_reply_later(&fresh.id).await.unwrap());

    undo(&fx, mutation_id.unwrap()).await;
    let restored = fx
        .state
        .store
        .reply_later_set_at(std::slice::from_ref(&fresh.id))
        .await
        .unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(
        restored[0].1.timestamp(),
        flagged_at.timestamp(),
        "same place in the queue"
    );
}

#[tokio::test]
async fn a_failed_second_batch_keeps_the_undo_of_the_first() {
    let fx = Fixture::new().await;
    let (thread, seen, fresh) = owed_thread(&fx).await;
    // The read batch names a message that is gone by the time it runs.
    let vanished = (
        ThreadId::new(),
        fx.account.clone(),
        DeskLaneKind::Waiting,
        vec![MessageId::new()],
    );
    let owed = (
        thread.clone(),
        fx.account.clone(),
        DeskLaneKind::Owed,
        vec![seen.id.clone(), fresh.id.clone()],
    );
    let response =
        crate::handler::desk_done::run_messages_for_test(&fx.state, vec![owed, vanished])
            .await
            .expect("the run reports per item, never loses what changed");
    let ResponseData::DeskItemsResolved {
        items, mutation_id, ..
    } = response
    else {
        panic!("expected DeskItemsResolved");
    };
    assert!(items[0].error.is_none());
    assert!(items[1].error.is_some());
    for id in [&seen.id, &fresh.id] {
        assert!(!in_inbox(&envelope(&fx, id).await));
    }

    undo(&fx, mutation_id.expect("the archive batch is undoable")).await;
    assert!(in_inbox(&envelope(&fx, &seen.id).await));
    let back = envelope(&fx, &fresh.id).await;
    assert!(in_inbox(&back) && !read(&back));
}

#[tokio::test]
async fn undo_of_a_failed_archive_reverses_it_at_the_provider_too() {
    let fx = Fixture::new().await;
    let (_, _, fresh) = owed_thread(&fx).await;
    fx.fake.fail_label_changes(true);
    let result = match request(
        &fx,
        Request::mutation(mxr_protocol::MutationCommand::Archive {
            message_ids: vec![fresh.id.clone()],
        }),
    )
    .await
    {
        ResponseData::MutationResult { result } => result,
        other => panic!("expected MutationResult, got {other:?}"),
    };
    assert_eq!(result.succeeded, 0);
    // The local copy shows no change, but the provider may have archived it.
    assert!(in_inbox(&envelope(&fx, &fresh.id).await));
    fx.fake.fail_label_changes(false);
    let before = fx.fake.mutations().len();

    undo(
        &fx,
        result
            .mutation_id
            .expect("an uncertain failure is undoable"),
    )
    .await;
    let sent = fx.fake.mutations()[before..].to_vec();
    assert!(
        sent.iter().any(|m| matches!(
            m,
            mxr_provider_fake::RecordedMutation::LabelsModified { provider_id, added, .. }
                if provider_id == &fresh.provider_id && added.iter().any(|l| l == "INBOX")
        )),
        "undo put INBOX back at the provider: {sent:?}"
    );
    assert!(in_inbox(&envelope(&fx, &fresh.id).await));
}
