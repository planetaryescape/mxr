//! Synthetic trusted-receiver evidence gates alert to-dos (D193).
//! This tests the decision, not the live provider's authentication policy.
//! The corpus runs each case through the
//! code the daemon runs after a sync: the provider's arrival record is built
//! by the same function the Gmail and IMAP adapters call, `updates::scan`
//! decides, and the to-do table is what is counted.

use super::desk::{Fixture, ME};
use super::*;
use crate::handler::updates;
use chrono::{DateTime, Duration, FixedOffset, Local, TimeZone, Utc};
use mxr_core::id::ThreadId;
use mxr_core::types::{Envelope, MessageDirection};
use mxr_core::{ArrivalEvidence, SenderCheck, UnverifiedReason};
use mxr_mail_parse::arrival::{arrival_evidence, VerdictPolicy};

const SIGN_IN: &str = "Security alert: New sign-in from Chrome on Windows";

fn pass(host: &str) -> String {
    format!("mx.google.com; dkim=pass header.d={host}; dmarc=pass (p=REJECT) header.from={host}")
}

fn fail(host: &str) -> String {
    format!("mx.google.com; dkim=fail header.d={host}; dmarc=fail (p=NONE) header.from={host}")
}

/// What the provider adapter wrote for the message.
#[derive(Clone)]
enum Arrival {
    /// Synthetic trusted receiver: these verdict headers, in received order.
    /// This fixture does not prove Gmail authors a live API message's header.
    Gmail(Vec<String>),
    /// An IMAP server that is not Gmail, Outlook included.
    OtherImap,
    /// Synced before mxr kept arrival records.
    NotRecorded,
}

#[derive(Clone)]
struct Case {
    name: &'static str,
    from: &'static str,
    display: Option<&'static str>,
    subject: &'static str,
    received_ago: Duration,
    arrival: Arrival,
    /// Earlier mail: (host, messages, days before now of the oldest).
    prior: Vec<(&'static str, u32, i64)>,
    /// Mail from the sender that arrives after the alert.
    later: Vec<(&'static str, u32)>,
    list: bool,
    in_inbox: bool,
    is_live: bool,
    read: bool,
    /// Whether this case must make a to-do.
    creates: bool,
}

impl Case {
    fn google_sign_in(name: &'static str, creates: bool) -> Self {
        Self {
            name,
            from: "no-reply@accounts.google.com",
            display: Some("Google"),
            subject: SIGN_IN,
            received_ago: Duration::minutes(20),
            arrival: Arrival::Gmail(vec![pass("accounts.google.com")]),
            prior: vec![("accounts.google.com", 12, 200)],
            later: vec![],
            list: false,
            in_inbox: true,
            is_live: true,
            read: false,
            creates,
        }
    }
}

fn corpus() -> Vec<Case> {
    let legit = |name, creates| Case::google_sign_in(name, creates);
    let mut cases = vec![
        legit("a real Google sign-in from a long-known sender", true),
        Case {
            from: "notifications@stripe.com",
            display: Some("Stripe"),
            subject: "Your payment of $20.00 failed",
            arrival: Arrival::Gmail(vec![pass("stripe.com")]),
            prior: vec![("stripe.com", 6, 90)],
            ..legit("a real failed payment from a known biller", true)
        },
    ];
    let adversarial = |name: &'static str| legit(name, false);
    cases.extend([
        Case {
            // Round 2: a forged pass below the provider's own failure.
            arrival: Arrival::Gmail(vec![
                fail("accounts.google.com"),
                pass("accounts.google.com"),
            ]),
            ..adversarial("a forged pass below the provider's own failure")
        },
        Case {
            // …and above it.
            arrival: Arrival::Gmail(vec![
                pass("accounts.google.com"),
                fail("accounts.google.com"),
            ]),
            ..adversarial("a forged pass above the provider's own failure")
        },
        Case {
            // Round 2: a header the receiver did not write.
            arrival: Arrival::Gmail(vec![
                "evil.example; dmarc=pass header.from=accounts.google.com".into(),
            ]),
            ..adversarial("a pass from an authserv-id that is not the provider's")
        },
        Case {
            arrival: Arrival::Gmail(vec![
                "mx.google.com.evil.example; dmarc=pass header.from=accounts.google.com".into(),
            ]),
            ..adversarial("an authserv-id that merely starts with the provider's")
        },
        Case {
            arrival: Arrival::Gmail(vec![pass("google.com")]),
            ..adversarial("a pass for a different From host than the message shows")
        },
        Case {
            arrival: Arrival::Gmail(vec![]),
            ..adversarial("no provider verdict at all")
        },
        Case {
            // Round 2: the candidate counted as its own prior mail.
            prior: vec![],
            ..adversarial("a first-time sender dated before today")
        },
        Case {
            // Round 2: backfill order. The sender's history all arrives later.
            prior: vec![],
            later: vec![("accounts.google.com", 5)],
            ..adversarial("an older alert classified after the sender's newer mail")
        },
        Case {
            // Round 2: an authenticated attacker domain wearing a brand.
            from: "security@evil.example",
            display: Some("Google"),
            arrival: Arrival::Gmail(vec![pass("evil.example")]),
            prior: vec![],
            ..adversarial("a look-alike display name on the attacker's own domain")
        },
        Case {
            from: "no-reply@g00gle.com",
            arrival: Arrival::Gmail(vec![pass("g00gle.com")]),
            prior: vec![("g00gle.com", 4, 60), ("google.com", 40, 300)],
            ..adversarial("a look-alike domain that built a little history")
        },
        Case {
            prior: vec![("accounts.google.com", 5, 10)],
            ..adversarial("history that is only ten days old")
        },
        Case {
            prior: vec![("accounts.google.com", 2, 200)],
            ..adversarial("two earlier messages")
        },
        Case {
            // Round 2: Outlook carried no authserv-id and failed silently.
            arrival: Arrival::OtherImap,
            ..adversarial("an Outlook or other IMAP account")
        },
        Case {
            arrival: Arrival::NotRecorded,
            ..adversarial("mail synced before arrival records existed")
        },
        Case {
            // Round 1: archived or not in the inbox.
            in_inbox: false,
            ..adversarial("an alert that is not in the inbox")
        },
        Case {
            read: true,
            ..adversarial("an alert the user already opened")
        },
        Case {
            // Round 1: backfilled mail.
            received_ago: Duration::days(3),
            ..adversarial("backfilled mail three days old")
        },
        Case {
            is_live: false,
            ..adversarial("a fresh message on a history/backfill page")
        },
        Case {
            list: true,
            ..adversarial("an alert relayed through a mailing list")
        },
    ]);
    cases
}

struct Run {
    fx: Fixture,
    now: DateTime<Utc>,
    candidate: Envelope,
}

async fn store_prior(fx: &Fixture, host: &str, at: DateTime<Utc>) {
    let mut message = fx
        .message(
            &ThreadId::new(),
            &format!("no-reply@{host}"),
            ME,
            Duration::zero(),
            None,
        )
        .await;
    message.subject = "Your account summary".into();
    message.date = at;
    fx.store_envelope(&message, MessageDirection::Inbound).await;
    fx.state
        .store
        .record_message_arrival(
            &message.id,
            &message.account_id,
            &message.from.email,
            &ArrivalEvidence {
                received_at: Some(at),
                is_live: false,
                sender: SenderCheck::Unverified {
                    reason: UnverifiedReason::NotRecorded,
                },
            },
        )
        .await
        .unwrap();
}

fn evidence(arrival: &Arrival, from: &str, received: DateTime<Utc>) -> ArrivalEvidence {
    let mut evidence = match arrival {
        Arrival::Gmail(headers) => arrival_evidence(
            VerdictPolicy::Pinned {
                authserv_id: "mx.google.com",
            },
            headers,
            from,
            Some(received),
        ),
        Arrival::OtherImap => arrival_evidence(
            VerdictPolicy::Unavailable(UnverifiedReason::ImapGivesNoVerdict),
            &[],
            from,
            Some(received),
        ),
        Arrival::NotRecorded => ArrivalEvidence {
            received_at: Some(received),
            ..Default::default()
        },
    };
    evidence.is_live = true;
    evidence
}

/// Store the case's mail and its arrival records, without scanning.
async fn arrange(case: &Case, config: mxr_config::MxrConfig) -> Run {
    let fx = Fixture::with_config(config).await;
    let now = Utc::now();
    for (host, count, oldest_days) in &case.prior {
        for i in 0..*count {
            // Spread from the oldest toward 20 days ago.
            let days = oldest_days - i64::from(i) * ((oldest_days - 20).max(1) / i64::from(*count));
            store_prior(&fx, host, now - Duration::days(days)).await;
        }
    }
    let received = now - case.received_ago;
    let thread = ThreadId::new();
    let mut candidate = fx
        .message(&thread, case.from, ME, Duration::zero(), None)
        .await;
    candidate.subject = case.subject.into();
    candidate.date = received;
    candidate.from.name = case.display.map(str::to_string);
    if case.read {
        candidate.flags |= mxr_core::MessageFlags::READ;
    }
    fx.store_envelope(&candidate, MessageDirection::Inbound)
        .await;
    if !case.in_inbox {
        fx.state
            .store
            .set_message_labels(&candidate.id, &[], mxr_core::EventSource::User)
            .await
            .unwrap();
    }
    fx.state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: candidate.id.clone(),
            text_plain: Some("We noticed a new sign-in to your account.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: received,
            metadata: mxr_core::MessageMetadata {
                list_id: case.list.then(|| "announcements.example".to_string()),
                ..Default::default()
            },
        })
        .await
        .unwrap();
    let mut candidate_evidence = evidence(&case.arrival, case.from, received);
    candidate_evidence.is_live = case.is_live;
    fx.state
        .store
        .record_message_arrival(
            &candidate.id,
            &candidate.account_id,
            &candidate.from.email,
            &candidate_evidence,
        )
        .await
        .unwrap();
    for (host, count) in &case.later {
        for i in 0..*count {
            store_prior(&fx, host, received + Duration::minutes(1 + i64::from(i))).await;
        }
    }
    Run { fx, now, candidate }
}

async fn scan(run: &Run) {
    updates::scan(
        &run.fx.state,
        std::slice::from_ref(&run.candidate.id),
        run.now,
    )
    .await
    .unwrap();
}

async fn todos_for(run: &Run) -> Vec<mxr_store::TodoRecord> {
    run.fx
        .state
        .store
        .open_todos_for_threads(
            &run.fx.account,
            std::slice::from_ref(&run.candidate.thread_id),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn the_corpus_makes_every_legitimate_alert_and_no_forged_one() {
    let (mut false_automatic, mut missed, mut checked) = (Vec::new(), Vec::new(), 0);
    for case in corpus() {
        let run = arrange(&case, mxr_config::MxrConfig::default()).await;
        scan(&run).await;
        let made = todos_for(&run).await;
        checked += 1;
        match (case.creates, made.len()) {
            (true, 1) | (false, 0) => {}
            (true, _) => missed.push(case.name),
            (false, _) => false_automatic.push(case.name),
        }
        if let Some(todo) = made.first().filter(|_| case.creates) {
            assert_eq!(todo.origin, "rule", "{}", case.name);
            assert!(!todo.user_edited);
            assert!(
                todo.reason.starts_with("Created automatically"),
                "{}",
                todo.reason
            );
        }
    }
    println!(
        "alert corpus: {checked} cases, false_automatic={false_automatic:?}, missed={missed:?}"
    );
    assert!(
        false_automatic.is_empty(),
        "false automatic: {false_automatic:?}"
    );
    assert!(missed.is_empty(), "missed: {missed:?}");
}

#[tokio::test]
async fn the_to_do_names_the_verified_host_not_the_display_name() {
    let case = Case::google_sign_in("title", true);
    let run = arrange(&case, mxr_config::MxrConfig::default()).await;
    scan(&run).await;
    let todo = todos_for(&run).await.remove(0);
    assert_eq!(todo.title, "Check new sign-in · accounts.google.com");
    assert_eq!(todo.counterparty.as_deref(), Some("accounts.google.com"));
    assert_eq!(todo.kind, "verify");
    assert!(todo
        .dedup_key
        .starts_with("alert|accounts.google.com|sign_in|"));
}

#[tokio::test]
async fn a_resync_a_reclassification_or_a_dismissed_to_do_never_makes_a_second() {
    let case = Case::google_sign_in("claim", true);
    let run = arrange(&case, mxr_config::MxrConfig::default()).await;
    scan(&run).await;
    scan(&run).await;
    assert_eq!(todos_for(&run).await.len(), 1, "re-sync");
    let todo = todos_for(&run).await.remove(0);
    run.fx
        .state
        .store
        .set_todos_state(
            std::slice::from_ref(&todo.id),
            mxr_store::TodoState::Dismissed,
            run.now,
        )
        .await
        .unwrap();
    scan(&run).await;
    assert!(
        todos_for(&run).await.is_empty(),
        "dismissed stays dismissed"
    );
    let again = run
        .fx
        .state
        .store
        .get_todo_by_dedup(&run.fx.account, &todo.dedup_key)
        .await
        .unwrap();
    assert!(again.is_some(), "the claim row is kept");
}

#[tokio::test]
async fn a_second_alert_of_a_kind_in_a_local_day_is_one_to_do_and_the_next_day_is_another() {
    let case = Case::google_sign_in("days", true);
    let run = arrange(&case, mxr_config::MxrConfig::default()).await;
    // A differently worded alert twenty minutes later, same local day.
    let noon = Local
        .from_local_datetime(
            &run.now
                .with_timezone(&Local)
                .date_naive()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        )
        .unwrap()
        .with_timezone(&Utc);
    let make = |received: DateTime<Utc>, subject: &'static str| {
        let fx = &run.fx;
        async move {
            let mut message = fx
                .message(
                    &ThreadId::new(),
                    "no-reply@accounts.google.com",
                    ME,
                    Duration::zero(),
                    None,
                )
                .await;
            message.subject = subject.into();
            message.date = received;
            fx.store_envelope(&message, MessageDirection::Inbound).await;
            fx.state
                .store
                .insert_body(&mxr_core::MessageBody {
                    message_id: message.id.clone(),
                    text_plain: Some("We noticed a new sign-in to your account.".into()),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: received,
                    metadata: Default::default(),
                })
                .await
                .unwrap();
            fx.state
                .store
                .record_message_arrival(
                    &message.id,
                    &message.account_id,
                    &message.from.email,
                    &evidence(
                        &Arrival::Gmail(vec![pass("accounts.google.com")]),
                        "no-reply@accounts.google.com",
                        received,
                    ),
                )
                .await
                .unwrap();
            message
        }
    };
    let first = make(noon, "Security alert: New sign-in on Mac").await;
    let second = make(
        noon + Duration::minutes(20),
        "Security alert: new login to your account",
    )
    .await;
    let next_day = make(
        noon + Duration::days(1),
        "Security alert: New sign-in on iPhone",
    )
    .await;
    let count = |message: &Envelope| {
        let state = run.fx.state.clone();
        let account = run.fx.account.clone();
        let thread = message.thread_id.clone();
        async move {
            state
                .store
                .open_todos_for_threads(&account, std::slice::from_ref(&thread))
                .await
                .unwrap()
                .len()
        }
    };
    // Each scan runs one minute after its alert arrived.
    for (message, at) in [
        (&first, noon),
        (&second, noon + Duration::minutes(20)),
        (&next_day, noon + Duration::days(1)),
    ] {
        updates::scan(
            &run.fx.state,
            std::slice::from_ref(&message.id),
            at + Duration::minutes(1),
        )
        .await
        .unwrap();
    }
    assert_eq!(count(&first).await, 1);
    assert_eq!(
        count(&second).await,
        0,
        "the same local day shares one claim"
    );
    assert_eq!(count(&next_day).await, 1, "a new local day is a new claim");
}

#[tokio::test]
async fn the_switch_turns_automatic_creation_off_and_the_alert_stays_a_suggestion() {
    let mut config = mxr_config::MxrConfig::default();
    config.updates.auto_todo_alerts = false;
    let case = Case::google_sign_in("off", true);
    let run = arrange(&case, config).await;
    scan(&run).await;
    assert!(todos_for(&run).await.is_empty());
    let digest = updates::digest_at(
        &run.fx.state,
        None,
        Some(run.now),
        false,
        false,
        run.now,
        &FixedOffset::east_opt(0).unwrap(),
    )
    .await
    .unwrap();
    let line = digest
        .needs_a_look
        .iter()
        .find(|line| line.message_ids.contains(&run.candidate.id))
        .expect("the alert is in Needs a look");
    let note = line.auto_todo.as_ref().expect("a reason");
    assert_eq!(note.state, "not_created");
    assert_eq!(note.reason_code, "switched_off");
}

#[tokio::test]
async fn the_digest_says_why_an_untrusted_alert_was_not_made_and_marks_a_made_one() {
    let utc = FixedOffset::east_opt(0).unwrap();
    // An IMAP server that gives mxr no verdict.
    let imap = Case {
        arrival: Arrival::OtherImap,
        ..Case::google_sign_in("imap", false)
    };
    let run = arrange(&imap, mxr_config::MxrConfig::default()).await;
    scan(&run).await;
    let digest = updates::digest_at(
        &run.fx.state,
        None,
        Some(run.now),
        false,
        false,
        run.now,
        &utc,
    )
    .await
    .unwrap();
    let line = digest
        .needs_a_look
        .iter()
        .find(|line| line.message_ids.contains(&run.candidate.id))
        .unwrap();
    let note = line.auto_todo.as_ref().unwrap();
    assert_eq!(note.reason_code, "imap_gives_no_verdict");
    assert!(note.reason_text.starts_with("Not created automatically:"));
    let suggestion = line.todo_suggestion.as_deref().unwrap();
    assert!(suggestion.contains("t adds it."), "{suggestion}");
    assert!(suggestion.contains(&note.reason_text), "{suggestion}");
    assert!(line.todo_id.is_none());

    // A verified one: the line is already in To do, and says what confirmed it.
    let real = arrange(
        &Case::google_sign_in("real", true),
        mxr_config::MxrConfig::default(),
    )
    .await;
    scan(&real).await;
    let digest = updates::digest_at(
        &real.fx.state,
        None,
        Some(real.now),
        false,
        false,
        real.now,
        &utc,
    )
    .await
    .unwrap();
    let line = digest
        .needs_a_look
        .iter()
        .chain(&digest.changed)
        .find(|line| line.message_ids.contains(&real.candidate.id))
        .unwrap();
    assert!(line.todo_id.is_some());
    assert!(line.todo_suggestion.is_none());
    let note = line.auto_todo.as_ref().unwrap();
    assert_eq!(note.state, "created");
    assert!(note.reason_text.contains("accounts.google.com"));
}

#[tokio::test]
async fn deleting_a_source_and_resyncing_it_cannot_release_its_alert_claim() {
    let run = arrange(
        &Case::google_sign_in("delete replay", true),
        mxr_config::MxrConfig::default(),
    )
    .await;
    scan(&run).await;
    assert_eq!(todos_for(&run).await.len(), 1);
    run.fx
        .state
        .store
        .delete_messages_and_derived(
            &run.fx.account,
            std::slice::from_ref(&run.candidate.provider_id),
        )
        .await
        .unwrap();
    assert_eq!(todos_for(&run).await.len(), 0);
    run.fx
        .store_envelope(&run.candidate, MessageDirection::Inbound)
        .await;
    run.fx
        .state
        .store
        .insert_body(&mxr_core::MessageBody {
            message_id: run.candidate.id.clone(),
            text_plain: Some("We noticed a new sign-in to your account.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: run.now,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    run.fx
        .state
        .store
        .record_message_arrival(
            &run.candidate.id,
            &run.fx.account,
            &run.candidate.from.email,
            &evidence(
                &Arrival::Gmail(vec![pass("accounts.google.com")]),
                &run.candidate.from.email,
                run.candidate.date,
            ),
        )
        .await
        .unwrap();
    scan(&run).await;
    assert_eq!(
        todos_for(&run).await.len(),
        0,
        "a successful claim survives source deletion"
    );
}
