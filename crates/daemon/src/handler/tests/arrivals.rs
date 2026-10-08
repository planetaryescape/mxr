//! Sorting shows its work (D119): the arrivals line sums and each count
//! opens exactly its emails, the never-bury rule, corrections and their
//! precedence, undo, the Not-sure cap and the track record.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::arrivals::arrivals_at;
use chrono::{Duration, FixedOffset, TimeZone, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageDirection, MessageFlags};
use mxr_protocol::{
    ArrivalBucketData, ArrivalsData, DaemonEvent, KindRuleData, ModeKindData, MoveOutcomeData,
    ThreadModesData,
};

/// One message: `to` and `cc` as given, list headers when `list` is set.
async fn mail(
    fx: &Fixture,
    thread: &ThreadId,
    from: &str,
    to: &[&str],
    cc: &[&str],
    list: bool,
    age: Duration,
) -> Envelope {
    let mut envelope = fx
        .message(thread, from, to.first().copied().unwrap_or(ME), age, None)
        .await;
    let addr = |email: &&str| Address {
        name: None,
        email: (*email).to_string(),
    };
    envelope.to = to.iter().map(addr).collect();
    envelope.cc = cc.iter().map(addr).collect();
    envelope.subject = format!("Note from {from}");
    if list {
        envelope.unsubscribe = UnsubscribeMethod::OneClick {
            url: "https://lists.example/unsub".into(),
        };
    }
    let direction = if from == ME {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    envelope
}

async fn inbound(fx: &Fixture, from: &str, list: bool) -> Envelope {
    mail(
        fx,
        &ThreadId::new(),
        from,
        &[ME],
        &[],
        list,
        Duration::minutes(5),
    )
    .await
}

/// You once wrote to `email`, so the contacts table knows it.
async fn wrote_to(fx: &Fixture, email: &str) {
    mail(
        fx,
        &ThreadId::new(),
        ME,
        &[email],
        &[],
        false,
        Duration::days(20),
    )
    .await;
    fx.state.store.refresh_contacts().await.unwrap();
}

/// The line now, with its window starting an hour ago.
async fn line(fx: &Fixture) -> ArrivalsData {
    let now = Utc::now();
    fx.state
        .store
        .set_mode_viewed("arrivals:since", now - Duration::hours(1))
        .await
        .unwrap();
    arrivals_at(&fx.state, None, false, None, now, &Utc)
        .await
        .unwrap()
}

fn count(line: &ArrivalsData, bucket: ArrivalBucketData) -> u32 {
    line.counts
        .iter()
        .find(|c| c.bucket == bucket)
        .map_or(0, |c| c.count)
}

async fn list(fx: &Fixture, line: &ArrivalsData, bucket: ArrivalBucketData) -> Vec<MessageId> {
    match request(
        fx,
        Request::ListArrivals {
            account_id: None,
            bucket: Some(bucket),
            since: Some(line.since),
            until: Some(line.until),
            limit: 1000,
        },
    )
    .await
    {
        ResponseData::ArrivalList { list } => {
            assert_eq!(list.total as usize, list.items.len());
            list.items.into_iter().map(|item| item.message_id).collect()
        }
        other => panic!("expected an arrival list, got {other:?}"),
    }
}

async fn move_to(
    fx: &Fixture,
    message: &MessageId,
    mode: ModeKindData,
    sender: bool,
) -> MoveOutcomeData {
    move_with(fx, message, mode, sender, None).await
}

async fn move_with(
    fx: &Fixture,
    message: &MessageId,
    mode: ModeKindData,
    sender: bool,
    source: Option<&str>,
) -> MoveOutcomeData {
    match request(
        fx,
        Request::MoveMessage {
            message_id: message.clone(),
            mode,
            sender,
            dry_run: false,
            source: source.map(str::to_string),
        },
    )
    .await
    {
        ResponseData::MessageMoved { outcome } => outcome,
        other => panic!("expected a move, got {other:?}"),
    }
}

async fn undo(fx: &Fixture, correction: i64) -> String {
    match request(
        fx,
        Request::UndoMove {
            correction_id: correction,
        },
    )
    .await
    {
        ResponseData::MoveUndone { copy, .. } => copy,
        other => panic!("expected an undo, got {other:?}"),
    }
}

async fn chip(fx: &Fixture, message: &MessageId) -> (ArrivalBucketData, String) {
    match request(
        fx,
        Request::GetArrivalModes {
            message_ids: vec![message.clone()],
        },
    )
    .await
    {
        ResponseData::ArrivalModes { mut items } => {
            let item = items.remove(0);
            (item.bucket, item.chip)
        }
        other => panic!("expected arrival modes, got {other:?}"),
    }
}

async fn membership(fx: &Fixture, thread: &ThreadId) -> ThreadModesData {
    match request(
        fx,
        Request::GetModeMembership {
            message_id: None,
            thread_id: Some(thread.clone()),
            thread_ids: Vec::new(),
        },
    )
    .await
    {
        ResponseData::ModeMembership { mut threads } => threads.remove(0),
        other => panic!("expected membership, got {other:?}"),
    }
}

fn modes(data: &ThreadModesData) -> Vec<ModeKindData> {
    data.modes.iter().map(|entry| entry.mode).collect()
}

#[tokio::test]
async fn the_line_sums_with_spam_aspects_and_mail_archived_after_it_arrived() {
    let fx = Fixture::new().await;
    let person = inbound(&fx, "maya@orbit.example", false).await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    let mut spam = inbound(&fx, "prize@win.example", false).await;
    spam.flags = MessageFlags::SPAM;
    fx.store_envelope(&spam, MessageDirection::Inbound).await;
    // A to-do from Maya's email: also in To do, never added to the sum.
    request(
        &fx,
        Request::CreateTodo {
            message_id: person.id.clone(),
            title: "Send the deck".into(),
            kind: None,
            due: None,
            time_zone: None,
            dry_run: false,
        },
    )
    .await;

    let first = line(&fx).await;
    // Archived after it arrived: still counted where it went.
    fx.state
        .store
        .set_message_labels(&newsletter.id, &[], mxr_core::types::EventSource::User)
        .await
        .unwrap();
    let after = line(&fx).await;
    for line in [&first, &after] {
        assert_eq!(line.total, 4);
        assert_eq!(line.counts.iter().map(|c| c.count).sum::<u32>(), line.total);
        assert_eq!(count(line, ArrivalBucketData::Messages), 1);
        assert_eq!(count(line, ArrivalBucketData::Reading), 1);
        assert_eq!(count(line, ArrivalBucketData::Updates), 1);
        assert_eq!(count(line, ArrivalBucketData::Spam), 1);
        assert_eq!(count(line, ArrivalBucketData::Sorting), 0);
        assert_eq!(line.also.len(), 1, "{:?}", line.also);
        assert_eq!(line.also[0].bucket, ArrivalBucketData::Todo);
        assert!(
            line.counts.iter().all(|c| c.count > 0),
            "zero counts are left out"
        );
        for c in &line.counts {
            assert_eq!(list(&fx, line, c.bucket).await.len() as u32, c.count);
        }
        assert_eq!(
            list(&fx, line, ArrivalBucketData::Todo).await,
            vec![person.id.clone()]
        );
    }
    assert!(after
        .line
        .starts_with(&format!("Since {}: 4 arrived. ", after.since_label)));
    assert!(after
        .line
        .contains("1 Messages · 1 Updates · 1 Reading · 1 spam."));
    assert!(after.line.ends_with("Also 1 in To do."));
    assert_eq!(
        after.clear_line.as_deref(),
        Some(
            format!(
                "Clear. All 4 emails since {} are accounted for.",
                after.since_label
            )
            .as_str()
        )
    );
    assert_eq!(
        list(&fx, &after, ArrivalBucketData::Reading).await,
        vec![newsletter.id]
    );
    assert_eq!(
        list(&fx, &after, ArrivalBucketData::Updates).await,
        vec![receipt.id]
    );
}

#[tokio::test]
async fn nothing_new_says_when_mail_last_came() {
    let fx = Fixture::new().await;
    inbound(&fx, "maya@orbit.example", false).await;
    let now = Utc::now();
    // The window opens after the mail arrived.
    fx.state
        .store
        .set_mode_viewed("arrivals:since", now + Duration::seconds(2))
        .await
        .unwrap();
    let line = arrivals_at(
        &fx.state,
        None,
        false,
        None,
        now + Duration::seconds(5),
        &Utc,
    )
    .await
    .unwrap();
    assert_eq!(line.total, 0);
    assert!(line.line.starts_with("Nothing new since "), "{}", line.line);
    assert!(line.line.contains("Latest mail"), "{}", line.line);
    assert!(line.clear_line.is_none());
}

#[tokio::test]
async fn the_window_runs_from_the_visit_before_and_a_quick_return_keeps_it() {
    let fx = Fixture::new().await;
    let tz = Utc;
    let morning = Utc.with_ymd_and_hms(2026, 10, 7, 8, 12, 0).unwrap();
    // Never opened: the start of today.
    let first = arrivals_at(&fx.state, None, true, None, morning, &tz)
        .await
        .unwrap();
    assert_eq!(first.since_label, "00:00");
    // The next visit counts from 08:12.
    let noon = morning + Duration::hours(4);
    let second = arrivals_at(&fx.state, None, true, None, noon, &tz)
        .await
        .unwrap();
    assert_eq!(second.since_label, "08:12");
    // Back within a minute (a double mount, a quick look elsewhere): the
    // same visit.
    let again = arrivals_at(
        &fx.state,
        None,
        true,
        None,
        noon + Duration::seconds(20),
        &tz,
    )
    .await
    .unwrap();
    assert_eq!(again.since_label, "08:12");
    // Never more than 24 hours back.
    let days_later = noon + Duration::days(3);
    let late = arrivals_at(&fx.state, None, true, None, days_later, &tz)
        .await
        .unwrap();
    assert_eq!(late.since, days_later - Duration::hours(24));
    // Reading without marking (the CLI, MCP) starts no visit, and shows the
    // window an open Now would show then: within the visit's minute the
    // one it opened with, once it has lapsed the visit itself, so the
    // line never reads smaller after the click than before it.
    let within = arrivals_at(
        &fx.state,
        None,
        false,
        None,
        days_later + Duration::seconds(30),
        &tz,
    )
    .await
    .unwrap();
    // Still the visit's own window, clamped to 24 hours before now.
    assert_eq!(
        within.since,
        days_later + Duration::seconds(30) - Duration::hours(24)
    );
    let lapsed_at = days_later + Duration::hours(1);
    let peek = arrivals_at(&fx.state, None, false, None, lapsed_at, &tz)
        .await
        .unwrap();
    assert_eq!(peek.since, days_later);
    let opened = arrivals_at(&fx.state, None, true, None, lapsed_at, &tz)
        .await
        .unwrap();
    assert_eq!(opened.since, peek.since);
}

#[tokio::test]
async fn list_mail_addressed_to_you_from_someone_you_wrote_to_reaches_messages() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    let known = inbound(&fx, "maya@orbit.example", true).await;
    let stranger = inbound(&fx, "editor@weekly.example", true).await;
    // Only copied: not covered, so Now asks about it.
    let copied_thread = ThreadId::new();
    let copied = mail(
        &fx,
        &copied_thread,
        "maya@orbit.example",
        &["ruth@keystone.example"],
        &[ME],
        false,
        Duration::minutes(3),
    )
    .await;

    let line = line(&fx).await;
    assert_eq!(
        list(&fx, &line, ArrivalBucketData::Messages).await,
        vec![known.id.clone()]
    );
    let reading = list(&fx, &line, ArrivalBucketData::Reading).await;
    assert_eq!(reading, vec![stranger.id.clone()]);
    assert_eq!(
        list(&fx, &line, ArrivalBucketData::Updates).await,
        vec![copied.id.clone()]
    );
    let (_, known_chip) = chip(&fx, &known.id).await;
    assert_eq!(
        known_chip,
        "→ Messages · addressed to you by someone you've written to"
    );

    // Live membership agrees: the known sender's list mail is in Messages.
    let held = membership(&fx, &known.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Messages), "{held:?}");
    assert!(!modes(&held).contains(&ModeKindData::Reading), "{held:?}");

    assert_eq!(line.not_sure.len(), 1);
    assert_eq!(line.not_sure[0].message_id, copied.id);
    assert_eq!(line.not_sure[0].mode, ModeKindData::Updates);
    assert!(line.not_sure[0].line.contains("copied you on"));
    assert_eq!(
        line.not_sure_line.as_deref(),
        Some("1 email I wasn't sure about. Where should it go?")
    );
    assert!(line.not_sure_hint.is_some());

    // Placements record the rule.
    let rule: String = sqlx::query_scalar("SELECT rule FROM arrivals WHERE message_id = ?1")
        .bind(known.id.as_str())
        .fetch_one(fx.state.store.reader())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(KindRuleData::WrittenTo).unwrap(),
        serde_json::Value::String(rule)
    );
}

#[tokio::test]
async fn a_move_takes_effect_everywhere_at_once_and_undo_puts_it_back() {
    let fx = Fixture::new().await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    let mut events = fx.state.event_tx.subscribe();
    let before = line(&fx).await;
    assert_eq!(count(&before, ArrivalBucketData::Reading), 1);

    let moved = move_to(&fx, &newsletter.id, ModeKindData::Messages, false).await;
    assert_eq!(moved.from, ArrivalBucketData::Reading);
    assert_eq!(moved.copy, "Moved to Messages.");
    assert_eq!(
        moved.ask_sender.as_deref(),
        Some("Always for this sender? (K)")
    );
    assert!(moved.hint.is_some());
    let correction = moved.correction_id.expect("a correction to undo");
    let mut changed = false;
    while let Ok(message) = events.try_recv() {
        changed |= matches!(
            message.payload,
            IpcPayload::Event(DaemonEvent::ModesChanged { .. })
        );
    }
    assert!(changed, "every client is told");

    let after = line(&fx).await;
    assert_eq!(count(&after, ArrivalBucketData::Messages), 1);
    assert_eq!(count(&after, ArrivalBucketData::Reading), 0);
    assert_eq!(after.total, before.total);
    let held = membership(&fx, &newsletter.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Messages), "{held:?}");
    assert!(!modes(&held).contains(&ModeKindData::Reading), "{held:?}");
    assert_eq!(
        chip(&fx, &newsletter.id).await,
        (
            ArrivalBucketData::Messages,
            "→ Messages · you moved this email".to_string()
        )
    );
    assert!(after
        .track_record
        .as_deref()
        .unwrap()
        .ends_with("you moved 1."));

    assert_eq!(undo(&fx, correction).await, "Moved back to Reading.");
    assert_eq!(undo(&fx, correction).await, "Already undone.");
    let back = line(&fx).await;
    assert_eq!(count(&back, ArrivalBucketData::Reading), 1);
    assert!(back.track_record.is_none(), "an undone move is no move");
    let held = membership(&fx, &newsletter.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Reading), "{held:?}");
}

#[tokio::test]
async fn a_sender_move_beats_earlier_email_moves_and_a_later_email_move_beats_it() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    let b = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;

    // X on A only.
    move_to(&fx, &a.id, ModeKindData::Messages, false).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);

    // K: the sender's mode wins over A's earlier move, in the same second.
    let sender = move_to(&fx, &b.id, ModeKindData::Updates, true).await;
    assert_eq!(
        sender.copy,
        "All mail from editor@weekly.example goes to Updates."
    );
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Updates);

    // X after K: this email only.
    let later = move_to(&fx, &b.id, ModeKindData::Reading, false).await;
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);

    // New mail from the sender follows K.
    let c = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    assert_eq!(chip(&fx, &c.id).await.0, ArrivalBucketData::Updates);

    // Undo K: A's move stands again; B's later move still stands.
    undo(&fx, sender.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);
    assert_eq!(chip(&fx, &c.id).await.0, ArrivalBucketData::Reading);
    undo(&fx, later.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &b.id).await.0, ArrivalBucketData::Reading);
}

#[tokio::test]
async fn a_sender_mode_set_elsewhere_is_logged_and_its_quick_reversal_is_an_undo() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let set = |kind| Request::SetSenderKind {
        account_id: fx.account.clone(),
        sender_email: "editor@weekly.example".into(),
        kind,
    };
    request(&fx, set(Some(mxr_protocol::SenderKindData::People))).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    let corrections = fx
        .state
        .store
        .list_corrections(std::slice::from_ref(&fx.account), 10)
        .await
        .unwrap();
    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].fields.from_mode, "auto");
    assert_eq!(corrections[0].fields.to_mode, "messages");
    // The client's undo puts the old kind back: the move is undone, not a
    // second move.
    request(&fx, set(None)).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Reading);
    assert!(!fx
        .state
        .store
        .has_moves(std::slice::from_ref(&fx.account))
        .await
        .unwrap());
}

#[tokio::test]
async fn to_do_and_archive_add_the_email_there_and_undo_takes_it_back() {
    let fx = Fixture::new().await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    line(&fx).await;
    let todo = move_to(&fx, &receipt.id, ModeKindData::Todo, false).await;
    assert_eq!(todo.copy, "Added to To do.");
    assert!(todo.ask_sender.is_none());
    let todo_id = todo.aspect_id.clone().expect("the to-do made");
    let after = line(&fx).await;
    assert_eq!(
        count(&after, ArrivalBucketData::Updates),
        1,
        "To do is an aspect"
    );
    assert_eq!(after.also[0].bucket, ArrivalBucketData::Todo);
    undo(&fx, todo.correction_id.unwrap()).await;
    let row = fx.state.store.get_todo(&todo_id).await.unwrap().unwrap();
    assert_eq!(row.state, mxr_store::TodoState::Dismissed);
    assert!(line(&fx).await.also.is_empty());
}

#[tokio::test]
async fn a_dry_run_changes_nothing_and_a_sender_move_needs_a_kind_mode() {
    let fx = Fixture::new().await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let preview = match request(
        &fx,
        Request::MoveMessage {
            message_id: newsletter.id.clone(),
            mode: ModeKindData::Updates,
            sender: false,
            dry_run: true,
            source: None,
        },
    )
    .await
    {
        ResponseData::MessageMoved { outcome } => outcome,
        other => panic!("{other:?}"),
    };
    assert_eq!(preview.copy, "Would move to Updates.");
    assert!(preview.correction_id.is_none());
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Reading
    );

    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(Request::MoveMessage {
            message_id: newsletter.id.clone(),
            mode: ModeKindData::Todo,
            sender: true,
            dry_run: false,
            source: None,
        }),
    };
    assert!(matches!(
        handle_request(&fx.state, &msg).await.payload,
        IpcPayload::Response(Response::Error { .. })
    ));
    // Moving twice: the second move stands, and undoing it returns to the
    // first, not to where it arrived.
    let first = move_to(&fx, &newsletter.id, ModeKindData::Updates, false).await;
    let second = move_to(&fx, &newsletter.id, ModeKindData::Messages, false).await;
    assert_eq!(second.from, ArrivalBucketData::Updates);
    undo(&fx, second.correction_id.unwrap()).await;
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Updates
    );
    undo(&fx, first.correction_id.unwrap()).await;
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Reading
    );
}

#[tokio::test]
async fn not_sure_asks_at_most_three_a_day_by_the_local_day() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    let mut ids = Vec::new();
    for _ in 0..5 {
        let copied = mail(
            &fx,
            &ThreadId::new(),
            "maya@orbit.example",
            &["ruth@keystone.example"],
            &[ME],
            false,
            Duration::minutes(3),
        )
        .await;
        ids.push(copied.id);
    }
    // In UTC+10, the first email arrived at 23:30 yesterday and the rest
    // after midnight today.
    let tz = FixedOffset::east_opt(10 * 3600).unwrap();
    let midnight = tz
        .with_ymd_and_hms(2026, 10, 8, 0, 0, 0)
        .unwrap()
        .with_timezone(&Utc);
    for (i, id) in ids.iter().enumerate() {
        let at = if i == 0 {
            midnight - Duration::minutes(30)
        } else {
            midnight + Duration::minutes(10 * i as i64)
        };
        sqlx::query("UPDATE arrivals SET first_seen_at = ?2 WHERE message_id = ?1")
            .bind(id.as_str())
            .bind(at.timestamp())
            .execute(fx.state.store.writer())
            .await
            .unwrap();
    }
    let now = midnight + Duration::hours(2);
    // Place them first.
    arrivals_at(&fx.state, None, false, None, now, &tz)
        .await
        .unwrap();
    let asked = arrivals_at(&fx.state, None, false, None, now, &tz)
        .await
        .unwrap();
    let shown: Vec<MessageId> = asked
        .not_sure
        .iter()
        .map(|q| q.message_id.clone())
        .collect();
    assert_eq!(
        shown,
        ids[1..4].to_vec(),
        "today's first three, in arrival order"
    );

    // Answering one never pulls the fourth in.
    let kept = move_with(&fx, &ids[1], ModeKindData::Updates, false, Some("not_sure")).await;
    assert_eq!(kept.copy, "Kept in Updates.");
    assert!(kept.correction_id.is_some(), "keeping it is an answer");
    let after = arrivals_at(&fx.state, None, false, None, now, &tz)
        .await
        .unwrap();
    let shown: Vec<MessageId> = after
        .not_sure
        .iter()
        .map(|q| q.message_id.clone())
        .collect();
    assert_eq!(shown, ids[2..4].to_vec());
    assert_eq!(
        after.not_sure_line.as_deref(),
        Some("2 emails I wasn't sure about. Where should these go?")
    );
    // Keeping an email where it was is not a move for the track record.
    assert!(after.track_record.is_none());
    // A day later UTC is still the 8th, but it's the 9th in UTC+10.
    let next_day = arrivals_at(&fx.state, None, false, None, now + Duration::days(1), &tz)
        .await
        .unwrap();
    assert!(next_day.not_sure.is_empty());
}

#[tokio::test]
async fn placement_does_not_depend_on_the_order_mail_was_stored() {
    let first = Fixture::new().await;
    let second = Fixture::new().await;
    let senders = [
        ("maya@orbit.example", false),
        ("editor@weekly.example", true),
        ("no-reply@shop.example", false),
    ];
    for (from, list) in senders {
        inbound(&first, from, list).await;
    }
    for (from, list) in senders.iter().rev() {
        inbound(&second, from, *list).await;
    }
    let a = line(&first).await;
    let b = line(&second).await;
    assert_eq!(a.counts, b.counts);
    assert_eq!(a.total, 3);
}

#[tokio::test]
async fn a_message_stored_mid_correction_is_placed_by_the_rules_and_the_move_stands() {
    let fx = Fixture::new().await;
    let thread = ThreadId::new();
    let first = mail(
        &fx,
        &thread,
        "editor@weekly.example",
        &[ME],
        &[],
        true,
        Duration::minutes(10),
    )
    .await;
    line(&fx).await;
    move_to(&fx, &first.id, ModeKindData::Messages, false).await;
    // Sync stores the next issue in the same thread.
    let next = mail(
        &fx,
        &thread,
        "editor@weekly.example",
        &[ME],
        &[],
        true,
        Duration::minutes(1),
    )
    .await;
    let placed = line(&fx).await;
    assert_eq!(chip(&fx, &first.id).await.0, ArrivalBucketData::Messages);
    assert_eq!(chip(&fx, &next.id).await.0, ArrivalBucketData::Reading);
    assert_eq!(placed.total, 2);
}

#[tokio::test]
async fn each_account_counts_its_own_mail() {
    let fx = Fixture::new().await;
    inbound(&fx, "maya@orbit.example", false).await;
    let other = mxr_core::Account {
        id: mxr_core::AccountId::new(),
        name: "Work".into(),
        email: "me@work.example".into(),
        sync_backend: None,
        send_backend: None,
        enabled: true,
    };
    fx.state.store.insert_account(&other).await.unwrap();
    let mut envelope = fx
        .message(
            &ThreadId::new(),
            "lead@work.example",
            ME,
            Duration::minutes(2),
            None,
        )
        .await;
    envelope.account_id = other.id.clone();
    envelope.id = MessageId::new();
    envelope.provider_id = "work-1".into();
    fx.state
        .store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    let all = line(&fx).await;
    assert_eq!(all.total, 3, "two in the default account, one at work");
    let now = Utc::now();
    fx.state
        .store
        .set_mode_viewed(
            &format!("arrivals:{}:since", other.id),
            now - Duration::hours(1),
        )
        .await
        .unwrap();
    let work = arrivals_at(&fx.state, Some(&other.id), false, None, now, &Utc)
        .await
        .unwrap();
    assert_eq!(work.total, 1);
}

/// The error text of a request the daemon refuses.
async fn refused(fx: &Fixture, request: Request) -> String {
    let msg = IpcMessage {
        id: 3,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    };
    match handle_request(&fx.state, &msg).await.payload {
        IpcPayload::Response(Response::Error { message, .. }) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_open_now_keeps_its_window_while_it_polls() {
    let fx = Fixture::new().await;
    let tz = Utc;
    let morning = Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap();
    arrivals_at(&fx.state, None, true, None, morning, &tz)
        .await
        .unwrap();
    let noon = morning + Duration::hours(4);
    let opened = arrivals_at(&fx.state, None, true, None, noon, &tz)
        .await
        .unwrap();
    assert_eq!(opened.since, morning);
    // A minute-plus later the open page refetches without marking and
    // names the window it holds: it must not jump to the visit itself.
    let later = noon + Duration::minutes(5);
    let polled = arrivals_at(&fx.state, None, false, Some(opened.since), later, &tz)
        .await
        .unwrap();
    assert_eq!(polled.since, opened.since);
    // A peek that holds no window (the CLI) still answers what Now would
    // open with.
    let peek = arrivals_at(&fx.state, None, false, None, later, &tz)
        .await
        .unwrap();
    assert_eq!(peek.since, noon);
}

#[tokio::test]
async fn undoing_an_older_move_is_refused_while_a_newer_one_stands() {
    let fx = Fixture::new().await;
    let newsletter = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let first = move_to(&fx, &newsletter.id, ModeKindData::Updates, false).await;
    let second = move_to(&fx, &newsletter.id, ModeKindData::Messages, false).await;
    let message = refused(
        &fx,
        Request::UndoMove {
            correction_id: first.correction_id.unwrap(),
        },
    )
    .await;
    assert!(message.contains("newer move"), "{message}");
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Messages,
        "the newer move is untouched"
    );
    // Newest first works, and then the older one is free to undo.
    undo(&fx, second.correction_id.unwrap()).await;
    undo(&fx, first.correction_id.unwrap()).await;
    assert_eq!(
        chip(&fx, &newsletter.id).await.0,
        ArrivalBucketData::Reading
    );
}

#[tokio::test]
async fn undoing_a_sender_move_leaves_earlier_email_moves_standing() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    // The sender was sent to Reading some time ago; then A was moved to
    // Messages by hand.
    let long_ago = Utc::now() - Duration::hours(3);
    request(
        &fx,
        Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: "editor@weekly.example".into(),
            kind: Some(mxr_protocol::SenderKindData::Reading),
        },
    )
    .await;
    fx.state
        .store
        .set_screener_decided_at(&fx.account, "editor@weekly.example", long_ago)
        .await
        .unwrap();
    move_to(&fx, &a.id, ModeKindData::Messages, false).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);

    // K to Updates overrides A's move; undoing K restores Reading for the
    // sender with its old date, so A's move stands again.
    let sender = move_to(&fx, &a.id, ModeKindData::Updates, true).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    undo(&fx, sender.correction_id.unwrap()).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
    let decision = fx
        .state
        .store
        .get_screener_decision(&fx.account, "editor@weekly.example")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(decision.decided_at.timestamp(), long_ago.timestamp());
}

#[tokio::test]
async fn reversing_a_sender_mode_set_elsewhere_keeps_earlier_email_moves() {
    let fx = Fixture::new().await;
    let a = inbound(&fx, "editor@weekly.example", true).await;
    line(&fx).await;
    let set = |kind| Request::SetSenderKind {
        account_id: fx.account.clone(),
        sender_email: "editor@weekly.example".into(),
        kind,
    };
    request(&fx, set(Some(mxr_protocol::SenderKindData::Reading))).await;
    let long_ago = Utc::now() - Duration::hours(3);
    fx.state
        .store
        .set_screener_decided_at(&fx.account, "editor@weekly.example", long_ago)
        .await
        .unwrap();
    move_to(&fx, &a.id, ModeKindData::Messages, false).await;
    request(&fx, set(Some(mxr_protocol::SenderKindData::PaperTrail))).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Updates);
    // The quick reversal is an undo: Reading is back, A's move stands.
    request(&fx, set(Some(mxr_protocol::SenderKindData::Reading))).await;
    assert_eq!(chip(&fx, &a.id).await.0, ArrivalBucketData::Messages);
}

#[tokio::test]
async fn moving_to_archive_when_already_filed_records_no_undo_and_keeps_the_record() {
    let fx = Fixture::new().await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    line(&fx).await;
    let first = move_to(&fx, &receipt.id, ModeKindData::Archive, false).await;
    let record = first.aspect_id.clone().expect("the record made");
    let again = move_to(&fx, &receipt.id, ModeKindData::Archive, false).await;
    assert_eq!(again.copy, "Already in Archive.");
    assert!(again.correction_id.is_none() && again.aspect_id.is_none());
    // Nothing the repeat logged can dismiss the record; undoing the first
    // filing still does.
    let still = fx
        .state
        .store
        .get_archive_record(&record)
        .await
        .unwrap()
        .unwrap();
    assert!(still.dismissed_at.is_none());
    let corrections = fx
        .state
        .store
        .list_corrections(std::slice::from_ref(&fx.account), 10)
        .await
        .unwrap();
    assert_eq!(corrections.len(), 1);
}

#[tokio::test]
async fn moving_to_to_do_twice_makes_one_task() {
    let fx = Fixture::new().await;
    let receipt = inbound(&fx, "no-reply@shop.example", false).await;
    line(&fx).await;
    let first = move_to(&fx, &receipt.id, ModeKindData::Todo, false).await;
    let todo = first.aspect_id.clone().expect("the to-do made");
    let again = move_to(&fx, &receipt.id, ModeKindData::Todo, false).await;
    assert_eq!(again.copy, "Already in To do.");
    assert!(again.correction_id.is_none() && again.aspect_id.is_none());
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM todos WHERE source_message_id = ?1 AND state = 'open'",
    )
    .bind(receipt.id.as_str())
    .fetch_one(fx.state.store.reader())
    .await
    .unwrap();
    assert_eq!(open, 1);
    // Undo of the one move still dismisses that task.
    undo(&fx, first.correction_id.unwrap()).await;
    let row = fx.state.store.get_todo(&todo).await.unwrap().unwrap();
    assert_eq!(row.state, mxr_store::TodoState::Dismissed);
}

#[tokio::test]
async fn someone_you_wrote_to_is_never_buried_by_copy_newsletter_address_or_crowd() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    wrote_to(&fx, "newsletter@orbit.example").await;
    // Newsletter-address rule, addressed to you.
    let by_address = inbound(&fx, "newsletter@orbit.example", true).await;
    // Crowd: more than ten recipients, addressed to you.
    let crowd: Vec<String> = (0..12).map(|i| format!("p{i}@team.example")).collect();
    let mut to: Vec<&str> = crowd.iter().map(String::as_str).collect();
    to.push(ME);
    let crowded = mail(
        &fx,
        &ThreadId::new(),
        "maya@orbit.example",
        &to,
        &[],
        true,
        Duration::minutes(4),
    )
    .await;
    // List mail with you only in Cc: not buried in Reading; Now asks.
    let cc_only = mail(
        &fx,
        &ThreadId::new(),
        "maya@orbit.example",
        &["ruth@keystone.example"],
        &[ME],
        true,
        Duration::minutes(3),
    )
    .await;
    let line = line(&fx).await;
    let messages = list(&fx, &line, ArrivalBucketData::Messages).await;
    assert!(messages.contains(&by_address.id), "newsletter address");
    assert!(messages.contains(&crowded.id), "crowd");
    assert!(
        list(&fx, &line, ArrivalBucketData::Reading)
            .await
            .is_empty(),
        "nothing from someone you wrote to lands in Reading"
    );
    assert_eq!(
        list(&fx, &line, ArrivalBucketData::Updates).await,
        vec![cc_only.id.clone()]
    );
    assert_eq!(line.not_sure.len(), 1);
    assert_eq!(line.not_sure[0].message_id, cc_only.id);
}

#[tokio::test]
async fn always_messages_for_a_sender_keeps_a_copied_email_in_messages() {
    let fx = Fixture::new().await;
    wrote_to(&fx, "maya@orbit.example").await;
    let copied = mail(
        &fx,
        &ThreadId::new(),
        "maya@orbit.example",
        &["ruth@keystone.example"],
        &[ME],
        false,
        Duration::minutes(3),
    )
    .await;
    line(&fx).await;
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Updates);
    move_to(&fx, &copied.id, ModeKindData::Messages, false).await;
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Messages);
    // K supersedes the per-email move; the sender's Messages setting must
    // hold the email there, as the preview and the toast promise.
    let preview = match request(
        &fx,
        Request::MoveMessage {
            message_id: copied.id.clone(),
            mode: ModeKindData::Messages,
            sender: true,
            dry_run: true,
            source: None,
        },
    )
    .await
    {
        ResponseData::MessageMoved { outcome } => outcome,
        other => panic!("expected a preview, got {other:?}"),
    };
    assert!(preview.copy.contains("Messages"));
    move_to(&fx, &copied.id, ModeKindData::Messages, true).await;
    assert_eq!(chip(&fx, &copied.id).await.0, ArrivalBucketData::Messages);
    let held = membership(&fx, &copied.thread_id).await;
    assert!(modes(&held).contains(&ModeKindData::Messages), "{held:?}");
    assert!(!modes(&held).contains(&ModeKindData::Updates), "{held:?}");
}
