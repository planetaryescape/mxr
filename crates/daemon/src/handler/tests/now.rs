use super::desk::{request, Fixture, ME};
use super::*;
use crate::handler::now;
use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, MessageDirection};
use mxr_protocol::NowData;

fn utc() -> FixedOffset {
    FixedOffset::east_opt(0).unwrap()
}

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
    let mut envelope = fx.attention_message(thread, from, to, age, None).await;
    envelope.subject = subject.to_string();
    let direction = if from == ME {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    envelope
}

async fn owed(fx: &Fixture, who: &str) -> Envelope {
    let thread = ThreadId::new();
    let mine = fx
        .attention_message(&thread, ME, who, Duration::days(3), None)
        .await;
    fx.attention_message(
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
async fn now_rows_share_the_modes_exact_seen_revisions() {
    let fx = Fixture::new().await;
    let ask = owed(&fx, "alex@example.com").await;
    todo(&fx, &ask.id, "Send the form").await;
    mail(
        &fx,
        &ThreadId::new(),
        "security@google.com",
        "Security alert: new sign-in",
        Duration::hours(20),
    )
    .await;
    let ResponseData::Now { now } = request(&fx, Request::GetNow { account_id: None }).await else {
        panic!("Now")
    };
    let task = &now.due_soon.todos[0].todo;
    assert_eq!(task.unread, Some(true));
    let ResponseData::Todo { todo: mode_task } = request(
        &fx,
        Request::GetTodo {
            todo_id: task.id.clone(),
        },
    )
    .await
    else {
        panic!("task")
    };
    assert_eq!(mode_task.seen_revision, task.seen_revision);
    let lines = &now.updates.as_ref().unwrap().lines;
    assert!(!lines.is_empty());
    let update = &lines[0];
    for (mode, account, id, revision) in [
        (
            mxr_protocol::ModeKindData::Todo,
            &task.account_id,
            &task.id,
            &task.seen_revision,
        ),
        (
            mxr_protocol::ModeKindData::Updates,
            &update.account_id,
            &update.id,
            &update.seen_revision,
        ),
    ] {
        request(
            &fx,
            Request::SetModeRowSeen {
                account_id: account.clone(),
                mode,
                row_id: id.clone(),
                revision: revision.clone().unwrap(),
                dry_run: false,
            },
        )
        .await;
    }
    let ResponseData::Now { now: refreshed } =
        request(&fx, Request::GetNow { account_id: None }).await
    else {
        panic!("Now")
    };
    assert_eq!(refreshed.due_soon.todos[0].todo.unread, Some(false));
    assert_eq!(
        refreshed.due_soon.todos[0].todo.seen_revision,
        task.seen_revision
    );
    assert_eq!(
        refreshed.updates.as_ref().unwrap().lines[0].unread,
        Some(false)
    );
    assert_eq!(
        refreshed.updates.as_ref().unwrap().lines[0].seen_revision,
        update.seen_revision
    );
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
            Duration::hours(20),
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
    assert!(data.people.rows[0].why.starts_with("your turn since "));
    assert_eq!(data.due_soon.todos.len(), 3);
    assert_eq!(data.due_soon.total, 4);
    assert_eq!(
        data.due_soon.more_line.as_deref(),
        Some("and 1 more in To do")
    );
    assert_eq!(
        data.due_soon.todos[0].why, data.due_soon.todos[0].todo.when_label,
        "a to-do's reason is its window in words, with no mode prefix"
    );
    let card = data.updates.as_ref().expect("one Updates card");
    assert_eq!(card.message_count, 4);
    assert_eq!(card.source_count, 4);
    assert_eq!(card.top_sources.len(), 3, "the card names three sources");
    assert!(!card.early);
    assert!(data.item_count <= 10);
    assert_eq!(
        data.item_count as usize,
        data.people.rows.len()
            + data.due_soon.todos.len()
            + usize::from(data.updates.is_some())
            + usize::from(data.reading.is_some())
    );
    assert!(
        data.headline
            .contains("5 people waiting on you, 4 things to act on."),
        "{}",
        data.headline
    );
    assert!(data.empty_state.is_none());
}

#[tokio::test]
async fn the_updates_card_is_the_latest_cut_with_what_needs_a_look_first() {
    let fx = Fixture::new().await;
    let at = today_at(18);
    let mut in_cut = Vec::new();
    for (source, age, subject) in [
        ("one", Duration::hours(6), "Build passed"),
        ("two", Duration::days(30), "Build passed"),
        ("three", Duration::hours(1), "Build passed"),
        ("four", Duration::hours(7), "Payment failed: card declined"),
    ] {
        let thread = ThreadId::new();
        let mut note = mail(
            &fx,
            &thread,
            &format!("notifications@{source}.example"),
            subject,
            age,
        )
        .await;
        note.date = at - age;
        fx.store_envelope(&note, MessageDirection::Inbound).await;
        if source != "three" {
            in_cut.push(note.id);
        }
    }

    let card = now_at(&fx, at).await.updates.expect("a card");
    assert_eq!(card.since, at - Duration::minutes(90), "the 16:30 cut");
    assert_eq!(card.cut_label, "16:30");
    assert_eq!(card.message_count, 3);
    assert_eq!(card.source_count, 3);
    assert!(!card.early);
    assert!(!card.selection_token.is_empty());
    assert!(card.lines.len() <= 3);
    assert!(
        card.lines[0].fact.contains("Payment failed"),
        "{:?}",
        card.lines
    );
    assert!(card.headline.contains("needs a look"), "{}", card.headline);
    let shown: Vec<_> = card
        .lines
        .iter()
        .flat_map(|line| &line.message_ids)
        .collect();
    assert!(shown.iter().all(|id| in_cut.contains(*id)));
}
