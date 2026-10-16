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

/// Earlier mail from `from`, so the source isn't a first-time sender.
async fn known_source(fx: &Fixture, from: &str) {
    update(
        fx,
        &ThreadId::new(),
        from,
        "Your account summary",
        Utc::now() - Duration::days(20),
    )
    .await;
}

async fn digest(fx: &Fixture, now: DateTime<Utc>) -> UpdatesDigestData {
    updates::digest_at(&fx.state, None, None, false, false, now, &utc())
        .await
        .unwrap()
}

#[tokio::test]
async fn digest_reports_the_persisted_pin_of_the_latest_email_only() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    known_source(&fx, "notifications@example.com").await;
    let thread = ThreadId::new();
    let older = update(
        &fx,
        &thread,
        "notifications@example.com",
        "Monthly report: 10 items",
        cut - Duration::hours(3),
    )
    .await;
    let latest = update(
        &fx,
        &thread,
        "notifications@example.com",
        "Monthly report: 12 items",
        cut - Duration::hours(2),
    )
    .await;
    fx.state
        .store
        .set_message_pins(std::slice::from_ref(&latest.id), true)
        .await
        .unwrap();

    let first = all_lines(&digest(&fx, now).await)
        .into_iter()
        .find(|line| line.latest_message_id.as_ref() == Some(&latest.id))
        .unwrap()
        .clone();
    assert!(first.latest_message_pinned);

    let next = update(
        &fx,
        &thread,
        "notifications@example.com",
        "Monthly report: 13 items",
        cut - Duration::hours(1),
    )
    .await;
    let rolled_digest = digest(&fx, now).await;
    let rolled = all_lines(&rolled_digest)
        .into_iter()
        .find(|line| line.latest_message_id.as_ref() == Some(&next.id))
        .unwrap()
        .clone();
    assert!(!rolled.latest_message_pinned);
    assert_ne!(older.id, latest.id);
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
async fn a_sign_in_alert_leads_needs_a_look_as_a_suggestion_and_t_makes_the_to_do() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    known_source(&fx, "no-reply@accounts.google.com").await;
    let routine = update(
        &fx,
        &ThreadId::new(),
        "notifications@github.com",
        "[acme/api] Run failed: CI - main (a1b2c3d)",
        cut - Duration::minutes(10),
    )
    .await;
    let alert = update(
        &fx,
        &ThreadId::new(),
        "no-reply@accounts.google.com",
        "Security alert: New sign-in from Chrome on Windows",
        cut - Duration::hours(1),
    )
    .await;
    // Arrival reads facts and makes nothing: no to-do without you.
    updates::scan(&fx.state, &[alert.id.clone(), routine.id.clone()])
        .await
        .unwrap();
    let open = fx
        .state
        .store
        .open_todos_for_threads(&fx.account, std::slice::from_ref(&alert.thread_id))
        .await
        .unwrap();
    assert!(open.is_empty(), "never a to-do on its own");

    let digest = digest(&fx, now).await;
    let first = &digest.needs_a_look[0];
    assert!(first.message_ids.contains(&alert.id), "suggestions lead");
    assert_eq!(first.source_name, "Google (accounts.google.com)");
    assert_eq!(
        first.todo_title,
        "Check new sign-in to Google (accounts.google.com)"
    );
    let suggestion = first.todo_suggestion.as_deref().expect("a suggestion");
    assert!(suggestion.contains("new sign-in alert"), "{suggestion}");
    assert!(first.todo_id.is_none());
    assert!(first.link.is_none(), "a sign-in line never opens a link");

    // t: the user's own action makes the to-do.
    let made = request(
        &fx,
        Request::CreateTodo {
            message_id: first.fact_message_id.clone().unwrap(),
            title: first.todo_title.clone(),
            kind: None,
            due: None,
            time_zone: None,
            dry_run: false,
        },
    )
    .await;
    assert!(matches!(made, ResponseData::TodoChange { .. }));
    let after = updates::digest_at(&fx.state, None, None, false, false, now, &utc())
        .await
        .unwrap();
    let line = after
        .needs_a_look
        .iter()
        .find(|line| line.message_ids.contains(&alert.id))
        .unwrap();
    assert_eq!(line.in_todo.as_deref(), Some("already in To do"));
    assert!(line.todo_suggestion.is_none());
    let preview = let_go(&fx, now, None, true).await.unwrap();
    assert!(preview.line.contains("also in To do"), "{}", preview.line);
}

#[tokio::test]
async fn a_borrowed_brand_shows_its_real_host_and_is_still_only_a_suggestion() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    let mut forged = update(
        &fx,
        &ThreadId::new(),
        "security@g00gle-alerts.example",
        "Security alert: New sign-in from Chrome on Windows",
        cut - Duration::hours(1),
    )
    .await;
    forged.from.name = Some("Google".into());
    fx.store_envelope(&forged, MessageDirection::Inbound).await;
    updates::scan(&fx.state, std::slice::from_ref(&forged.id))
        .await
        .unwrap();
    assert!(fx
        .state
        .store
        .open_todos_for_threads(&fx.account, std::slice::from_ref(&forged.thread_id))
        .await
        .unwrap()
        .is_empty());
    let digest = digest(&fx, now).await;
    let line = &digest.needs_a_look[0];
    assert_eq!(line.source_name, "Google (g00gle-alerts.example)");
    assert!(line.todo_title.ends_with("(g00gle-alerts.example)"));
}

#[tokio::test]
async fn letting_go_of_one_source_leaves_another_sources_mail_in_a_shared_thread() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    let shared = ThreadId::new();
    let vercel = update(
        &fx,
        &shared,
        "notifications@vercel.com",
        "Deployment succeeded",
        cut - Duration::hours(2),
    )
    .await;
    let strava = update(
        &fx,
        &shared,
        "no-reply@strava.com",
        "Your week in running: 21.3 km",
        cut - Duration::hours(1),
    )
    .await;
    let source = |token: Option<String>, dry_run: bool| {
        let state = fx.state.clone();
        async move {
            updates::let_go_at(
                &state,
                &LetGo {
                    account_id: None,
                    cut: None,
                    source_key: Some("vercel.com"),
                    selection_token: token.as_deref(),
                    dry_run,
                },
                now,
                &utc(),
            )
            .await
            .map_err(|error| error.to_string())
        }
    };
    let preview = source(None, true).await.unwrap();
    assert_eq!(preview.message_ids, vec![vercel.id.clone()]);
    // A run without the preview's token is refused.
    assert!(source(None, false)
        .await
        .unwrap_err()
        .contains("Preview first"));
    let run = source(Some(preview.selection_token.clone()), false)
        .await
        .unwrap();
    assert_eq!(run.message_ids, vec![vercel.id.clone()]);
    assert_eq!(run.items[0].archived, 0, "Strava's mail holds the thread");
    let after = digest(&fx, now).await;
    assert!(all_lines(&after)
        .iter()
        .any(|line| line.message_ids.contains(&strava.id)));
    assert!(all_lines(&after)
        .iter()
        .all(|line| !line.message_ids.contains(&vercel.id)));
}

#[tokio::test]
async fn a_message_put_back_in_the_inbox_is_never_archived_by_a_later_let_go() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    let thread = ThreadId::new();
    let first = update(
        &fx,
        &thread,
        "notifications@vercel.com",
        "Deployment succeeded",
        cut - Duration::hours(3),
    )
    .await;
    // An earlier let go saw it; then you put it back in the inbox.
    let mark = mxr_store::ModeDoneMark {
        account_id: fx.account.clone(),
        thread_id: thread.clone(),
        mode: "updates".into(),
        through: mxr_store::DeskDismissal::through(
            &fx.state
                .store
                .desk_messages_in_threads(&fx.account, std::slice::from_ref(&thread))
                .await
                .unwrap(),
        )
        .unwrap(),
    };
    fx.state.store.mark_mode_done(&[mark]).await.unwrap();
    let second = update(
        &fx,
        &thread,
        "notifications@vercel.com",
        "Deployment succeeded",
        cut - Duration::hours(1),
    )
    .await;
    let preview = let_go(&fx, now, None, true).await.unwrap();
    assert_eq!(preview.message_ids, vec![second.id.clone()]);
    let run = let_go(&fx, now, Some(&preview.selection_token), false)
        .await
        .unwrap();
    assert_eq!(run.items[0].archived, 0, "{}", run.items[0].copy);
    let restored = fx
        .state
        .store
        .place_message(&first.id)
        .await
        .unwrap()
        .unwrap();
    assert!(restored.in_inbox, "the restored message stays in the inbox");
}

#[tokio::test]
async fn this_needs_me_names_the_message_its_fact_came_from() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    update(
        &fx,
        &ThreadId::new(),
        "notifications@plausible.io",
        "Weekly report: 1,204 visitors",
        cut - Duration::days(7) - Duration::hours(2),
    )
    .await;
    let report = update(
        &fx,
        &ThreadId::new(),
        "notifications@plausible.io",
        "Weekly report: 998 visitors",
        cut - Duration::hours(3),
    )
    .await;
    update(
        &fx,
        &ThreadId::new(),
        "notifications@plausible.io",
        "Your site is getting more traffic",
        cut - Duration::hours(1),
    )
    .await;
    let digest = digest(&fx, now).await;
    let line = all_lines(&digest)
        .into_iter()
        .find(|line| line.source_key == "plausible.io")
        .unwrap();
    assert!(line.fact.contains("998 visitors"), "{}", line.fact);
    assert_eq!(line.fact_message_id.as_ref(), Some(&report.id));
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
        items: vec![
            mxr_store::DeliveryItem {
                name: "Notebook".into(),
                quantity: Some(2),
            },
            mxr_store::DeliveryItem {
                name: "Pen set".into(),
                quantity: None,
            },
        ],
        tracking_number: Some("TRACK-123".into()),
        order_number: Some("ORDER-4".into()),
        last_event_at: now - Duration::hours(1),
        eta_until: Some(now + Duration::days(1)),
        ..quiet.clone()
    };
    let pickup = mxr_store::Delivery {
        id: DeliveryId::new(),
        dedup_key: "pickup-parcel".into(),
        status: "available_for_pickup".into(),
        tracking_number: Some("PICKUP-42".into()),
        last_event_at: now - Duration::hours(2),
        ..moving.clone()
    };
    let failed = mxr_store::Delivery {
        id: DeliveryId::new(),
        dedup_key: "failed-parcel".into(),
        status: "attempt_fail".into(),
        last_event_at: now - Duration::hours(3),
        ..moving.clone()
    };
    let from_only = mxr_store::Delivery {
        id: DeliveryId::new(),
        dedup_key: "from-only-parcel".into(),
        eta_from: Some(now + Duration::days(2)),
        eta_until: None,
        last_event_at: now - Duration::minutes(30),
        ..moving.clone()
    };
    let carrier_only = mxr_store::Delivery {
        id: DeliveryId::new(),
        dedup_key: "carrier-only-parcel".into(),
        merchant: None,
        carrier: Some("DHL".into()),
        ..moving.clone()
    };
    // Its ETA passed ten days ago, but it moved yesterday: not quiet.
    let late = mxr_store::Delivery {
        id: DeliveryId::new(),
        dedup_key: "late-parcel".into(),
        eta_until: Some(now - Duration::days(10)),
        last_event_at: now - Duration::days(1),
        ..quiet.clone()
    };
    fx.state.store.insert_delivery(&quiet).await.unwrap();
    fx.state.store.insert_delivery(&moving).await.unwrap();
    fx.state.store.insert_delivery(&late).await.unwrap();
    fx.state.store.insert_delivery(&pickup).await.unwrap();
    fx.state.store.insert_delivery(&failed).await.unwrap();
    fx.state.store.insert_delivery(&from_only).await.unwrap();
    fx.state.store.insert_delivery(&carrier_only).await.unwrap();
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
    let mut active: Vec<_> = deliveries.iter().map(|d| d.id.clone()).collect();
    active.sort_by_key(ToString::to_string);
    let mut expected = vec![
        moving.id.clone(),
        late.id.clone(),
        pickup.id.clone(),
        failed.id.clone(),
        from_only.id.clone(),
        carrier_only.id.clone(),
    ];
    expected.sort_by_key(ToString::to_string);
    assert_eq!(active, expected);

    let digest = digest(&fx, now).await;
    let parcels: Vec<_> = all_lines(&digest)
        .into_iter()
        .filter(|line| line.tracker.as_ref().is_some_and(|t| t.kind == "parcel"))
        .collect();
    assert_eq!(parcels.len(), 6, "the quiet parcel went quiet long ago");
    let tracker = parcels
        .iter()
        .find(|line| line.id.ends_with(&moving.id.to_string()))
        .and_then(|line| line.tracker.as_ref())
        .unwrap();
    assert_eq!(tracker.steps.len(), 4);
    assert_eq!(tracker.step, Some(1));
    assert!(tracker
        .detail
        .as_deref()
        .unwrap()
        .starts_with("Expected by "));
    assert_eq!(tracker.items[0].name, "Notebook");
    assert_eq!(tracker.items[0].quantity, Some(2));
    assert_eq!(tracker.tracking_number.as_deref(), Some("TRACK-123"));
    assert_eq!(tracker.order_number.as_deref(), Some("ORDER-4"));
    let from_only = parcels
        .iter()
        .find(|line| line.id.ends_with(&from_only.id.to_string()))
        .and_then(|line| line.tracker.as_ref())
        .unwrap();
    assert!(from_only
        .detail
        .as_deref()
        .unwrap()
        .starts_with("Expected from "));
    let carrier_only = parcels
        .iter()
        .find(|line| line.id.ends_with(&carrier_only.id.to_string()))
        .unwrap();
    assert_eq!(carrier_only.source_name, "DHL");
    assert!(carrier_only
        .tracker
        .as_ref()
        .unwrap()
        .detail
        .as_deref()
        .unwrap()
        .starts_with("Expected by "));
    let pickup = parcels
        .iter()
        .find(|line| line.id.ends_with(&pickup.id.to_string()))
        .and_then(|line| line.tracker.as_ref())
        .unwrap();
    assert_eq!(pickup.state_label, "ready to collect");
    assert_eq!(pickup.step, None, "pickup must not appear as shipped");
    let failed = parcels
        .iter()
        .find(|line| line.id.ends_with(&failed.id.to_string()))
        .unwrap();
    assert_eq!(failed.section, UpdateSectionData::NeedsALook);
    assert_eq!(
        failed.tracker.as_ref().unwrap().state_label,
        "delivery attempt failed"
    );
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

#[tokio::test]
async fn undo_puts_a_let_go_digest_back_line_for_line() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    for (from, subject, hours) in [
        (
            "no-reply@strava.com",
            "Your week in running: 21.3 km over 3 runs",
            3,
        ),
        ("notifications@vercel.com", "Deployment succeeded", 2),
        (
            "no-reply@accounts.google.com",
            "Security alert: New sign-in from Chrome on Windows",
            1,
        ),
    ] {
        update(
            &fx,
            &ThreadId::new(),
            from,
            subject,
            cut - Duration::hours(hours),
        )
        .await;
    }
    let before = digest(&fx, now).await;
    let before_lines: Vec<String> = all_lines(&before)
        .iter()
        .map(|l| l.source_key.clone())
        .collect();
    let preview = let_go(&fx, now, None, true).await.unwrap();
    let run = let_go(&fx, now, Some(&preview.selection_token), false)
        .await
        .unwrap();
    assert!(all_lines(&digest(&fx, now).await).is_empty());
    let undone = request(
        &fx,
        Request::UndoMutation {
            mutation_id: run.mutation_id.clone().expect("an undo id"),
        },
    )
    .await;
    let _ = undone;
    let after = digest(&fx, now).await;
    let after_lines: Vec<String> = all_lines(&after)
        .iter()
        .map(|l| l.source_key.clone())
        .collect();
    assert_eq!(after_lines, before_lines);
}

/// One source's let go, as the `e` key sends it.
async fn let_go_source(
    fx: &Fixture,
    now: DateTime<Utc>,
    source_key: &str,
    token: Option<&str>,
    dry_run: bool,
) -> Result<UpdatesLetGoData, String> {
    updates::let_go_at(
        &fx.state,
        &LetGo {
            account_id: None,
            cut: None,
            source_key: Some(source_key),
            selection_token: token,
            dry_run,
        },
        now,
        &utc(),
    )
    .await
    .map_err(|error| error.to_string())
}

fn sorted(mut ids: Vec<MessageId>) -> Vec<MessageId> {
    ids.sort_by_key(MessageId::as_str);
    ids
}

#[tokio::test]
async fn repeat_alerts_from_one_source_fold_to_one_line_and_let_go_still_takes_every_message() {
    let fx = Fixture::new().await;
    let (cut, now) = clock();
    known_source(&fx, "billing@acme.com").await;
    let first = update(
        &fx,
        &ThreadId::new(),
        "billing@acme.com",
        "Payment failed for your Acme subscription",
        cut - Duration::hours(3),
    )
    .await;
    let again = update(
        &fx,
        &ThreadId::new(),
        "billing@acme.com",
        "Payment failed again for your Acme subscription",
        cut - Duration::hours(1),
    )
    .await;
    // The same alert from another source is its own line, and the same
    // source's ordinary mail stays out of the alert line.
    let other = update(
        &fx,
        &ThreadId::new(),
        "billing@globex.com",
        "Payment failed for your Globex subscription",
        cut - Duration::hours(2),
    )
    .await;
    let ordinary = update(
        &fx,
        &ThreadId::new(),
        "billing@acme.com",
        "Your monthly statement is ready",
        cut - Duration::minutes(30),
    )
    .await;

    let digest = digest(&fx, now).await;
    let acme: Vec<_> = digest
        .needs_a_look
        .iter()
        .filter(|line| line.source_key == "acme.com")
        .collect();
    assert_eq!(acme.len(), 1, "one line for one failed payment: {acme:?}");
    let line = acme[0];
    assert_eq!(line.count, 2);
    assert_eq!(
        sorted(line.message_ids.clone()),
        sorted(vec![first.id.clone(), again.id.clone()])
    );
    assert_eq!(line.thread_ids.len(), 2, "both threads, merged");
    assert_eq!(
        line.fact_message_id.as_ref(),
        Some(&again.id),
        "the latest fact leads"
    );
    assert_eq!(line.latest_message_id.as_ref(), Some(&again.id));
    assert_eq!(
        digest
            .needs_a_look
            .iter()
            .filter(|line| line.source_key == "globex.com")
            .count(),
        1
    );
    assert!(
        all_lines(&digest)
            .iter()
            .filter(|line| line.message_ids.contains(&ordinary.id))
            .all(|line| !line.message_ids.contains(&again.id)),
        "ordinary mail is not folded into the alert"
    );

    // The whole digest's dry run is exactly the messages its lines hold, so
    // folding a line never drops a message from the selection.
    let everything = let_go(&fx, now, None, true).await.unwrap();
    let in_lines: Vec<MessageId> = all_lines(
        &updates::digest_at(&fx.state, None, None, false, false, now, &utc())
            .await
            .unwrap(),
    )
    .iter()
    .flat_map(|line| line.message_ids.clone())
    .collect();
    assert_eq!(sorted(everything.message_ids), sorted(in_lines),);

    // Letting go of the source previews both messages, the run lets go of
    // exactly those, and both leave the digest.
    let preview = let_go_source(&fx, now, "acme.com", None, true)
        .await
        .unwrap();
    for id in [&first.id, &again.id, &ordinary.id] {
        assert!(preview.message_ids.contains(id), "{id:?} is previewed");
    }
    assert!(
        !preview.message_ids.contains(&other.id),
        "another source's mail is not"
    );
    assert!(line
        .message_ids
        .iter()
        .all(|id| preview.message_ids.contains(id)));
    let run = let_go_source(&fx, now, "acme.com", Some(&preview.selection_token), false)
        .await
        .unwrap();
    assert_eq!(run.message_ids, preview.message_ids);
    assert_eq!(run.thread_ids, preview.thread_ids);
    let after = updates::digest_at(&fx.state, None, None, false, false, now, &utc())
        .await
        .unwrap();
    assert!(all_lines(&after)
        .iter()
        .all(|line| line.source_key != "acme.com"));
    assert_eq!(after.needs_a_look.len(), 1, "Globex is untouched");
}

/// The demo dataset seeded at `seed` under `cuts`, synced into a fresh
/// daemon whose `updates.cuts` is the same list.
async fn demo_state(seed: DateTime<Utc>, cuts: &[String]) -> Arc<AppState> {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.updates.cuts = cuts.to_vec();
    state.set_config_for_test(config).await;
    let account_id = state.default_account_id();
    let provider = Arc::new(
        mxr_provider_fake::FakeProvider::with_demo_dataset_at(account_id.clone(), 302, seed, cuts)
            .with_page_size(500),
    );
    state.add_sync_provider_for_test(provider.clone());
    loop {
        let outcome = state
            .sync_engine
            .sync_account_with_outcome(provider.as_ref())
            .await
            .unwrap();
        if !outcome.has_more {
            break;
        }
    }
    state
}

/// Strava's weekly summary is the demo's computed delta; it only sits in
/// Changed when the seeded mail is fresh in the digest the daemon builds.
async fn strava_delta(state: &AppState, now: DateTime<Utc>) -> Option<String> {
    let digest = updates::digest_at(state, None, None, false, false, now, &chrono::Local)
        .await
        .unwrap();
    digest
        .changed
        .iter()
        .find(|line| line.source_key == "strava.com")
        .and_then(|line| line.delta.as_ref().map(|d| d.text.clone()))
}

/// The web e2e daemon runs about 25 minutes on one seed. A fixture dated
/// against cuts the digest doesn't use (or a cut that falls mid-run) left
/// the seeded mail outside the digest it was read in.
#[tokio::test]
async fn demo_updates_mail_stays_in_the_digest_whatever_cuts_the_daemon_uses() {
    // Seeded a minute after a cut, a minute before one, and exactly on one.
    let seed = Utc::now();
    let local = |minutes: i64| {
        (seed + Duration::minutes(minutes))
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string()
    };
    let cases: Vec<Vec<String>> = vec![
        vec!["08:00".into(), "16:30".into()],
        vec![local(-1)],
        vec![local(1)],
        vec![local(0)],
        vec![local(-90), local(40), "03:17".into()],
    ];
    for cuts in cases {
        let state = demo_state(seed, &cuts).await;
        assert_eq!(
            strava_delta(&state, seed).await.as_deref(),
            Some("up 12% on last week"),
            "seeded and read at the same moment, cuts {cuts:?}"
        );
    }
}

/// The e2e config pins one daily cut a minute before the seed, so a run
/// of any length under a day never reaches another cut.
#[tokio::test]
async fn a_single_pinned_cut_keeps_the_seeded_digest_for_a_whole_e2e_run() {
    let seed = Utc::now();
    let pinned = vec![(seed - Duration::minutes(1))
        .with_timezone(&chrono::Local)
        .format("%H:%M")
        .to_string()];
    let state = demo_state(seed, &pinned).await;
    for minutes in [0, 25, 90, 23 * 60 + 30] {
        assert_eq!(
            strava_delta(&state, seed + Duration::minutes(minutes))
                .await
                .as_deref(),
            Some("up 12% on last week"),
            "read {minutes} minutes after seeding"
        );
    }
}
