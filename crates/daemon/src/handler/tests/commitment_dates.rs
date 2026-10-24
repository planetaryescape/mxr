//! Which commitment dates mxr acts on. A date counts only when its source is
//! the email's words or the user (`by_when_source`); a model-written date
//! stays on the row but the desk, the promise to-do mirror and Now ignore it.

use super::desk::{Fixture, ME};
use super::*;
use crate::handler::{commitments_extract, now, todos};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use mxr_core::id::ThreadId;
use mxr_core::types::{Address, Draft, DraftContent, DraftIntent, MessageDirection};
use mxr_store::{
    ByWhenSource, CommitmentDirection, CommitmentStatus, DraftCommitmentCandidate, TodoState,
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
) -> String {
    let thread = ThreadId::new();
    let sent = fx
        .message(&thread, ME, "nora@example.com", Duration::days(3), None)
        .await;
    fx.store_envelope(&sent, MessageDirection::Outbound).await;
    let at = Utc::now();
    let draft = Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id: fx.account.clone(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "nora@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Accepted promise".into(),
        content: DraftContent::markdown(format!("promise {id}")),
        inline_assets: vec![],
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: at,
        updated_at: at,
    };
    let candidate = DraftCommitmentCandidate {
        id: mxr_store::new_candidate_id(),
        draft_id: draft.id.clone(),
        account_id: fx.account.clone(),
        email: "nora@example.com".into(),
        direction,
        who_owes: "you".into(),
        what: format!("promise {id}"),
        by_when: Some(by_when),
        by_when_source: source,
        extracted_at: at,
    };
    fx.state
        .store
        .upsert_draft_commitment_candidate(&candidate)
        .await
        .unwrap();
    commitments_extract::promote_after_send(&fx.state, &draft, &sent.id)
        .await
        .unwrap();
    let stored_id = format!("{}::{}", sent.id, candidate.id);
    fx.state
        .store
        .set_contact_commitment_status(&fx.account, &stored_id, status, None)
        .await
        .unwrap();
    stored_id
}

async fn run_todo_pass(fx: &Fixture, at: DateTime<Utc>) {
    super::projection_diagnostics::trace(&fx.state, &fx.account, "before promise tick").await;
    let mut fingerprints = HashMap::new();
    while todos::tick(&fx.state, at, &mut fingerprints).await.unwrap() {}
    super::projection_diagnostics::trace(&fx.state, &fx.account, "after promise tick").await;
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
                let stored_id = seed(&fx, &id, due, source, direction, status).await;
                cases.push((id, stored_id, acts(source, direction, status)));
            }
        }
    }
    assert_eq!(cases.len(), 24);

    super::projection_diagnostics::trace(&fx.state, &fx.account, "24 seeded commitments").await;
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
        .filter(|(_, _, acts)| *acts)
        .map(|(_, stored_id, _)| stored_id.clone())
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
    for (id, stored_id, acts) in &cases {
        match mirrored.get(stored_id) {
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
    let mut accepted_ids = Vec::new();
    for id in ["c-untouched", "c-touched"] {
        accepted_ids.push(
            seed(
                &fx,
                id,
                soon,
                Some(ByWhenSource::Words),
                CommitmentDirection::Yours,
                CommitmentStatus::Open,
            )
            .await,
        );
    }
    let (open, touched) = (accepted_ids[0].as_str(), accepted_ids[1].as_str());
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

/// A draft candidate saved before due words existed can hold a date a model
/// wrote and has no source. Sending its draft must not make that date count.
#[tokio::test]
async fn a_legacy_draft_candidates_date_stays_unverified_after_send() {
    let fx = Fixture::new().await;
    let at = Utc::now();
    let due = at - Duration::days(2);
    let thread = ThreadId::new();
    let sent = fx
        .message(&thread, ME, "nora@example.com", Duration::days(3), None)
        .await;
    fx.store_envelope(&sent, MessageDirection::Outbound).await;
    let draft = Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id: fx.account.clone(),
        from: None,
        reply_headers: None,
        intent: DraftIntent::New,
        to: vec![Address {
            name: None,
            email: "nora@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "deck".into(),
        content: DraftContent::markdown("I'll send both."),
        inline_assets: Vec::new(),
        managed_attachments: None,
        attachments: vec![],
        inline_calendar_reply: None,
        created_at: at,
        updated_at: at,
    };
    // Same date, one with no source (a row from before provenance) and one
    // dated from the draft's words.
    let mut ids = Vec::new();
    for (what, source) in [("legacy", None), ("fresh", Some(ByWhenSource::Words))] {
        let candidate = DraftCommitmentCandidate {
            id: mxr_store::new_candidate_id(),
            draft_id: draft.id.clone(),
            account_id: fx.account.clone(),
            email: "nora@example.com".into(),
            direction: CommitmentDirection::Yours,
            who_owes: ME.into(),
            what: what.into(),
            by_when: Some(due),
            by_when_source: source,
            extracted_at: at,
        };
        fx.state
            .store
            .upsert_draft_commitment_candidate(&candidate)
            .await
            .unwrap();
        ids.push((what, format!("{}::{}", sent.id, candidate.id)));
    }

    commitments_extract::promote_after_send(&fx.state, &draft, &sent.id)
        .await
        .unwrap();

    let id_of = |what: &str| ids.iter().find(|(w, _)| *w == what).unwrap().1.clone();
    let legacy = fx
        .state
        .store
        .get_contact_commitment(&id_of("legacy"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(legacy.by_when_source, None);
    assert!(legacy.date_unverified(), "kept, shown as unverified");

    let ResponseData::Desk { due: lane, .. } = fx.desk(100).await else {
        panic!("expected a desk");
    };
    let on_desk: BTreeSet<String> = lane
        .rows
        .iter()
        .filter_map(|row| row.commitment_id.clone())
        .collect();
    assert_eq!(on_desk, BTreeSet::from([id_of("fresh")]));

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
    assert_eq!(mirrored.get(&id_of("legacy")), Some(&None));
    assert!(mirrored[&id_of("fresh")].is_some());
}
