//! Synthetic trusted-receiver evidence gates alert to-dos (D193).
//! This tests the decision, not the live provider's authentication policy.
//! The corpus runs each case through the
//! code the daemon runs after a sync: the provider's arrival record is built
//! from a synthetic pinned receiver verdict, `updates::scan`
//! decides, and the to-do table is what is counted.

use super::desk::{Fixture, ME};
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
            from: "notifications@stripe.com",
            display: Some("Stripe"),
            subject: "Your payment of $20.00 failed",
            arrival: Arrival::OtherImap,
            prior: vec![],
            ..adversarial("an unverified first-time payment-failure alert")
        },
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
                message_fingerprint: Some(message.id.to_string()),
                received_at: Some(at),
                is_live: false,
                sender: SenderCheck::Verified {
                    host: host.to_string(),
                },
            },
        )
        .await
        .unwrap();
    // Fixture history was already locally observed at its logical past time.
    sqlx::query("UPDATE message_arrivals SET observed_at=? WHERE message_id=?")
        .bind(at.timestamp())
        .bind(message.id.as_str())
        .execute(fx.state.store.writer())
        .await
        .expect("past local observation");
}

/// Decision fixtures model a source whose independently verified originals
/// predate this candidate. Provider tests separately exercise real signatures.
pub(super) async fn authenticated_known_source(fx: &Fixture, message: &Envelope) {
    let host = message
        .from
        .email
        .rsplit_once('@')
        .expect("fixture address")
        .1;
    for days in [60, 40, 20] {
        store_prior(fx, host, message.date - Duration::days(days)).await;
    }
    fx.state
        .store
        .record_message_arrival(
            &message.id,
            &message.account_id,
            &message.from.email,
            &ArrivalEvidence {
                received_at: Some(message.date),
                sender: SenderCheck::Verified { host: host.into() },
                message_fingerprint: Some(message.id.to_string()),
                is_live: true,
            },
        )
        .await
        .expect("synthetic verified source");
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
    evidence.message_fingerprint = Some(mxr_core::MessageId::new().to_string());
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
    let mut invalid_provenance = Vec::new();
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
            if todo.origin != "rule"
                || todo.user_edited
                || !todo.reason.starts_with("Created automatically")
                || !todo
                    .title
                    .contains(case.from.split('@').next_back().unwrap())
            {
                invalid_provenance.push(case.name);
            }
        }
    }
    println!(
        "alert corpus: {checked} cases, false_automatic={false_automatic:?}, missed={missed:?}, invalid_provenance={invalid_provenance:?}"
    );
    assert!(
        false_automatic.is_empty(),
        "false automatic: {false_automatic:?}"
    );
    assert!(missed.is_empty(), "missed: {missed:?}");
    assert!(
        invalid_provenance.is_empty(),
        "invalid provenance: {invalid_provenance:?}"
    );
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
            &[mxr_store::TodoState::Open],
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
        .chain(&digest.since.lines)
        .find(|line| line.message_ids.contains(&run.candidate.id))
        .expect("the alert stays in the digest");
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
        .chain(&digest.since.lines)
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
        .chain(&digest.since.lines)
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

#[tokio::test]
async fn a_warmed_attacker_domain_is_an_explicit_truthfully_named_residual() {
    let case = Case {
        from: "no-reply@evil.example",
        display: Some("Google"),
        arrival: Arrival::Gmail(vec![pass("evil.example")]),
        prior: vec![("evil.example", 12, 200)],
        ..Case::google_sign_in("warmed attacker domain", true)
    };
    let run = arrange(&case, mxr_config::MxrConfig::default()).await;
    scan(&run).await;
    let made = todos_for(&run).await;
    println!(
        "documented heuristic residual: warmed_attacker_domain_created={}",
        made.len()
    );
    assert_eq!(
        made.len(),
        1,
        "sender identity does not establish alert truth"
    );
    assert_eq!(made[0].title, "Check new sign-in · evil.example");
    assert!(made[0].reason.contains("evil.example"));
    assert!(!made[0].title.contains("Google"));
}

#[tokio::test]
async fn history_excludes_the_candidate_even_when_the_cut_is_after_its_receipt() {
    let case = Case {
        prior: vec![],
        ..Case::google_sign_in("first sender", false)
    };
    let run = arrange(&case, mxr_config::MxrConfig::default()).await;
    let history = run
        .fx
        .state
        .store
        .sender_history(
            &run.fx.account,
            "accounts.google.com",
            run.now + Duration::days(1),
            &run.candidate.id,
        )
        .await
        .unwrap();
    assert_eq!(history.messages, 0);
    assert_eq!(history.oldest, None);
}

#[tokio::test]
async fn receiver_copies_cannot_manufacture_independent_history_or_borrow_an_account() {
    let run = arrange(
        &Case::google_sign_in("copied source history", true),
        mxr_config::MxrConfig::default(),
    )
    .await;
    sqlx::query(
        "UPDATE message_arrivals SET message_fingerprint='one-original' WHERE message_id<>?",
    )
    .bind(run.candidate.id.as_str())
    .execute(run.fx.state.store.writer())
    .await
    .expect("synthetic copies");
    let prior = run
        .fx
        .state
        .store
        .sender_history(
            &run.fx.account,
            "accounts.google.com",
            run.now,
            &run.candidate.id,
        )
        .await
        .expect("history");
    assert_eq!(prior.messages, 1);
    scan(&run).await;
    assert!(todos_for(&run).await.is_empty());
    let other = run
        .fx
        .state
        .store
        .sender_history(
            &mxr_core::AccountId::new(),
            "accounts.google.com",
            run.now,
            &run.candidate.id,
        )
        .await
        .expect("other account");
    assert_eq!(other.messages, 0);
    println!("copy adversaries: inflated_history_created=0 cross_account_history=0");
}

#[tokio::test]
async fn original_identity_claim_survives_a_fresh_receiver_reseal_on_a_new_day() {
    let mut run = arrange(
        &Case::google_sign_in("reseal", true),
        mxr_config::MxrConfig::default(),
    )
    .await;
    scan(&run).await;
    assert_eq!(todos_for(&run).await.len(), 1);
    let original = run
        .fx
        .state
        .store
        .message_arrival(&run.candidate.id)
        .await
        .expect("arrival")
        .expect("stored");
    run.now += Duration::days(2);
    run.candidate.id = mxr_core::MessageId::new();
    run.candidate.provider_id = "new-copy-provider-id".into();
    run.candidate.date = run.now - Duration::minutes(5);
    run.fx
        .store_envelope(&run.candidate, MessageDirection::Inbound)
        .await;
    run.fx
        .state
        .store
        .record_message_arrival(
            &run.candidate.id,
            &run.fx.account,
            &run.candidate.from.email,
            &ArrivalEvidence {
                received_at: Some(run.candidate.date),
                sender: SenderCheck::Verified {
                    host: "accounts.google.com".into(),
                },
                is_live: true,
                message_fingerprint: original.message_fingerprint,
            },
        )
        .await
        .expect("receiver reseal");
    scan(&run).await;
    assert_eq!(
        todos_for(&run).await.len(),
        1,
        "same signed original cannot claim a new local day"
    );
    println!("replay adversary: extra_tasks_on_new_day=0");
}

#[tokio::test]
async fn copied_old_originals_observed_after_the_candidate_do_not_create_source_history() {
    let run = arrange(
        &Case::google_sign_in("history copied after candidate", true),
        mxr_config::MxrConfig::default(),
    )
    .await;
    sqlx::query("UPDATE message_arrivals SET observed_at=? WHERE message_id<>?")
        .bind(run.now.timestamp())
        .bind(run.candidate.id.as_str())
        .execute(run.fx.state.store.writer())
        .await
        .expect("newly copied old originals");
    scan(&run).await;
    assert!(todos_for(&run).await.is_empty());
    println!("cross-mailbox old-copy adversary: manufactured_history_created=0");
}

#[tokio::test]
async fn original_gmail_raw_crosses_sync_store_and_daemon_with_authenticated_timing() {
    use base64::Engine;
    use mxr_core::MailSyncProvider;
    use mxr_mail_parse::receiver_arc::{
        test_fixtures::{fixture, SIGNED},
        ReceiverVerifier,
    };
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };
    let fx = Fixture::new().await;
    let now = Utc::now();
    for days in [60, 40, 20] {
        store_prior(&fx, "stripe.com", now - Duration::days(days)).await;
    }
    let (raw, dns) = fixture("google.com", "stripe.com", &SIGNED, false).await;
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw);
    let body = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode("Your payment failed. Please update your payment details.");
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/messages/signed-original")).and(query_param("format", "full"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"signed-original", "threadId":"signed-thread", "internalDate":"1", "sizeEstimate":raw.len(), "labelIds":["INBOX","UNREAD"],
            "payload":{"mimeType":"text/plain", "headers":[
                {"name":"From","value":"Stripe <notifications@stripe.com>"},
                {"name":"To","value":"user@example.com"},
                {"name":"Subject","value":"Your payment of $20.00 failed"},
                {"name":"ARC-Seal","value":"fetch original to verify"}
            ], "body":{"data":body,"size":60}}
        }))).expect(1).mount(&server).await;
    Mock::given(method("GET"))
        .and(path("/messages/signed-original"))
        .and(query_param("format", "raw"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"id":"signed-original","threadId":"signed-thread","raw":encoded}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let client = mxr_provider_gmail::client::GmailClient::new(
        mxr_provider_gmail::auth::GmailAuth::for_test_token("synthetic-token"),
    )
    .with_base_url(server.uri());
    let provider = mxr_provider_gmail::GmailProvider::new(fx.account.clone(), client)
        .with_test_receiver(ReceiverVerifier::for_test(dns));
    let synced = provider
        .fetch_message("signed-original")
        .await
        .expect("synthetic API fetch")
        .expect("message");
    assert!(
        matches!(synced.arrival.sender, SenderCheck::Verified { ref host } if host == "stripe.com")
    );
    let signed_at = synced
        .arrival
        .received_at
        .expect("authenticated handling time");
    assert!(synced.envelope.date < signed_at - Duration::days(365));
    let id = synced.envelope.id.clone();
    let mut page = [mxr_store::SyncUpsert {
        envelope: synced.envelope,
        body: synced.body,
        direction: MessageDirection::Inbound,
        label_ids: vec![fx.inbox.clone()],
        arrival: ArrivalEvidence {
            is_live: true,
            ..synced.arrival
        },
    }];
    fx.state
        .store
        .apply_sync_upserts(&mut page)
        .await
        .expect("sync transaction");
    updates::scan(&fx.state, std::slice::from_ref(&id), Utc::now())
        .await
        .expect("daemon scan");
    let tasks = fx
        .state
        .store
        .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
        .await
        .expect("tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "Fix failed payment · stripe.com");
    assert_eq!(tasks[0].source_date, Some(signed_at));
    assert!(tasks[0].relevant_until.is_none_or(|at| at > now));
    assert!(tasks[0].reason.contains("verified origin stripe.com"));
    updates::scan(&fx.state, &[id], Utc::now())
        .await
        .expect("rescan");
    assert_eq!(
        fx.state
            .store
            .list_todos_in_state(Some(&fx.account), mxr_store::TodoState::Open, 100)
            .await
            .expect("tasks")
            .len(),
        1
    );
    println!("signed Gmail RAW -> sync -> store -> daemon: created=1 rescan_extra=0 API_date_used_for_age=false");
}

#[tokio::test]
async fn filing_or_reading_between_decision_and_insert_cannot_create_an_alert_task() {
    for archive in [true, false] {
        let run = arrange(
            &Case::google_sign_in("concurrent eligibility change", true),
            mxr_config::MxrConfig::default(),
        )
        .await;
        let message = run
            .fx
            .state
            .store
            .place_message(&run.candidate.id)
            .await
            .expect("message")
            .expect("stored");
        let alert = super::super::alert_todos::Alert {
            kind: mxr_updates::NeedsYou::SignIn,
            window_until: None,
        };
        let assessment = super::super::alert_todos::assess(&run.fx.state, &message, alert, run.now)
            .await
            .expect("decision");
        let mxr_updates::trust::Decision::Create(claim) = assessment.decision else {
            std::panic::resume_unwind(Box::new("eligible before user action"));
        };
        let record = super::super::alert_todos::alert_record(
            &run.fx.state,
            &message,
            &claim,
            assessment.history,
            assessment.handled_at,
            run.now,
        );
        if archive {
            run.fx
                .state
                .store
                .set_message_labels(&run.candidate.id, &[], mxr_core::EventSource::User)
                .await
                .expect("archive");
        } else {
            sqlx::query("UPDATE messages SET flags=flags | ? WHERE id=?")
                .bind(i64::from(mxr_core::MessageFlags::READ.bits()))
                .bind(run.candidate.id.as_str())
                .execute(run.fx.state.store.writer())
                .await
                .expect("opened elsewhere");
        }
        assert!(!run
            .fx
            .state
            .store
            .insert_alert_todo(&record, claim.kind.as_str())
            .await
            .expect("atomic insert fence"));
        assert!(todos_for(&run).await.is_empty());
    }
}

#[tokio::test]
async fn independent_remediation_in_a_first_time_alert_cannot_bypass_receiver_history() {
    let case = Case {
        from: "notifications@stripe.com",
        subject: "Your payment failed",
        prior: vec![],
        arrival: Arrival::Gmail(vec![pass("stripe.com")]),
        ..Case::google_sign_in("first-time independent remediation", false)
    };
    let run = arrange(&case, mxr_config::MxrConfig::default()).await;
    super::promise_projection::replace(
        &run.fx,
        &run.candidate,
        "Your payment failed.\n\nPlease update your payment details.",
    )
    .await;
    scan(&run).await;
    assert!(todos_for(&run).await.is_empty());
}
