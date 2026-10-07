//! Updates as a twice-daily briefing on a moved clock: cut boundaries and
//! the "since" section, sources folded into lines by signal, deltas only
//! within a template and unit, let go acting on exactly the previewed
//! cut, breakthroughs to To do once, tuning, expiry, quiet parcels and the
//! rail.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::updates::{self, LetGo};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use mxr_core::id::{DeliveryId, MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection};
use mxr_protocol::{
    RailStatusData, UpdateSectionData, UpdateSourceSettingData, UpdatesDigestData, UpdatesLetGoData,
};
use mxr_updates::Cuts;

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

/// A clock two hours after a default cut that is at least three hours in
/// the past, so mail dated around the cut is never in the future.
fn clock() -> (DateTime<Utc>, DateTime<Utc>) {
    let wall = Utc::now();
    let cut = Cuts::default().window(wall - Duration::hours(3), &utc()).at;
    (cut, cut + Duration::hours(2))
}

/// One automated message in the inbox, dated `at`.
async fn update(
    fx: &Fixture,
    thread: &ThreadId,
    from: &str,
    subject: &str,
    at: DateTime<Utc>,
) -> Envelope {
    let mut envelope = fx.message(thread, from, ME, Utc::now() - at, None).await;
    envelope.subject = subject.to_string();
    envelope.date = at;
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    envelope
}

async fn digest(fx: &Fixture, now: DateTime<Utc>) -> UpdatesDigestData {
    updates::digest_at(&fx.state, None, None, false, false, now, &utc())
        .await
        .unwrap()
}

async fn let_go(
    fx: &Fixture,
    now: DateTime<Utc>,
    token: Option<&str>,
    dry_run: bool,
) -> Result<UpdatesLetGoData, String> {
    updates::let_go_at(
        &fx.state,
        &LetGo {
            account_id: None,
            cut: None,
            source_key: None,
            selection_token: token,
            dry_run,
        },
        now,
        &utc(),
    )
    .await
    .map_err(|error| error.to_string())
}

fn all_lines(digest: &UpdatesDigestData) -> Vec<&mxr_protocol::UpdateLineData> {
    digest
        .needs_a_look
        .iter()
        .chain(&digest.changed)
        .chain(&digest.routine)
        .collect()
}

#[tokio::test]
async fn the_cut_splits_the_digest_from_since_and_sources_fold_by_signal() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    for hours in [5, 3, 1] {
        update(
            &fx,
            &ThreadId::new(),
            "notifications@vercel.com",
            "Deployment succeeded for acme-web",
            cut - Duration::hours(hours),
        )
        .await;
    }
    // Vercel's history: seen before, so routine.
    update(
        &fx,
        &ThreadId::new(),
        "notifications@vercel.com",
        "Deployment succeeded for acme-web",
        cut - Duration::days(6),
    )
    .await;
    // Strava, a week apart: the delta is computed in code.
    update(
        &fx,
        &ThreadId::new(),
        "no-reply@strava.com",
        "Your week in running: 19.0 km over 2 runs",
        cut - Duration::hours(24 * 7 + 3),
    )
    .await;
    update(
        &fx,
        &ThreadId::new(),
        "no-reply@strava.com",
        "Your week in running: 21.3 km over 3 runs",
        cut - Duration::hours(3),
    )
    .await;
    let after = update(
        &fx,
        &ThreadId::new(),
        "notifications@linear.app",
        "3 issues moved to Done",
        cut + Duration::hours(1),
    )
    .await;

    let digest = digest(&fx, now).await;
    assert_eq!(digest.cut.at, cut);
    assert_eq!(digest.cut.cuts, vec!["08:00", "16:30"]);
    let vercel = digest
        .routine
        .iter()
        .find(|line| line.source_key == "vercel.com")
        .expect("Vercel folds to one routine line");
    assert_eq!(vercel.count, 4, "every Vercel message up to the cut");
    let strava = all_lines(&digest)
        .into_iter()
        .find(|line| line.source_key == "strava.com")
        .expect("a Strava line");
    assert_eq!(strava.section, UpdateSectionData::Changed);
    assert_eq!(
        strava.delta.as_ref().map(|d| d.text.as_str()),
        Some("up 12% on last week")
    );
    assert!(strava
        .provenance
        .iter()
        .any(|p| p.field == "delta" && p.source == "code"));
    // After the cut: in "since", never in the digest.
    assert!(all_lines(&digest)
        .iter()
        .all(|line| !line.message_ids.contains(&after.id)));
    assert_eq!(digest.since.message_count, 1);
    assert!(digest.since.lines[0].message_ids.contains(&after.id));
    assert!(digest.since.label.starts_with("arriving for "));
    assert!(digest.headline.contains("changed"), "{}", digest.headline);
}

#[tokio::test]
async fn let_go_acts_on_exactly_the_previewed_cut() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    let shared = ThreadId::new();
    let in_cut = update(
        &fx,
        &shared,
        "notifications@github.com",
        "[acme/api] Run succeeded: CI - main (5e6f7a8)",
        cut - Duration::hours(2),
    )
    .await;
    let other = update(
        &fx,
        &ThreadId::new(),
        "notifications@vercel.com",
        "Deployment succeeded",
        cut - Duration::hours(1),
    )
    .await;
    // Same thread, after the cut: it stays.
    let later = update(
        &fx,
        &shared,
        "notifications@github.com",
        "[acme/api] Run failed: CI - main (9f8e7d6)",
        cut + Duration::hours(1),
    )
    .await;

    let preview = let_go(&fx, now, None, true).await.unwrap();
    assert!(preview.dry_run);
    let mut previewed = preview.message_ids.clone();
    previewed.sort_by_key(MessageId::as_str);
    let mut expected = vec![in_cut.id.clone(), other.id.clone()];
    expected.sort_by_key(MessageId::as_str);
    assert_eq!(previewed, expected);
    assert!(
        preview
            .line
            .starts_with("Let go of 2 updates from 2 sources"),
        "{}",
        preview.line
    );
    assert_eq!(
        digest(&fx, now).await.selection_token,
        preview.selection_token
    );

    // Mail that lands in the cut between preview and run: the run refuses.
    let sneaked = update(
        &fx,
        &ThreadId::new(),
        "notifications@vercel.com",
        "Deployment succeeded",
        cut - Duration::minutes(30),
    )
    .await;
    let refused = let_go(&fx, now, Some(&preview.selection_token), false).await;
    assert!(refused.unwrap_err().contains("changed since the preview"));
    let preview = let_go(&fx, now, None, true).await.unwrap();
    assert!(preview.message_ids.contains(&sneaked.id));

    let run = let_go(&fx, now, Some(&preview.selection_token), false)
        .await
        .unwrap();
    assert!(!run.dry_run);
    assert_eq!(run.message_ids, preview.message_ids);
    assert_eq!(run.thread_ids, preview.thread_ids);
    assert!(run.mutation_id.is_some());
    let shared_outcome = run
        .items
        .iter()
        .find(|item| item.thread_id == shared)
        .expect("the shared thread");
    assert!(shared_outcome.marked);
    assert_eq!(
        shared_outcome.archived, 0,
        "Updates still holds the later run"
    );

    let after = digest(&fx, now).await;
    assert!(all_lines(&after).is_empty(), "{after:?}");
    assert!(after.empty_state.is_some());
    assert!(after
        .since
        .lines
        .iter()
        .any(|line| line.message_ids.contains(&later.id)));
    // The other threads left the inbox: no other mode held them.
    let other_outcome = run
        .items
        .iter()
        .find(|item| item.thread_id == other.thread_id)
        .unwrap();
    assert_eq!(other_outcome.archived, 1, "{}", other_outcome.copy);
}

#[tokio::test]
async fn a_new_sign_in_breaks_through_once_and_shows_already_in_to_do() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    let alert = update(
        &fx,
        &ThreadId::new(),
        "no-reply@accounts.google.com",
        "Security alert: New sign-in from Chrome on Windows",
        cut - Duration::hours(1),
    )
    .await;
    // Three days old: past its window, it never breaks through.
    let stale = update(
        &fx,
        &ThreadId::new(),
        "no-reply@accounts.google.com",
        "Security alert: New sign-in from Safari on Mac",
        now - Duration::days(3),
    )
    .await;
    // A second alert of the same kind that day is the same thing to check.
    let repeat = update(
        &fx,
        &ThreadId::new(),
        "no-reply@accounts.google.com",
        "Security alert: New sign-in from Chrome on Windows",
        cut - Duration::minutes(30),
    )
    .await;
    let made = updates::scan(
        &fx.state,
        &[alert.id.clone(), stale.id.clone(), repeat.id.clone()],
        now,
    )
    .await
    .unwrap();
    assert_eq!(made, 1);
    let again = updates::scan(&fx.state, std::slice::from_ref(&alert.id), now)
        .await
        .unwrap();
    assert_eq!(again, 0, "claimed by its dedup key");
    let todo = fx
        .state
        .store
        .open_todos_for_threads(&fx.account, std::slice::from_ref(&alert.thread_id))
        .await
        .unwrap()
        .pop()
        .expect("a to-do");
    assert_eq!(todo.title, "Check new sign-in to Google");
    assert_eq!(todo.origin, "rule");
    assert!(todo.relevant_until.is_some());

    let digest = digest(&fx, now).await;
    let line = digest
        .needs_a_look
        .iter()
        .find(|line| line.message_ids.contains(&alert.id))
        .expect("a needs-a-look line");
    assert_eq!(line.in_todo.as_deref(), Some("already in To do"));
    assert_eq!(line.todo_id.as_deref(), Some(todo.id.as_str()));
    assert!(line.link.is_none(), "a sign-in line never opens a link");
    assert!(all_lines(&digest)
        .iter()
        .all(|line| !line.message_ids.contains(&stale.id)));
    let preview = let_go(&fx, now, None, true).await.unwrap();
    assert!(preview.line.contains("also in To do"), "{}", preview.line);
}

#[tokio::test]
async fn expired_codes_never_show_and_are_listed_as_expired() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    let code = update(
        &fx,
        &ThreadId::new(),
        "verify@acme-login.example",
        "Your verification code is 482913",
        cut - Duration::hours(1),
    )
    .await;
    let digest = digest(&fx, now).await;
    assert!(all_lines(&digest).is_empty());
    let listed = updates::digest_at(&fx.state, None, None, false, true, now, &utc())
        .await
        .unwrap();
    assert_eq!(listed.expired.len(), 1);
    assert_eq!(listed.expired[0].message_id, code.id);
    assert_eq!(listed.expired[0].kind, "one-time code");
    // Nothing breaks through past its window.
    assert_eq!(updates::scan(&fx.state, &[code.id], now).await.unwrap(), 0);
    // Letting go of the cut takes the expired code with it.
    let preview = let_go(&fx, now, None, true).await.unwrap();
    assert_eq!(preview.hidden_count, 1);
}

#[tokio::test]
async fn tuning_hides_muted_sources_and_routine_from_changes_only() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    for (from, subject) in [
        ("notifications@vercel.com", "Deployment succeeded"),
        ("notifications@uptimerobot.com", "Monitor is up"),
    ] {
        for days in [6, 0] {
            update(
                &fx,
                &ThreadId::new(),
                from,
                subject,
                cut - Duration::days(days) - Duration::hours(1),
            )
            .await;
        }
    }
    let muted = request(
        &fx,
        Request::SetUpdateSource {
            account_id: None,
            source: "notifications@vercel.com".into(),
            setting: UpdateSourceSettingData::Muted,
            dry_run: false,
        },
    )
    .await;
    let ResponseData::UpdateSource { change } = muted else {
        panic!("{muted:?}");
    };
    assert_eq!(change.source_key, "vercel.com");
    assert_eq!(change.prior, UpdateSourceSettingData::EveryDigest);
    request(
        &fx,
        Request::SetUpdateSource {
            account_id: None,
            source: "uptimerobot.com".into(),
            setting: UpdateSourceSettingData::ChangesOnly,
            dry_run: false,
        },
    )
    .await;
    let digest = digest(&fx, now).await;
    assert!(all_lines(&digest).is_empty(), "{:?}", all_lines(&digest));
    let hidden = digest.hidden_line.expect("a hidden line");
    assert!(hidden.contains("muted"), "{hidden}");
    assert!(hidden.contains("changes-only"), "{hidden}");
    assert_eq!(digest.muted_total, 1);
}

#[tokio::test]
async fn a_parcel_with_no_news_goes_quiet_and_leaves_the_active_list() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let quiet = mxr_store::Delivery {
        id: DeliveryId::new(),
        account_id: fx.account.clone(),
        dedup_key: "quiet-parcel".into(),
        merchant: Some("Bookshop".into()),
        carrier: Some("DHL".into()),
        tracking_number: None,
        tracking_url: None,
        order_number: None,
        status: "in_transit".into(),
        eta_from: None,
        eta_until: None,
        delivered_at: None,
        items: Vec::new(),
        confidence: 1.0,
        source: "heuristic".into(),
        thread_id: None,
        last_event_at: now - Duration::days(20),
        created_at: now - Duration::days(21),
        updated_at: now - Duration::days(20),
        resolved_at: None,
        dismissed_at: None,
    };
    let moving = mxr_store::Delivery {
        id: DeliveryId::new(),
        dedup_key: "moving-parcel".into(),
        last_event_at: now - Duration::hours(1),
        eta_until: Some(now + Duration::days(1)),
        ..quiet.clone()
    };
    fx.state.store.insert_delivery(&quiet).await.unwrap();
    fx.state.store.insert_delivery(&moving).await.unwrap();
    let active = request(
        &fx,
        Request::ListDeliveries {
            account_id: None,
            filter: Some("active".into()),
        },
    )
    .await;
    let ResponseData::Deliveries { deliveries } = active else {
        panic!("{active:?}");
    };
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].id, moving.id);

    let digest = digest(&fx, now).await;
    let parcels: Vec<_> = all_lines(&digest)
        .into_iter()
        .filter(|line| line.tracker.as_ref().is_some_and(|t| t.kind == "parcel"))
        .collect();
    assert_eq!(parcels.len(), 1, "the quiet parcel went quiet long ago");
    let tracker = parcels[0].tracker.as_ref().unwrap();
    assert_eq!(tracker.steps.len(), 4);
    assert_eq!(tracker.step, Some(1));
}

#[tokio::test]
async fn updates_is_built_on_the_rail_with_no_badge() {
    let fx = Fixture::new().await;
    update(
        &fx,
        &ThreadId::new(),
        "notifications@vercel.com",
        "Deployment succeeded",
        Utc::now() - Duration::hours(1),
    )
    .await;
    let rail = request(&fx, Request::GetRail { account_id: None }).await;
    let ResponseData::Rail { rail } = rail else {
        panic!("{rail:?}");
    };
    let entry = rail.entries.iter().find(|e| e.id == "updates").unwrap();
    assert_eq!(entry.status, RailStatusData::Built);
    assert!(entry.early_note.is_none());
    assert_eq!(entry.badge, None, "badges count work only");
    assert_eq!(entry.count, Some(1));
    assert_eq!(
        entry.header.as_deref(),
        Some("Notifications gathered twice a day. Read the digest, then let go.")
    );
}
