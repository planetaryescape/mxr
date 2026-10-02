//! To do on a moved clock: lead times, surfacing once, the pay-link gate,
//! "looks done", expiry, the first run's windows and catch-up cap, and the
//! preview matching the change.

use super::desk::{request, Fixture};
use super::*;
use crate::handler::todos;
use chrono::{DateTime, Duration, Local, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{
    Address, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
};
use mxr_protocol::{
    TodoCatchupDecisionData, TodoData, TodoRunwayData, TodoStateActionData, TodoStateData,
};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};
use mxr_todo::pass::{self, PassConfig};
use std::collections::HashMap;

const CAMDEN_AUTH: &str =
    "mx.google.com; dkim=pass header.i=@camden.gov.uk; spf=pass; dmarc=pass (p=REJECT) header.from=camden.gov.uk";
const LOOKALIKE_AUTH: &str =
    "mx.google.com; dkim=pass header.i=@camden-gov.uk; spf=pass; dmarc=pass (p=NONE) header.from=camden-gov.uk";

struct Mail<'a> {
    from: (&'a str, &'a str),
    subject: &'a str,
    text: String,
    html: Option<String>,
    auth: Option<&'a str>,
    sent: DateTime<Utc>,
    outbound: bool,
}

impl<'a> Mail<'a> {
    fn new(
        from: (&'a str, &'a str),
        subject: &'a str,
        text: impl Into<String>,
        sent: DateTime<Utc>,
    ) -> Self {
        Self {
            from,
            subject,
            text: text.into(),
            html: None,
            auth: None,
            sent,
            outbound: false,
        }
    }
}

async fn put(fx: &Fixture, mail: Mail<'_>) -> MessageId {
    let id = MessageId::new();
    let thread = ThreadId::new();
    let envelope = Envelope {
        id: id.clone(),
        account_id: fx.account.clone(),
        provider_id: format!("todo-{id}"),
        thread_id: thread,
        message_id_header: Some(format!("<{id}@example.com>")),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some(mail.from.0.to_string()),
            email: mail.from.1.to_string(),
        },
        to: vec![Address {
            name: Some("Priya Shah".to_string()),
            email: if mail.outbound {
                "priya@work.com"
            } else {
                super::desk::ME
            }
            .to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: mail.subject.to_string(),
        date: mail.sent,
        flags: if mail.outbound {
            MessageFlags::READ | MessageFlags::SENT
        } else {
            MessageFlags::empty()
        },
        snippet: mail.text.chars().take(100).collect(),
        has_attachments: false,
        size_bytes: 10,
        unsubscribe: UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: vec![],
        keywords: std::collections::BTreeSet::new(),
    };
    let direction = if mail.outbound {
        MessageDirection::Outbound
    } else {
        MessageDirection::Inbound
    };
    fx.store_envelope(&envelope, direction).await;
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: id.clone(),
            text_plain: Some(mail.text),
            text_html: mail.html,
            attachments: vec![],
            fetched_at: mail.sent,
            metadata: MessageMetadata {
                auth_results: mail
                    .auth
                    .map(|auth| vec![auth.to_string()])
                    .unwrap_or_default(),
                ..MessageMetadata::default()
            },
        })
        .await
        .unwrap();
    id
}

fn cfg(now: DateTime<Utc>) -> PassConfig<Local> {
    PassConfig {
        now,
        tz: Local,
        morning_hour: 9,
        catchup_days: 14,
        catchup_max: 25,
    }
}

async fn scan(fx: &Fixture, ids: &[MessageId], now: DateTime<Utc>) -> pass::PassSummary {
    pass::scan_messages(&fx.state.store, &cfg(now), ids)
        .await
        .unwrap()
}

/// Runs the background tick until the first run is complete.
async fn finish_first_run(fx: &Fixture, now: DateTime<Utc>) {
    let mut fingerprints = HashMap::new();
    while todos::tick(&fx.state, now, &mut fingerprints)
        .await
        .unwrap()
    {}
}

async fn runway(fx: &Fixture, now: DateTime<Utc>) -> TodoRunwayData {
    todos::runway_at(&fx.state, None, true, now).await.unwrap()
}

fn titles(todos: &[TodoData]) -> Vec<&str> {
    todos.iter().map(|todo| todo.title.as_str()).collect()
}

fn coming_up(runway: &TodoRunwayData) -> Vec<&TodoData> {
    runway
        .coming_up
        .iter()
        .flat_map(|week| &week.todos)
        .collect()
}

fn invoice_html(due: DateTime<Utc>, link: &str) -> String {
    format!(
        r#"<script type="application/ld+json">{{"@type":"Invoice","provider":{{"name":"Camden Council"}},"description":"Council tax","totalPaymentDue":{{"price":"142.00","priceCurrency":"GBP"}},"paymentDueDate":"{}"}}</script><p>Your council tax is due.</p><p><a href="{link}">Pay now</a></p>"#,
        due.with_timezone(&Local).format("%Y-%m-%d")
    )
}

/// Camden's earlier statement, then its bill due in five days.
async fn camden_bill(fx: &Fixture, now: DateTime<Utc>) -> MessageId {
    let mut statement = Mail::new(
        ("Camden Council", "council.tax@camden.gov.uk"),
        "Your annual council tax statement",
        "Your annual statement is attached.",
        now - Duration::days(40),
    );
    statement.auth = Some(CAMDEN_AUTH);
    put(fx, statement).await;
    let due = now + Duration::days(5);
    let mut bill = Mail::new(
        ("Camden Council", "council.tax@camden.gov.uk"),
        "Your council tax bill",
        "Your council tax payment of £142.00 is due soon.",
        now - Duration::hours(1),
    );
    bill.html = Some(invoice_html(
        due,
        "https://www.camden.gov.uk/pay-council-tax",
    ));
    bill.auth = Some(CAMDEN_AUTH);
    put(fx, bill).await
}

#[tokio::test]
async fn a_bill_due_in_five_days_shows_up_three_days_early_once_with_a_trusted_button() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let bill = camden_bill(&fx, now).await;
    finish_first_run(&fx, now).await;

    let today = runway(&fx, now).await;
    assert!(today.now.is_empty(), "not yet: {:?}", titles(&today.now));
    let row = coming_up(&today)
        .into_iter()
        .find(|todo| todo.source_message_id.as_ref() == Some(&bill))
        .expect("in Coming up");
    assert_eq!(row.title, "Pay council tax");
    assert_eq!(row.counterparty.as_deref(), Some("Camden Council"));
    assert_eq!(
        row.amount.as_ref().map(|amount| amount.display.as_str()),
        Some("£142.00")
    );
    assert!(
        row.when_label.starts_with("shows up "),
        "{}",
        row.when_label
    );
    assert!(row.when_label.contains("act by"), "{}", row.when_label);
    let action = row.action.as_ref().expect("a button");
    assert!(action.trusted);
    assert_eq!(action.label, "Pay on camden.gov.uk");
    assert_eq!(
        today
            .empty_state
            .as_deref()
            .map(|s| s.starts_with("Nothing needs you. Next: pay council tax shows up")),
        Some(true)
    );

    let surface = row.surface_at.expect("a surface time");
    assert_eq!(
        surface.with_timezone(&Local).date_naive(),
        (row.due_at.unwrap() - Duration::days(3))
            .with_timezone(&Local)
            .date_naive()
    );
    let then = surface + Duration::minutes(1);
    let later = runway(&fx, then).await;
    assert_eq!(titles(&later.now), vec!["Pay council tax"]);
    assert!(
        later
            .headline
            .starts_with("1 thing needs you. Pay council tax, act by"),
        "{}",
        later.headline
    );

    // Announced once, even across a restart of the loop.
    assert_eq!(
        pass::sweep(&fx.state.store, then).await.unwrap().surfaced,
        vec![row.id.clone()]
    );
    finish_first_run(&fx, then + Duration::minutes(1)).await;
    assert!(pass::sweep(&fx.state.store, then + Duration::minutes(2))
        .await
        .unwrap()
        .surfaced
        .is_empty());

    // Past the due date it stays, as "was due", until you act.
    let long_after = row.due_at.unwrap() + Duration::days(30);
    pass::sweep(&fx.state.store, long_after).await.unwrap();
    let overdue = runway(&fx, long_after).await;
    assert_eq!(titles(&overdue.now), vec!["Pay council tax"]);
    assert!(
        overdue.now[0].when_label.starts_with("was due "),
        "{}",
        overdue.now[0].when_label
    );
}

#[tokio::test]
async fn a_lookalike_domain_gets_open_email_to_pay_and_its_raw_domain() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    finish_first_run(&fx, now).await;
    let mut bill = Mail::new(
        ("Camden Council", "council.tax@camden-gov.uk"),
        "Your council tax bill",
        "Your council tax payment of £142.00 is due soon.",
        now - Duration::hours(1),
    );
    bill.html = Some(invoice_html(
        now + Duration::days(1),
        "https://camden-gov.uk/pay",
    ));
    bill.auth = Some(LOOKALIKE_AUTH);
    let id = put(&fx, bill).await;
    scan(&fx, &[id], now).await;
    let today = runway(&fx, now).await;
    let action = today.now[0].action.as_ref().expect("a button");
    assert!(!action.trusted);
    assert_eq!(action.label, "Open email to pay");
    assert_eq!(action.domain.as_deref(), Some("camden-gov.uk"));
    assert_eq!(
        action.untrusted_reason.as_deref(),
        Some("there's no earlier mail from this domain")
    );
}

#[tokio::test]
async fn the_receipt_offers_looks_done_and_never_closes_the_row() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    camden_bill(&fx, now).await;
    finish_first_run(&fx, now).await;
    let mut receipt = Mail::new(
        ("Camden Council", "council.tax@camden.gov.uk"),
        "Payment received",
        "Thank you for your payment of £142.00.",
        now + Duration::hours(2),
    );
    receipt.auth = Some(CAMDEN_AUTH);
    let id = put(&fx, receipt).await;
    let summary = scan(&fx, std::slice::from_ref(&id), now + Duration::hours(2)).await;
    assert_eq!(summary.looks_done, 1);
    assert_eq!(summary.created, 0, "a receipt is not a to-do");
    let row = coming_up(&runway(&fx, now + Duration::hours(2)).await)[0].clone();
    assert_eq!(row.state, TodoStateData::Open);
    let looks = row.looks_done.expect("looks done");
    assert_eq!(looks.message_id.as_ref(), Some(&id));
    assert!(
        looks.reason.starts_with("payment received"),
        "{}",
        looks.reason
    );
}

#[tokio::test]
async fn an_invite_for_an_event_that_already_ended_never_appears() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let event = (now - Duration::days(1))
        .with_timezone(&Local)
        .format("%-d %B %Y")
        .to_string();
    let reply_by = (now - Duration::days(4))
        .with_timezone(&Local)
        .format("%-d %B %Y")
        .to_string();
    let id = put(
        &fx,
        Mail::new(
            ("Sam Okafor", "sam@work.com"),
            "Sam's leaving drinks",
            format!("Drinks on {event} at the Lamb. Please RSVP by {reply_by}."),
            now - Duration::days(9),
        ),
    )
    .await;
    let summary = scan(&fx, &[id], now).await;
    assert_eq!(summary.expired_at_birth.get("rsvp"), Some(&1));
    let today = runway(&fx, now).await;
    assert!(today.now.is_empty() && today.coming_up.is_empty() && today.whenever.is_empty());
    assert_eq!(
        today.expired_since_last_looked, 0,
        "found already over, not lapsed"
    );
    match request(
        &fx,
        Request::ListTodos {
            account_id: None,
            state: TodoStateData::Expired,
            limit: 10,
        },
    )
    .await
    {
        ResponseData::Todos { todos } => {
            assert_eq!(titles(&todos), vec!["RSVP to Sam's leaving drinks"]);
        }
        other => panic!("expected todos, got {other:?}"),
    }
}

#[tokio::test]
async fn the_first_run_caps_the_catch_up_and_its_preview_equals_the_change() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let mut ids = Vec::new();
    for n in 0..30 {
        ids.push(
            put(
                &fx,
                Mail::new(
                    ("Priya Shah via DocuSign", "dse@docusign.net"),
                    "Please DocuSign: Engagement letter",
                    format!("Priya sent you document {n} to review and sign."),
                    now - Duration::hours(n * 10 + 1),
                ),
            )
            .await,
        );
    }
    let old = put(
        &fx,
        Mail::new(
            ("Priya Shah via DocuSign", "dse@docusign.net"),
            "Please DocuSign: Old lease",
            "Please review and sign.",
            now - Duration::days(20),
        ),
    )
    .await;
    finish_first_run(&fx, now).await;

    let catchup = match request(&fx, Request::GetTodoCatchup { account_id: None }).await {
        ResponseData::TodoCatchup { catchup } => catchup,
        other => panic!("expected the catch-up, got {other:?}"),
    };
    assert_eq!(catchup.todos.len(), 25);
    assert_eq!(catchup.overflow_count, 5);
    assert!(
        catchup
            .title
            .starts_with("Catch up: 25 things from the last two weeks"),
        "{}",
        catchup.title
    );
    assert_eq!(
        catchup
            .already_over
            .iter()
            .map(|kind| kind.count)
            .sum::<u32>(),
        1,
        "the 20-day-old one"
    );
    assert!(catchup
        .todos
        .iter()
        .all(|todo| todo.source_message_id.as_ref() != Some(&old)));
    let today = runway(&fx, now).await;
    assert!(
        today.now.is_empty() && today.whenever.is_empty(),
        "the catch-up is not the runway"
    );
    assert_eq!(today.catchup_count, 25);

    let preview = match request(
        &fx,
        Request::SetTodoCatchup {
            account_id: None,
            decision: TodoCatchupDecisionData::LetGoAll,
            dry_run: true,
        },
    )
    .await
    {
        ResponseData::TodoChange { change } => change,
        other => panic!("expected a change, got {other:?}"),
    };
    assert!(preview.dry_run);
    assert_eq!(
        runway(&fx, now).await.catchup_count,
        25,
        "a preview writes nothing"
    );
    let commit = match request(
        &fx,
        Request::SetTodoCatchup {
            account_id: None,
            decision: TodoCatchupDecisionData::LetGoAll,
            dry_run: false,
        },
    )
    .await
    {
        ResponseData::TodoChange { change } => change,
        other => panic!("expected a change, got {other:?}"),
    };
    let ids_of = |todos: &[TodoData]| {
        let mut ids: Vec<String> = todos.iter().map(|todo| todo.id.clone()).collect();
        ids.sort();
        ids
    };
    assert_eq!(ids_of(&preview.changed), ids_of(&commit.changed));
    assert_eq!(commit.changed.len(), 25);
    assert_eq!(
        runway(&fx, now).await.expired_since_last_looked,
        0,
        "letting go is yours, not a lapse"
    );

    // A re-run with a new rule version brings none of it back.
    fx.state
        .store
        .start_todo_run(&fx.account, mxr_todo::RULES_VERSION, now)
        .await
        .unwrap();
    finish_first_run(&fx, now + Duration::minutes(5)).await;
    let after = runway(&fx, now + Duration::minutes(5)).await;
    assert_eq!(after.catchup_count, 0);
    assert!(after.now.is_empty() && after.whenever.is_empty());
    let _ = ids;
}

#[tokio::test]
async fn a_verify_link_lets_go_after_its_window_and_counts_as_expired_since_you_looked() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    finish_first_run(&fx, now).await;
    let id = put(
        &fx,
        Mail::new(
            ("Octopus Energy", "hello@octopus.energy"),
            "Verify your new sign-in email",
            "Please verify your email address: https://octopus.energy/verify/abc",
            now,
        ),
    )
    .await;
    scan(&fx, &[id], now).await;
    let today = runway(&fx, now).await;
    assert_eq!(
        titles(&today.now),
        vec!["Verify your email address for Octopus Energy"]
    );
    assert_eq!(
        today.now[0].when_label, "act now",
        "no stated expiry, so no expiry is claimed"
    );
    let later = now + Duration::days(4);
    let mut fingerprints = HashMap::new();
    todos::tick(&fx.state, later, &mut fingerprints)
        .await
        .unwrap();
    let after = runway(&fx, later).await;
    assert!(after.now.is_empty());
    assert_eq!(after.expired_since_last_looked, 1);
    assert_eq!(
        runway(&fx, later).await.expired_since_last_looked,
        0,
        "opening To do resets the count"
    );
}

#[tokio::test]
async fn done_undo_and_dismiss_preview_what_they_change() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    finish_first_run(&fx, now).await;
    let id = put(
        &fx,
        Mail::new(
            ("Spotify", "no-reply@spotify.com"),
            "We can't process your payment",
            "We couldn't charge your card. Update your payment details.",
            now,
        ),
    )
    .await;
    scan(&fx, &[id], now).await;
    let todo = runway(&fx, now).await.now[0].clone();
    assert_eq!(todo.title, "Fix payment for Spotify");
    let short = todo.id.trim_start_matches("todo_")[..8].to_string();
    let set = |action, dry_run| Request::SetTodoState {
        todo_ids: vec![short.clone()],
        action,
        dry_run,
    };
    let ResponseData::TodoChange { change: preview } =
        request(&fx, set(TodoStateActionData::Done, true)).await
    else {
        panic!()
    };
    assert_eq!(preview.changed[0].state, TodoStateData::Done);
    assert_eq!(
        runway(&fx, now).await.now.len(),
        1,
        "the preview wrote nothing"
    );
    let ResponseData::TodoChange { change } =
        request(&fx, set(TodoStateActionData::Done, false)).await
    else {
        panic!()
    };
    assert_eq!(change.changed[0].id, preview.changed[0].id);
    let today = runway(&fx, now).await;
    assert!(today.now.is_empty());
    assert_eq!(
        titles(&today.done_this_week),
        vec!["Fix payment for Spotify"]
    );
    let ResponseData::TodoChange { change } =
        request(&fx, set(TodoStateActionData::Done, false)).await
    else {
        panic!()
    };
    assert!(change.changed.is_empty());
    assert_eq!(change.unchanged, vec![todo.id.clone()]);
    let ResponseData::TodoChange { change } =
        request(&fx, set(TodoStateActionData::Undo, false)).await
    else {
        panic!()
    };
    assert!(change.changed[0].user_touched);
    let ResponseData::TodoChange { change } =
        request(&fx, set(TodoStateActionData::Dismiss, false)).await
    else {
        panic!()
    };
    assert_eq!(change.changed[0].state, TodoStateData::Dismissed);
}

#[tokio::test]
async fn editing_a_due_date_retimes_the_row_and_records_you_as_the_source() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    finish_first_run(&fx, now).await;
    let id = put(
        &fx,
        Mail::new(
            ("Priya Shah", "priya@work.com"),
            "Please sign the contract",
            "Please sign the contract when you can.",
            now,
        ),
    )
    .await;
    let response = request(
        &fx,
        Request::CreateTodo {
            message_id: id,
            title: "Sign the contract".into(),
            kind: Some("sign".into()),
            due: Some("in 10d".into()),
            time_zone: None,
            dry_run: false,
        },
    )
    .await;
    let ResponseData::TodoChange { change } = response else {
        panic!("{response:?}")
    };
    let todo = &change.changed[0];
    assert!(todo.user_touched);
    assert!(todo.surface_at.is_some());
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::UpdateTodo {
            todo_id: todo.id.clone(),
            edits: vec![mxr_protocol::TodoEditData {
                field: "amount".into(),
                value: "£12.50".into(),
            }],
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!()
    };
    let edited = &change.changed[0];
    assert_eq!(edited.amount.as_ref().map(|a| a.minor), Some(1250));
    let amount = edited
        .fields
        .iter()
        .find(|field| field.field == "amount")
        .expect("provenance");
    assert_eq!(amount.source, "user");
    let ResponseData::Todo { todo: why } = request(
        &fx,
        Request::GetTodo {
            todo_id: edited.id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    assert!(why
        .fields
        .iter()
        .any(|field| field.field == "act_by_at" && field.source == "table"));
}

#[tokio::test]
async fn promises_you_made_join_through_the_catch_up_and_tick_off_with_their_commitment() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let mut sent = Mail::new(
        (super::desk::ME, super::desk::ME),
        "Re: Engagement form",
        "I'll send you the signed form.",
        now - Duration::days(2),
    );
    sent.outbound = true;
    let evidence = put(&fx, sent).await;
    let thread = fx
        .state
        .store
        .get_envelope(&evidence)
        .await
        .unwrap()
        .unwrap()
        .thread_id;
    fx.state
        .store
        .upsert_contact_commitment(&ContactCommitmentRecord {
            id: "c-form".into(),
            account_id: fx.account.clone(),
            email: "priya@work.com".into(),
            thread_id: thread,
            direction: CommitmentDirection::Yours,
            status: CommitmentStatus::Open,
            who_owes: "you".into(),
            what: "send the signed form".into(),
            by_when: None,
            evidence_msg_id: evidence,
            extracted_at: now,
            resolved_at: None,
        })
        .await
        .unwrap();
    finish_first_run(&fx, now).await;
    let ResponseData::TodoCatchup { catchup } =
        request(&fx, Request::GetTodoCatchup { account_id: None }).await
    else {
        panic!()
    };
    assert_eq!(titles(&catchup.todos), vec!["Send the signed form"]);
    assert_eq!(
        catchup.todos[0].person_label.as_deref(),
        Some("you promised priya@work.com")
    );
    let id = catchup.todos[0].id.clone();
    request(
        &fx,
        Request::SetTodoCatchup {
            account_id: None,
            decision: TodoCatchupDecisionData::Keep {
                todo_ids: vec![id.clone()],
            },
            dry_run: false,
        },
    )
    .await;
    let today = runway(&fx, now).await;
    assert_eq!(titles(&today.whenever), vec!["Send the signed form"]);
    request(
        &fx,
        Request::SetTodoState {
            todo_ids: vec![id],
            action: TodoStateActionData::Done,
            dry_run: false,
        },
    )
    .await;
    let commitment = fx
        .state
        .store
        .get_contact_commitment("c-form")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(commitment.status, CommitmentStatus::Resolved);
}

#[tokio::test]
async fn an_empty_to_do_explains_what_lands_here() {
    let fx = Fixture::new().await;
    let today = runway(&fx, Utc::now()).await;
    assert_eq!(
        today.empty_state.as_deref(),
        Some(mxr_protocol::todo_copy::NEVER_HAD_ANY)
    );
    assert_eq!(today.header, mxr_protocol::todo_copy::HEADER);
}
