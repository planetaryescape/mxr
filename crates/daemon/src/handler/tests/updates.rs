use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::updates::{self, LetGo};
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection};
use mxr_protocol::{UpdateSectionData, UpdatesDigestData, UpdatesLetGoData};
use mxr_updates::Cuts;

fn utc() -> chrono::Local {
    chrono::Local
}

fn clock() -> (DateTime<Utc>, DateTime<Utc>) {
    let wall = Utc::now();
    let cut = Cuts::default().window(wall - Duration::hours(3), &utc()).at;
    (cut, cut + Duration::hours(2))
}

async fn update(
    fx: &Fixture,
    thread: &ThreadId,
    from: &str,
    subject: &str,
    at: DateTime<Utc>,
) -> Envelope {
    let mut envelope = fx.message(thread, from, ME, Utc::now() - at, None).await;
    fx.state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("Here is your account update.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: at,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    envelope.subject = subject.to_string();
    envelope.date = at;
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    envelope
}

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
    update(
        &fx,
        &ThreadId::new(),
        "notifications@vercel.com",
        "Deployment succeeded for acme-web",
        cut - Duration::days(6),
    )
    .await;
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
    updates::scan(&fx.state, &[alert.id.clone(), routine.id.clone()], now)
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
    updates::scan(&fx.state, std::slice::from_ref(&forged.id), now)
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
