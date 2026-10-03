use super::*;
use chrono::TimeZone;
use mxr_core::id::MessageId;
use mxr_protocol::{DeskLaneKind, ThreadShapeData};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 2, 15, 0, 0).single().unwrap()
}

fn facts(email: &str, outbound: u32, inbound: u32, known_days: i64, last_days: i64) -> PersonFacts {
    PersonFacts {
        email: email.into(),
        display_name: None,
        first_seen_at: now() - Duration::days(known_days),
        last_inbound_at: Some(now() - Duration::days(last_days)),
        last_outbound_at: Some(now() - Duration::days(last_days)),
        total_inbound: inbound,
        total_outbound: outbound,
        replied_count: 0,
        cadence_days_p50: None,
        is_list_sender: false,
    }
}

fn topic(key: RowKey, state: TopicStateData, hours_ago: i64) -> Topic {
    let account = AccountId::new();
    let thread = ThreadId::new();
    Topic {
        key,
        data: MessagesTopicData {
            account_id: account.clone(),
            thread_id: thread.clone(),
            subject: "Contract renewal".into(),
            shape: ThreadShapeData::OneToOne,
            state,
            last_at: now() - Duration::hours(hours_ago),
            message_count: 2,
            with: vec![],
            message_ids: vec![],
            reply_to_message_id: MessageId::new(),
        },
        people: vec!["samir@launchpad.example".into()],
        turn: (state == TopicStateData::YourTurn).then(|| DeskRowData {
            lane: DeskLaneKind::Owed,
            account_id: account,
            thread_id: thread,
            message_id: MessageId::new(),
            message_ids: vec![],
            counterparty_email: "samir@launchpad.example".into(),
            counterparty_name: None,
            subject: "Contract renewal".into(),
            reason: "replied to your message".into(),
            since: now() - Duration::hours(hours_ago),
            age_seconds: hours_ago * 3600,
            usual_seconds: Some(47 * 60),
            usual_samples: 4,
            overdue: true,
            unread: true,
            starred: false,
            commitment_id: None,
            back_at: None,
        }),
        unread: false,
    }
}

#[test]
fn closeness_follows_reciprocity_recency_and_longevity() {
    let n = now();
    assert_eq!(closeness(None, n), ClosenessData::New);
    assert_eq!(closeness(Some(&facts("a@x", 0, 9, 400, 1)), n), ClosenessData::New);
    assert_eq!(closeness(Some(&facts("a@x", 48, 30, 900, 3)), n), ClosenessData::Close);
    // Lots of mail, but a year quiet: no longer close.
    assert_eq!(closeness(Some(&facts("a@x", 48, 30, 900, 200)), n), ClosenessData::Regular);
    // You write, they never answer: not close.
    assert_eq!(closeness(Some(&facts("a@x", 20, 1, 900, 3)), n), ClosenessData::Regular);
    assert_eq!(closeness(Some(&facts("a@x", 1, 1, 10, 3)), n), ClosenessData::Occasional);
}

#[test]
fn a_rows_band_follows_its_topics() {
    let key = RowKey::Person("samir@launchpad.example".into());
    let n = now();
    assert_eq!(
        band(&[topic(key.clone(), TopicStateData::Quiet, 2), topic(key.clone(), TopicStateData::YourTurn, 30)], n),
        MessagesBandData::YourTurn
    );
    assert_eq!(band(&[topic(key.clone(), TopicStateData::Waiting, 400)], n), MessagesBandData::Recent);
    assert_eq!(band(&[topic(key.clone(), TopicStateData::Quiet, 30)], n), MessagesBandData::Recent);
    assert_eq!(band(&[topic(key.clone(), TopicStateData::Quiet, 24 * 9)], n), MessagesBandData::Quiet);
    assert_eq!(band(&[topic(key, TopicStateData::Done, 1)], n), MessagesBandData::Quiet);
}

#[test]
fn your_turn_ranks_closeness_first_then_how_late() {
    let row = |id: &str, closeness: ClosenessData, hours: i64| MessagesRowData {
        id: id.into(),
        kind: MessagesRowKindData::Person,
        account_id: AccountId::new(),
        band: MessagesBandData::YourTurn,
        title: id.into(),
        person: None,
        members: vec![],
        closeness,
        your_turn: true,
        last_at: now(),
        turn_since: Some(now() - Duration::hours(hours)),
        preview: None,
        topics: vec![],
        usual_reply_seconds: None,
        pace_label: None,
        overdue: false,
        pinned: false,
        unread: false,
        why: String::new(),
    };
    let mut rows = vec![
        (row("stranger", ClosenessData::New, 50), Some(50.0)),
        (row("close-late", ClosenessData::Close, 20), Some(20.0)),
        (row("close-fresh", ClosenessData::Close, 1), Some(1.0)),
        (row("regular", ClosenessData::Regular, 90), Some(90.0)),
    ];
    rank_your_turn(&mut rows);
    let order: Vec<&str> = rows.iter().map(|(r, _)| r.id.as_str()).collect();
    assert_eq!(order, vec!["close-late", "close-fresh", "regular", "stranger"]);
}

#[test]
fn a_person_with_several_threads_is_one_row_with_your_turn_first() {
    let primary = "samir@launchpad.example".to_string();
    let key = RowKey::Person(primary.clone());
    let people = People::default();
    let facts = HashMap::from([(primary.clone(), facts(&primary, 48, 30, 900, 1))]);
    let names = HashMap::from([(primary.clone(), "Samir Patel".to_string())]);
    let pinned = HashSet::new();
    let account = AccountId::new();
    let ctx = RowContext {
        account_id: &account,
        people: &people,
        facts: &facts,
        names: &names,
        pinned: &pinned,
        now: now(),
    };
    let row = build_row(
        key,
        vec![
            topic(RowKey::Person(primary.clone()), TopicStateData::Waiting, 24),
            topic(RowKey::Person(primary.clone()), TopicStateData::YourTurn, 16),
        ],
        &ctx,
    );
    assert_eq!(row.title, "Samir Patel");
    assert_eq!(row.band, MessagesBandData::YourTurn);
    assert_eq!(row.topics.len(), 2);
    assert_eq!(row.topics[0].state, TopicStateData::YourTurn);
    assert_eq!(row.pace_label.as_deref(), Some("usually 47m"));
    assert_eq!(row.closeness, ClosenessData::Close);
    assert_eq!(
        row.why,
        "Here because: Samir replied to your message, and you write to each other often (rule)."
    );
}

#[test]
fn merged_addresses_are_one_person() {
    let people = People::new(&[PersonLink {
        email: "samir.patel@gmail.example".into(),
        person_email: "samir@launchpad.example".into(),
        linked_at: now(),
    }]);
    assert_eq!(people.primary("Samir.Patel@gmail.example"), "samir@launchpad.example");
    assert_eq!(
        people.addresses("samir@launchpad.example"),
        vec!["samir@launchpad.example".to_string(), "samir.patel@gmail.example".to_string()]
    );
}

#[test]
fn labels_read_plainly() {
    assert_eq!(pace_label(47 * 60), "usually 47m");
    assert_eq!(pace_label(5 * 3600), "usually 5h");
    assert_eq!(pace_label(3 * 86_400), "usually 3d");
    assert_eq!(preview_text("Can you take a look\n\nand reply?"), "Can you take a look and reply?");
    let long = preview_text(&"word ".repeat(60));
    assert!(long.ends_with('…') && long.chars().count() <= 141, "{long}");
    let n = now();
    assert_eq!(day_label(n - Duration::hours(1), n, &Utc), "today");
    assert_eq!(day_label(n - Duration::days(1), n, &Utc), "yesterday");
    assert_eq!(day_label(n - Duration::days(3), n, &Utc), "Tuesday");
    assert_eq!(day_label(n - Duration::days(40), n, &Utc), "23 Aug");
    assert_eq!(day_label(n - Duration::days(400), n, &Utc), "28 Aug 2025");
}
