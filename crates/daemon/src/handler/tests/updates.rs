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
        last_event_at: now - Duration::hours(1),
        eta_until: Some(now + Duration::days(1)),
        ..quiet.clone()
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
    let mut expected = vec![moving.id.clone(), late.id.clone()];
    expected.sort_by_key(ToString::to_string);
    assert_eq!(active, expected);

    let digest = digest(&fx, now).await;
    let parcels: Vec<_> = all_lines(&digest)
        .into_iter()
        .filter(|line| line.tracker.as_ref().is_some_and(|t| t.kind == "parcel"))
        .collect();
    assert_eq!(parcels.len(), 2, "the quiet parcel went quiet long ago");
    let tracker = parcels
        .iter()
        .find(|line| line.id.ends_with(&moving.id.to_string()))
        .and_then(|line| line.tracker.as_ref())
        .unwrap();
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
