//! Now on a moved clock: the four sections in their fixed order, three
//! items each with "and N more", the evening pick, the clear state, and
//! nothing past its relevancy window.

use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::now;
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection, MessageFlags};
use mxr_protocol::NowData;

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

/// Today at `hour` UTC: the tests pick morning or evening explicitly.
fn today_at(hour: u32) -> DateTime<Utc> {
    let today = Utc::now().date_naive();
    utc()
        .from_local_datetime(&today.and_hms_opt(hour, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

async fn now_at(fx: &Fixture, at: DateTime<Utc>) -> NowData {
    now::now_at(&fx.state, None, at, &utc()).await.unwrap()
}

async fn mail(
    fx: &Fixture,
    thread: &ThreadId,
    from: &str,
    subject: &str,
    age: Duration,
) -> Envelope {
    let to = if from == ME {
        "someone@example.com"
    } else {
        ME
    };
    let mut envelope = fx.message(thread, from, to, age, None).await;
    envelope.subject = subject.to_string();
    let direction = if from == ME {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    envelope
}

/// You wrote to `who`; they replied, so it's your turn.
async fn owed(fx: &Fixture, who: &str) -> Envelope {
    let thread = ThreadId::new();
    let mine = fx.message(&thread, ME, who, Duration::days(3), None).await;
    fx.message(
        &thread,
        who,
        ME,
        Duration::hours(5),
        mine.message_id_header.as_deref(),
    )
    .await
}

async fn todo(fx: &Fixture, message: &MessageId, title: &str) -> String {
    match request(
        fx,
        Request::CreateTodo {
            message_id: message.clone(),
            title: title.into(),
            kind: None,
            due: Some("tomorrow".into()),
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    {
        ResponseData::TodoChange { change } => change.changed[0].id.clone(),
        other => panic!("expected a to-do, got {other:?}"),
    }
}

#[tokio::test]
async fn now_caps_each_section_at_three_and_says_how_many_more() {
    let fx = Fixture::new().await;
    let mut asks = Vec::new();
    for who in ["a", "b", "c", "d", "e"] {
        asks.push(owed(&fx, &format!("{who}@people.example")).await);
    }
    for (index, ask) in asks.iter().take(4).enumerate() {
        todo(&fx, &ask.id, &format!("Send form {index}")).await;
    }
    for source in ["one", "two", "three", "four"] {
        let thread = ThreadId::new();
        mail(
            &fx,
            &thread,
            &format!("notifications@{source}.example"),
            "Build passed",
            Duration::hours(1),
        )
        .await;
    }

    let at = Utc::now();
    let data = now_at(&fx, at).await;
    assert_eq!(data.people.rows.len(), 3);
    assert_eq!(data.people.total, 5);
    assert_eq!(
        data.people.more_line.as_deref(),
        Some("and 2 more in Messages")
    );
    assert!(data.people.rows[0]
        .why
        .starts_with("From Messages: your turn with "));
    assert_eq!(data.due_soon.todos.len(), 3);
    assert_eq!(data.due_soon.total, 4);
    assert_eq!(
        data.due_soon.more_line.as_deref(),
        Some("and 1 more in To do")
    );
    assert!(data.due_soon.todos[0].why.starts_with("From To do: "));
    let card = data.updates.as_ref().expect("one Updates card");
    assert_eq!(card.message_count, 4);
    assert_eq!(card.source_count, 4);
    assert_eq!(card.top_sources.len(), 3, "the card names three sources");
    assert!(card.early);
    assert!(data.item_count <= 10);
    assert_eq!(
        data.item_count as usize,
        data.people.rows.len()
            + data.due_soon.todos.len()
            + usize::from(data.updates.is_some())
            + usize::from(data.reading.is_some())
    );
    assert!(
        data.headline.contains("5 people, 4 things to act on."),
        "{}",
        data.headline
    );
    assert!(data.empty_state.is_none());
}

#[tokio::test]
async fn a_long_you_owe_becomes_one_line() {
    let fx = Fixture::new().await;
    for index in 0..7 {
        owed(&fx, &format!("p{index}@people.example")).await;
    }
    let data = now_at(&fx, Utc::now()).await;
    assert_eq!(
        data.people.overload_line.as_deref(),
        Some("7 people are waiting on you. The three below are furthest past your usual pace.")
    );
    assert_eq!(data.people.rows.len(), 3);
}

#[tokio::test]
async fn nothing_past_its_relevancy_window_enters_now() {
    let fx = Fixture::new().await;
    let ask = owed(&fx, "sam@people.example").await;
    let id = todo(&fx, &ask.id, "Sign the lease").await;
    assert_eq!(now_at(&fx, Utc::now()).await.due_soon.total, 1);

    sqlx::query("UPDATE todos SET relevant_until = ?1 WHERE id = ?2")
        .bind((Utc::now() - Duration::hours(1)).timestamp())
        .bind(&id)
        .execute(fx.state.store.writer())
        .await
        .unwrap();
    let data = now_at(&fx, Utc::now()).await;
    assert_eq!(data.due_soon.total, 0, "past its window: never on Now");
    assert_eq!(data.people.total, 1, "Messages don't expire");
}

#[tokio::test]
async fn the_reading_pick_shows_only_in_the_evening() {
    let fx = Fixture::new().await;
    // Two issues from a newsletter you read, one you haven't opened.
    for (age, read) in [(Duration::hours(30), true), (Duration::hours(2), false)] {
        let thread = ThreadId::new();
        let mut issue = mail(
            &fx,
            &thread,
            "digest@longreads.example",
            "The quiet death of the three-pane layout",
            age,
        )
        .await;
        if read {
            issue.flags |= MessageFlags::READ;
            fx.store_envelope(&issue, MessageDirection::Inbound).await;
        }
    }

    let morning = now_at(&fx, today_at(9)).await;
    assert!(morning.reading.is_none());
    assert_eq!(
        morning.not_now.as_deref(),
        Some("Not now: Reading 2 this week")
    );

    let evening = now_at(&fx, today_at(18)).await;
    let pick = evening.reading.expect("an evening pick");
    assert_eq!(pick.sender_email, "digest@longreads.example");
    assert_eq!(
        pick.why,
        "From Reading: you've read 1 of the 2 from digest@longreads.example in your inbox."
    );
    assert!(evening.not_now.is_none());
}

#[tokio::test]
async fn an_empty_now_says_clear_and_when_the_next_to_do_surfaces() {
    let fx = Fixture::new().await;
    // Mark the first run done, as it is once history is sorted.
    let mut fingerprints = std::collections::HashMap::new();
    while crate::handler::todos::tick(&fx.state, Utc::now(), &mut fingerprints)
        .await
        .unwrap()
    {}
    let thread = ThreadId::new();
    // Sent two hours ago: too soon to be waiting on anyone.
    let mine = mail(&fx, &thread, ME, "Contract", Duration::hours(2)).await;
    match request(
        &fx,
        Request::CreateTodo {
            message_id: mine.id.clone(),
            title: "Renew the contract".into(),
            kind: Some("renewal".into()),
            due: Some("in 40d".into()),
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    {
        ResponseData::TodoChange { .. } => {}
        other => panic!("expected a to-do, got {other:?}"),
    }

    let data = now_at(&fx, Utc::now()).await;
    assert_eq!(data.item_count, 0);
    let empty = data.empty_state.expect("the clear line");
    assert!(
        empty.starts_with("Clear. The next to-do surfaces "),
        "{empty}"
    );
    assert!(data.next_at.is_some());
}

#[tokio::test]
async fn people_on_now_is_your_turn_only() {
    let fx = Fixture::new().await;
    // Mark the first run done, so an empty Now says Clear.
    let mut fingerprints = std::collections::HashMap::new();
    while crate::handler::todos::tick(&fx.state, Utc::now(), &mut fingerprints)
        .await
        .unwrap()
    {}
    // You wrote last a day ago: waiting on them, not your turn.
    let thread = ThreadId::new();
    mail(&fx, &thread, ME, "Quote for the kitchen", Duration::days(1)).await;

    let data = now_at(&fx, Utc::now()).await;
    assert!(data.people.rows.is_empty(), "waiting rows are not People");
    assert_eq!(data.people.total, 0);
    assert_eq!(data.item_count, 0);
    assert!(
        data.empty_state
            .as_deref()
            .is_some_and(|line| line.starts_with("Clear.")),
        "{:?}",
        data.empty_state
    );
}

#[tokio::test]
async fn overdue_to_dos_lead_due_soon_so_the_cap_never_cuts_them() {
    let fx = Fixture::new().await;
    let mut ids = Vec::new();
    for who in ["a", "b", "c", "d"] {
        let ask = owed(&fx, &format!("{who}@people.example")).await;
        ids.push(todo(&fx, &ask.id, &format!("Send form {who}")).await);
    }
    // The last one was due yesterday.
    let late = ids.last().unwrap().clone();
    sqlx::query("UPDATE todos SET due_at = ?1, act_by_at = ?2 WHERE id = ?3")
        .bind((Utc::now() - Duration::days(1)).timestamp())
        .bind((Utc::now() - Duration::days(3)).timestamp())
        .bind(&late)
        .execute(fx.state.store.writer())
        .await
        .unwrap();

    let data = now_at(&fx, Utc::now()).await;
    assert_eq!(data.due_soon.total, 4);
    assert_eq!(data.due_soon.todos[0].todo.id, late, "overdue first");
    assert!(data.due_soon.todos[0].todo.overdue);
}

#[tokio::test]
async fn the_updates_card_covers_the_latest_cut_and_never_more_than_two_days() {
    let fx = Fixture::new().await;
    let at = today_at(18);
    // Since 08:00 today (the cut before the 16:30 one): on the card.
    for (source, age) in [("one", 1), ("two", 6)] {
        let thread = ThreadId::new();
        let mut note = mail(
            &fx,
            &thread,
            &format!("notifications@{source}.example"),
            "Build passed",
            Duration::hours(age),
        )
        .await;
        note.date = at - Duration::hours(age);
        fx.store_envelope(&note, MessageDirection::Inbound).await;
    }
    // Months old and still in the inbox: never on today's card.
    let thread = ThreadId::new();
    let mut old = mail(
        &fx,
        &thread,
        "notifications@old.example",
        "Build passed",
        Duration::days(90),
    )
    .await;
    old.date = at - Duration::days(90);
    fx.store_envelope(&old, MessageDirection::Inbound).await;

    let card = now_at(&fx, at).await.updates.expect("a card");
    assert_eq!(card.message_count, 2);
    assert_eq!(card.source_count, 2);
    assert_eq!(card.since, today_at(8));
    assert_eq!(card.thread_ids.len(), 2, "letting go acts on the card's threads");

    // In the morning the card reaches back to yesterday's 16:30 cut, but
    // never past two days.
    let morning = now_at(&fx, today_at(9)).await;
    let since = morning.updates.map(|card| card.since);
    assert!(since.is_none_or(|since| since >= today_at(9) - Duration::days(2)));
}

#[tokio::test]
async fn the_reading_pick_is_an_unread_issue_inside_its_sources_fade() {
    let fx = Fixture::new().await;
    let at = today_at(18);
    // A weekly newsletter: its fade is 14 days (twice a week, clamped).
    // The only unread issue is 40 days old, so nothing is picked.
    for (days, read) in [(40, false), (33, true), (26, true)] {
        let thread = ThreadId::new();
        let mut issue = mail(
            &fx,
            &thread,
            "digest@weekly.example",
            "This week",
            Duration::days(days),
        )
        .await;
        issue.date = at - Duration::days(days);
        if read {
            issue.flags |= MessageFlags::READ;
        }
        fx.store_envelope(&issue, MessageDirection::Inbound).await;
    }
    assert!(
        now_at(&fx, at).await.reading.is_none(),
        "an old newsletter is never tonight's pick"
    );

    let thread = ThreadId::new();
    let mut fresh = mail(
        &fx,
        &thread,
        "digest@weekly.example",
        "This week",
        Duration::days(2),
    )
    .await;
    fresh.date = at - Duration::days(2);
    fx.store_envelope(&fresh, MessageDirection::Inbound).await;
    let pick = now_at(&fx, at)
        .await
        .reading
        .expect("an issue inside the fade");
    assert_eq!(pick.message_id, fresh.id);
}
