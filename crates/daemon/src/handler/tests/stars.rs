//! Star and unstar undo: each message's own star goes back as it was, at
//! the provider too, including stars outside the list a conversation's
//! unstar reached (clients name those from the row's `starred_message_ids`).

use super::desk::{request, Fixture, ME};
use super::*;
use chrono::Duration;
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection, MessageFlags};
use mxr_protocol::{MutationCommand, MutationResultData};
use mxr_provider_fake::RecordedMutation;

async fn star(fx: &Fixture, ids: &[&Envelope], starred: bool) -> MutationResultData {
    match request(
        fx,
        Request::mutation(MutationCommand::Star {
            message_ids: ids.iter().map(|e| e.id.clone()).collect(),
            starred,
        }),
    )
    .await
    {
        ResponseData::MutationResult { result } => result,
        other => panic!("expected MutationResult, got {other:?}"),
    }
}

async fn undo(fx: &Fixture, mutation_id: String) {
    let response = request(fx, Request::UndoMutation { mutation_id }).await;
    assert!(matches!(response, ResponseData::Ack), "{response:?}");
}

async fn starred(fx: &Fixture, id: &MessageId) -> bool {
    fx.state
        .store
        .get_envelope(id)
        .await
        .unwrap()
        .unwrap()
        .flags
        .contains(MessageFlags::STARRED)
}

async fn set_starred(fx: &Fixture, envelope: &mut Envelope, direction: MessageDirection) {
    envelope.flags |= MessageFlags::STARRED;
    fx.store_envelope(envelope, direction).await;
}

/// The provider's star calls since `from`, as (provider id, starred).
fn star_calls(fx: &Fixture, from: usize) -> Vec<(String, bool)> {
    fx.fake.mutations()[from..]
        .iter()
        .filter_map(|m| match m {
            RecordedMutation::StarredSet {
                provider_id,
                starred,
            } => Some((provider_id.clone(), *starred)),
            _ => None,
        })
        .collect()
}

/// Maya wrote (in the inbox, starred); you replied (in Sent, starred); she
/// wrote again (in the inbox, not starred).
async fn starred_conversation(fx: &Fixture) -> (Envelope, Envelope, Envelope) {
    let thread = ThreadId::new();
    let mut first = fx
        .message(&thread, "maya@example.com", ME, Duration::days(3), None)
        .await;
    set_starred(fx, &mut first, MessageDirection::Inbound).await;
    let mut reply = fx
        .message(
            &thread,
            ME,
            "maya@example.com",
            Duration::days(2),
            first.message_id_header.as_deref(),
        )
        .await;
    set_starred(fx, &mut reply, MessageDirection::Outbound).await;
    let last = fx
        .message(
            &thread,
            "maya@example.com",
            ME,
            Duration::days(1),
            reply.message_id_header.as_deref(),
        )
        .await;
    (first, reply, last)
}

#[tokio::test]
async fn unstarring_a_conversation_clears_stars_outside_the_list_and_undo_puts_them_back() {
    let fx = Fixture::new().await;
    let (first, reply, last) = starred_conversation(&fx).await;
    let before = fx.fake.mutations().len();

    // The inbox row lists Maya's two messages and names the starred reply in
    // Sent among its starred messages; the unstar covers all three.
    let result = star(&fx, &[&first, &last, &reply], false).await;
    assert_eq!(result.succeeded, 3);
    for id in [&first.id, &reply.id, &last.id] {
        assert!(!starred(&fx, id).await);
    }
    let mut calls = star_calls(&fx, before);
    calls.sort();
    let mut want = vec![
        (first.provider_id.clone(), false),
        (reply.provider_id.clone(), false),
        (last.provider_id.clone(), false),
    ];
    want.sort();
    assert_eq!(calls, want, "every unstar reached the provider");

    let before = fx.fake.mutations().len();
    undo(&fx, result.mutation_id.expect("a star is undoable")).await;
    assert!(starred(&fx, &first.id).await);
    assert!(starred(&fx, &reply.id).await);
    assert!(!starred(&fx, &last.id).await, "it was never starred");
    let mut calls = star_calls(&fx, before);
    calls.sort();
    let mut want = vec![
        (first.provider_id.clone(), true),
        (reply.provider_id.clone(), true),
    ];
    want.sort();
    assert_eq!(calls, want, "undo restars only what had a star");
}

#[tokio::test]
async fn undo_of_a_star_restores_each_message_as_it_was() {
    let fx = Fixture::new().await;
    let (first, reply, last) = starred_conversation(&fx).await;

    // Starring the conversation: two of three were starred already.
    let result = star(&fx, &[&first, &reply, &last], true).await;
    assert!(starred(&fx, &last.id).await);

    let before = fx.fake.mutations().len();
    undo(&fx, result.mutation_id.expect("a star is undoable")).await;
    assert!(
        starred(&fx, &first.id).await,
        "starred before, starred after"
    );
    assert!(
        starred(&fx, &reply.id).await,
        "starred before, starred after"
    );
    assert!(
        !starred(&fx, &last.id).await,
        "the one it starred is unstarred"
    );
    let undo_calls = star_calls(&fx, before);
    assert_eq!(
        undo_calls,
        vec![(last.provider_id.clone(), false)],
        "not a naive inverse: the already-starred ones are left alone"
    );
}

#[tokio::test]
async fn a_star_that_fails_part_way_is_undone_at_the_provider_too() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let mut messages = Vec::new();
    for age in [3, 2, 1] {
        messages.push(
            fx.message(&thread, "maya@example.com", ME, Duration::days(age), None)
                .await,
        );
    }
    let [one, two, three] = [&messages[0], &messages[1], &messages[2]];
    fx.fake.fail_mutations_of(Some(&two.provider_id));

    let result = star(&fx, &[one, two, three], true).await;
    assert_eq!(result.succeeded, 1);
    assert!(starred(&fx, &one.id).await);
    assert!(!starred(&fx, &two.id).await, "its star failed");
    assert!(!starred(&fx, &three.id).await, "never reached");
    let mutation_id = result
        .mutation_id
        .expect("what changed, and what may have, is undoable");

    fx.fake.fail_mutations_of(None);
    let before = fx.fake.mutations().len();
    undo(&fx, mutation_id).await;
    assert!(!starred(&fx, &one.id).await);
    let mut calls = star_calls(&fx, before);
    calls.sort();
    let mut want = vec![
        (one.provider_id.clone(), false),
        // The local copy shows no star, but the provider may have one.
        (two.provider_id.clone(), false),
    ];
    want.sort();
    assert_eq!(calls, want, "the untouched third is left alone");
}

#[tokio::test]
async fn an_unstar_job_undoes_every_chunk_exactly() {
    let fx = Fixture::new().await;
    let (first, reply, last) = starred_conversation(&fx).await;
    let started = request(
        &fx,
        Request::StartMutationJob {
            mutation: MutationCommand::Star {
                message_ids: vec![first.id.clone(), reply.id.clone(), last.id.clone()],
                starred: false,
            },
            client_correlation_id: None,
        },
    )
    .await;
    let ResponseData::JobStarted { job } = started else {
        panic!("expected JobStarted, got {started:?}");
    };

    let job = loop {
        match request(
            &fx,
            Request::GetJob {
                job_id: job.job_id.clone(),
            },
        )
        .await
        {
            ResponseData::Job { job } if job.finished_at.is_some() => break job,
            ResponseData::Job { .. } => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            other => panic!("expected Job, got {other:?}"),
        }
    };
    assert_eq!(job.progress.succeeded, 3);
    assert!(!starred(&fx, &reply.id).await);
    for mutation_id in job.undo_ids {
        undo(&fx, mutation_id).await;
    }
    assert!(starred(&fx, &first.id).await);
    assert!(starred(&fx, &reply.id).await);
    assert!(!starred(&fx, &last.id).await, "it was never starred");
}
