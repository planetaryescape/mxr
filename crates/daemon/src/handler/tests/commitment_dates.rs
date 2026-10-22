//! Which commitment dates mxr acts on. A date counts only when its source is
//! the email's words or the user (`by_when_source`); a model-written date
//! stays on the row but the desk, the promise to-do mirror and Now ignore it.

use super::desk::{Fixture, ME};
use super::*;
use crate::handler::{now, todos};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use mxr_core::id::ThreadId;
use mxr_core::types::MessageDirection;
use mxr_store::{
    ByWhenSource, CommitmentDirection, CommitmentStatus, ContactCommitmentRecord, TodoState,
};
use std::collections::{BTreeSet, HashMap};

const SOURCES: [Option<ByWhenSource>; 4] = [
    Some(ByWhenSource::ModelDate),
    Some(ByWhenSource::Words),
    Some(ByWhenSource::User),
    None,
];
const DIRECTIONS: [CommitmentDirection; 2] =
    [CommitmentDirection::Yours, CommitmentDirection::Theirs];
const STATUSES: [CommitmentStatus; 3] = [
    CommitmentStatus::Open,
    CommitmentStatus::Resolved,
    CommitmentStatus::Expired,
];

/// What every acting surface must agree on for one row.
fn acts(
    source: Option<ByWhenSource>,
    direction: CommitmentDirection,
    status: CommitmentStatus,
) -> bool {
    direction == CommitmentDirection::Yours
        && status == CommitmentStatus::Open
        && source.is_some_and(ByWhenSource::is_actionable)
}

fn case_id(
    source: Option<ByWhenSource>,
    direction: CommitmentDirection,
    status: CommitmentStatus,
) -> String {
    format!(
        "c-{}-{}-{}",
        source.map_or("none", ByWhenSource::as_str),
        direction.as_str(),
        status.as_str()
    )
}

/// One commitment on its own thread, evidenced by a message you sent.
async fn seed(
    fx: &Fixture,
    id: &str,
    by_when: DateTime<Utc>,
    source: Option<ByWhenSource>,
    direction: CommitmentDirection,
    status: CommitmentStatus,
) {
    let thread = ThreadId::new();
    let sent = fx
        .message(&thread, ME, "nora@example.com", Duration::days(3), None)
        .await;
    fx.store_envelope(&sent, MessageDirection::Outbound).await;
    fx.state
        .store
        .upsert_contact_commitment(&ContactCommitmentRecord {
            id: id.into(),
            account_id: fx.account.clone(),
            email: "nora@example.com".into(),
            thread_id: thread,
            direction,
            status,
            who_owes: "you".into(),
            what: format!("promise {id}"),
            by_when: Some(by_when),
            by_when_source: source,
            evidence_msg_id: sent.id.clone(),
            extracted_at: Utc::now(),
            resolved_at: None,
        })
        .await
        .unwrap();
}

async fn run_todo_pass(fx: &Fixture, at: DateTime<Utc>) {
    let mut fingerprints = HashMap::new();
    while todos::tick(&fx.state, at, &mut fingerprints).await.unwrap() {}
}

#[tokio::test]
async fn every_acting_surface_agrees_on_which_commitment_dates_count() {
    let fx = Fixture::new().await;
    let at = Utc::now();
    // Two days overdue: inside the desk's window, so only the source decides.
    let due = at - Duration::days(2);
    let mut cases = Vec::new();
    for source in SOURCES {
        for direction in DIRECTIONS {
            for status in STATUSES {
                let id = case_id(source, direction, status);
                seed(&fx, &id, due, source, direction, status).await;
                cases.push((id, acts(source, direction, status)));
            }
        }
    }
    assert_eq!(cases.len(), 24);

    // The desk's Due lane.
    let ResponseData::Desk { due: lane, .. } = fx.desk(100).await else {
        panic!("expected a desk");
    };
    let on_desk: BTreeSet<String> = lane
        .rows
        .iter()
        .filter_map(|row| row.commitment_id.clone())
        .collect();
    let expected: BTreeSet<String> = cases
        .iter()
        .filter(|(_, acts)| *acts)
        .map(|(id, _)| id.clone())
        .collect();
    assert_eq!(on_desk, expected, "desk due rows");
    assert_eq!(expected.len(), 2, "words and user, yours and open");

    // The promise to-do mirror: the date a to-do carries.
    run_todo_pass(&fx, at).await;
    let mirrored: HashMap<String, Option<DateTime<Utc>>> = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|todo| Some((todo.commitment_id?, todo.due_at)))
        .collect();
    for (id, acts) in &cases {
        match mirrored.get(id) {
            // Only open promises of yours are mirrored at all.
            None => assert!(
                !id.contains("-yours-open"),
                "{id}: an open promise of yours is mirrored"
            ),
            Some(due_at) => assert_eq!(due_at.is_some(), *acts, "{id}: mirrored due date"),
        }
    }
    assert_eq!(
        mirrored.len(),
        SOURCES.len(),
        "one open promise of yours per source is mirrored"
    );
}

#[tokio::test]
async fn now_shows_a_promise_due_soon_only_when_its_date_is_not_a_models() {
    let fx = Fixture::new().await;
    let at = Utc::now();
    // The same date on every source: due in the Now band.
    let soon = at + Duration::hours(2);
    for source in SOURCES {
        seed(
            &fx,
            &case_id(source, CommitmentDirection::Yours, CommitmentStatus::Open),
            soon,
            source,
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        )
        .await;
    }
    run_todo_pass(&fx, at).await;

    let utc = FixedOffset::east_opt(0).unwrap();
    let data = now::now_at(&fx.state, None, at, &utc).await.unwrap();
    // A promise's to-do is titled by what was promised, which `seed` makes
    // from the commitment id.
    let in_now: BTreeSet<String> = data
        .due_soon
        .todos
        .iter()
        .map(|item| item.todo.title.clone())
        .collect();
    let expected: BTreeSet<String> = [ByWhenSource::Words, ByWhenSource::User]
        .into_iter()
        .map(|source| {
            format!(
                "Promise {}",
                case_id(
                    Some(source),
                    CommitmentDirection::Yours,
                    CommitmentStatus::Open,
                )
            )
        })
        .collect();
    assert_eq!(in_now, expected);
}

/// BK's upgrade path: promises mirrored while their dates counted, then the
/// dates turn out to be a model's and a restarted daemon reruns the mirror.
#[tokio::test]
async fn a_mirrored_promise_loses_its_due_date_when_the_date_turns_out_to_be_a_models() {
    let fx = Fixture::new().await;
    let at = Utc::now();
    let soon = at + Duration::hours(2);
    let (open, touched) = ("c-untouched", "c-touched");
    for id in [open, touched] {
        seed(
            &fx,
            id,
            soon,
            Some(ByWhenSource::Words),
            CommitmentDirection::Yours,
            CommitmentStatus::Open,
        )
        .await;
    }
    run_todo_pass(&fx, at).await;
    let promise_todos = || async {
        let mut rows = fx
            .state
            .store
            .list_promise_todos(&fx.account)
            .await
            .unwrap();
        rows.sort_by(|a, b| a.commitment_id.cmp(&b.commitment_id));
        rows
    };
    let before = promise_todos().await;
    assert_eq!(before.len(), 2);
    for todo in &before {
        assert_eq!(
            todo.due_at.map(|due| due.timestamp()),
            Some(soon.timestamp())
        );
        assert_eq!(todo.state, TodoState::Open);
    }

    // The user edited one of them, so the mirror must leave it alone.
    sqlx::query("UPDATE todos SET user_edited = 1 WHERE commitment_id = ?")
        .bind(touched)
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    // The dates were really a model's, as on a database from before R0.
    sqlx::query("UPDATE contact_commitments SET by_when_source = 'model_date'")
        .execute(fx.state.store.writer())
        .await
        .unwrap();

    // A fresh fingerprints map, as after a daemon restart.
    run_todo_pass(&fx, at).await;
    let after = promise_todos().await;

    assert_eq!(after.len(), 2, "no promise to-do is duplicated or dropped");
    let find = |rows: &[mxr_store::TodoRecord], id: &str| {
        rows.iter()
            .find(|todo| todo.commitment_id.as_deref() == Some(id))
            .cloned()
            .unwrap()
    };
    let untouched = find(&after, open);
    assert_eq!(untouched.id, find(&before, open).id, "same to-do, updated");
    assert_eq!(untouched.due_at, None);
    assert_eq!(untouched.state, TodoState::Open);
    let kept = find(&after, touched);
    assert_eq!(
        kept.due_at.map(|due| due.timestamp()),
        Some(soon.timestamp()),
        "a to-do the user edited keeps its date"
    );
    assert_eq!(kept.state, TodoState::Open);
}
