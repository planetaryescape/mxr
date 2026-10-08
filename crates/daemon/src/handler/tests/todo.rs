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
    TodoActionKindData, TodoCatchupDecisionData, TodoData, TodoRunwayData, TodoStateActionData,
    TodoStateData,
};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};
use mxr_todo::pass;
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

async fn scan(fx: &Fixture, ids: &[MessageId], now: DateTime<Utc>) -> pass::PassSummary {
    let cfg = todos::pass_config(&fx.state, now);
    pass::scan_messages(&fx.state.store, &cfg, ids)
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
    // Three statements over 80 days: an established relationship.
    for days in [120, 80, 40] {
        let mut statement = Mail::new(
            ("Camden Council", "council.tax@camden.gov.uk"),
            "Your council tax statement",
            "Your statement is attached.",
            now - Duration::days(days),
        );
        statement.auth = Some(CAMDEN_AUTH);
        put(fx, statement).await;
    }
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
async fn a_bill_due_in_five_days_shows_up_three_days_early_once_and_opens_its_email() {
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
    // Even a genuine biller with DMARC and a long history gets no one-click
    // link: Enter opens the email with the link highlighted.
    let action = row.action.as_ref().expect("a button");
    assert_eq!(action.kind, TodoActionKindData::OpenEmail);
    assert!(!action.trusted);
    assert_eq!(action.label, "Open email to pay");
    assert_eq!(action.message_id.as_ref(), Some(&bill));
    assert_eq!(action.url, "https://www.camden.gov.uk/pay-council-tax");
    assert_eq!(action.domain.as_deref(), Some("camden.gov.uk"));
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
    scan(&fx, std::slice::from_ref(&id), now).await;
    let today = runway(&fx, now).await;
    let action = today.now[0].action.as_ref().expect("a button");
    assert_eq!(action.kind, TodoActionKindData::OpenEmail);
    assert!(!action.trusted);
    assert_eq!(action.label, "Open email to pay");
    assert_eq!(action.domain.as_deref(), Some("camden-gov.uk"));
    assert_eq!(action.message_id.as_ref(), Some(&id));
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

/// A commitment marked yours, with `evidence` as its evidence message.
async fn commitment(fx: &Fixture, id: &str, email: &str, what: &str, evidence: &MessageId) {
    let thread = fx
        .state
        .store
        .get_envelope(evidence)
        .await
        .unwrap()
        .unwrap()
        .thread_id;
    fx.state
        .store
        .upsert_contact_commitment(&ContactCommitmentRecord {
            id: id.into(),
            account_id: fx.account.clone(),
            email: email.into(),
            thread_id: thread,
            direction: CommitmentDirection::Yours,
            status: CommitmentStatus::Open,
            who_owes: "you".into(),
            what: what.into(),
            by_when: None,
            evidence_msg_id: evidence.clone(),
            extracted_at: Utc::now(),
            resolved_at: None,
        })
        .await
        .unwrap();
}

async fn promise_rows(fx: &Fixture) -> Vec<mxr_store::TodoRecord> {
    let mut rows = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    rows.sort_by(|a, b| a.title.cmp(&b.title));
    rows
}

#[tokio::test]
async fn marketing_the_model_called_yours_never_becomes_a_promise() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let offer = put(
        &fx,
        Mail::new(
            ("Example Gym", "offers@gym.example"),
            "Come back for 50% off",
            "Rejoin before 5th October and grab 50% off.",
            now - Duration::days(2),
        ),
    )
    .await;
    commitment(
        &fx,
        "c-gym",
        "offers@gym.example",
        "rejoin the gym before 5th October",
        &offer,
    )
    .await;
    finish_first_run(&fx, now).await;
    assert!(promise_rows(&fx).await.is_empty());
}

#[tokio::test]
async fn a_welcome_emails_call_to_action_is_not_a_to_do() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let welcome = put(
        &fx,
        Mail::new(
            ("Club Founders", "founders@club.example"),
            "Welcome to the Club",
            "Welcome aboard! Start the tutorial https://club.example/manual/first-steps, \
             get started with your first project, and complete your profile.",
            now - Duration::hours(3),
        ),
    )
    .await;
    // What the model made of it: a promise of yours.
    commitment(
        &fx,
        "c-welcome",
        "founders@club.example",
        "Start the tutorial https://club.example/manual/first-steps",
        &welcome,
    )
    .await;
    scan(&fx, &[welcome], now).await;
    finish_first_run(&fx, now).await;
    let ResponseData::TodoCatchup { catchup } =
        request(&fx, Request::GetTodoCatchup { account_id: None }).await
    else {
        panic!()
    };
    assert!(catchup.todos.is_empty(), "{:?}", titles(&catchup.todos));
    assert!(promise_rows(&fx).await.is_empty());
    let today = runway(&fx, now).await;
    assert!(today.whenever.is_empty());
}

#[tokio::test]
async fn a_promise_title_never_carries_a_link() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let mut sent = Mail::new(
        (super::desk::ME, super::desk::ME),
        "Re: Setup",
        "I'll go through the tutorial.",
        now - Duration::days(1),
    );
    sent.outbound = true;
    let id = put(&fx, sent).await;
    commitment(
        &fx,
        "c-link",
        "priya@work.com",
        "go through the tutorial at https://club.example/manual",
        &id,
    )
    .await;
    finish_first_run(&fx, now).await;
    let rows = promise_rows(&fx).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Go through the tutorial");
}

#[tokio::test]
async fn promise_rows_built_from_inbound_mail_are_dismissed_unless_touched() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let mut evidence = Vec::new();
    for (what, days) in [("send the deck", 2), ("send the contract", 3)] {
        let mut sent = Mail::new(
            (super::desk::ME, super::desk::ME),
            "Re: Plans",
            format!("I'll {what}."),
            now - Duration::days(days),
        );
        sent.outbound = true;
        let id = put(&fx, sent).await;
        commitment(&fx, &format!("c-{days}"), "priya@work.com", what, &id).await;
        evidence.push(id);
    }
    finish_first_run(&fx, now).await;
    let rows = promise_rows(&fx).await;
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|row| row.state == mxr_store::TodoState::Open));
    // What an earlier version left behind: the same rows, but their
    // evidence is mail someone else sent. The user moved one of them.
    for id in &evidence {
        sqlx::query("UPDATE messages SET direction = 'inbound' WHERE id = ?")
            .bind(id.as_str())
            .execute(fx.state.store.writer())
            .await
            .unwrap();
    }
    let touched = rows
        .iter()
        .find(|row| row.title == "Send the contract")
        .unwrap();
    sqlx::query("UPDATE todos SET user_edited = 1 WHERE id = ?")
        .bind(&touched.id)
        .execute(fx.state.store.writer())
        .await
        .unwrap();

    // A restart mirrors promises afresh.
    finish_first_run(&fx, now + Duration::minutes(5)).await;

    let states: Vec<(String, mxr_store::TodoState)> = promise_rows(&fx)
        .await
        .into_iter()
        .map(|row| (row.title, row.state))
        .collect();
    assert_eq!(
        states,
        vec![
            ("Send the contract".to_string(), mxr_store::TodoState::Open),
            ("Send the deck".to_string(), mxr_store::TodoState::Dismissed),
        ]
    );
}

#[tokio::test]
async fn a_promise_names_the_person_without_a_glued_on_id() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let mut sent = Mail::new(
        (super::desk::ME, super::desk::ME),
        "Re: Licence",
        "I'll update the payment details.",
        now - Duration::days(1),
    );
    sent.outbound = true;
    let id = put(&fx, sent).await;
    sqlx::query(
        "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at, refreshed_at)
         VALUES (?, 'licensing@acme.example', 'Acme Licensing2026-09-27887d708f6e134fe8bd5c9e0584954c5b', 0, 0, 0)",
    )
    .bind(fx.account.as_str())
    .execute(fx.state.store.writer())
    .await
    .unwrap();
    commitment(
        &fx,
        "c-licence",
        "licensing@acme.example",
        "update the payment details",
        &id,
    )
    .await;
    finish_first_run(&fx, now).await;
    let rows = promise_rows(&fx).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].counterparty.as_deref(), Some("Acme Licensing"));
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

#[tokio::test]
async fn a_catch_up_decision_scoped_to_one_account_never_touches_another() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    let id = put(
        &fx,
        Mail::new(
            ("Priya Shah via DocuSign", "dse@docusign.net"),
            "Please DocuSign: Engagement letter",
            "Please review and sign.",
            now - Duration::hours(3),
        ),
    )
    .await;
    scan(&fx, &[id], now).await;
    let ResponseData::TodoCatchup { catchup } =
        request(&fx, Request::GetTodoCatchup { account_id: None }).await
    else {
        panic!()
    };
    let other = mxr_core::AccountId::new();
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::SetTodoCatchup {
            account_id: Some(other),
            decision: TodoCatchupDecisionData::LetGo {
                todo_ids: vec![catchup.todos[0].id.clone()],
            },
            dry_run: false,
        },
    )
    .await
    else {
        panic!()
    };
    assert!(change.changed.is_empty());
    assert_eq!(runway(&fx, now).await.catchup_count, 1);
}

async fn todo_guide(fx: &Fixture) -> mxr_protocol::ModeGuideData {
    let ResponseData::ModeGuides { mut guides } = request(
        fx,
        Request::GetModeGuide {
            mode: Some("to-do".to_string()),
        },
    )
    .await
    else {
        panic!("expected ModeGuides")
    };
    assert_eq!(guides.len(), 1);
    guides.remove(0)
}

#[tokio::test]
async fn the_mode_guide_serves_to_do_copy_from_one_table() {
    let fx = Fixture::new().await;
    let guide = todo_guide(&fx).await;
    assert_eq!(guide.mode, "todo");
    assert_eq!(guide.header, mxr_protocol::todo_copy::HEADER);
    assert_eq!(guide.never_had_any, mxr_protocol::todo_copy::NEVER_HAD_ANY);
    assert_eq!(guide.about, mxr_protocol::todo_copy::ABOUT);
    assert_eq!(
        guide
            .hints
            .iter()
            .map(|h| h.id.as_str())
            .collect::<Vec<_>>(),
        vec!["todo.runway", "todo.catchup"]
    );
    assert!(guide.hints.iter().all(|hint| !hint.seen));
    assert!(guide
        .keys
        .iter()
        .any(|key| key.key == "e" && key.verb == "tick off"));
    let ResponseData::ModeGuides { guides } =
        request(&fx, Request::GetModeGuide { mode: None }).await
    else {
        panic!()
    };
    assert_eq!(
        guides.iter().map(|g| g.mode.as_str()).collect::<Vec<_>>(),
        vec!["now", "messages", "todo", "updates", "reading", "archive"]
    );
    let refused = handle_request(
        &fx.state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::GetModeGuide {
                mode: Some("paper trail".to_string()),
            }),
        },
    )
    .await;
    let IpcPayload::Response(Response::Error { message, .. }) = refused.payload else {
        panic!("a mode that doesn't exist is refused")
    };
    assert!(
        message.contains("Modes so far: now, messages, todo, updates, reading"),
        "{message}"
    );
}

fn hint_seen(guides: &[mxr_protocol::ModeGuideData], mode: &str, id: &str) -> Option<bool> {
    guides
        .iter()
        .find(|guide| guide.mode == mode)?
        .hints
        .iter()
        .find(|hint| hint.id == id)
        .map(|hint| hint.seen)
}

#[tokio::test]
async fn a_dismissed_hint_holds_for_every_client_until_shown_again() {
    let fx = Fixture::new().await;
    let set = |hint: &str, seen| Request::SetHintSeen {
        hint: hint.to_string(),
        seen,
    };
    let ResponseData::ModeGuides { guides } = request(&fx, set("todo.runway", true)).await else {
        panic!()
    };
    let runway = |guide: &mxr_protocol::ModeGuideData| {
        guide
            .hints
            .iter()
            .find(|hint| hint.id == "todo.runway")
            .cloned()
            .expect("runway hint")
    };
    let first = runway(&guides[0]).seen_at.expect("seen");
    // A second dismissal keeps the first time, and a fresh read agrees;
    // the mode's other hint is untouched.
    request(&fx, set("todo.runway", true)).await;
    let guide = todo_guide(&fx).await;
    assert!(runway(&guide).seen);
    assert_eq!(runway(&guide).seen_at, Some(first));
    assert_eq!(hint_seen(&[guide], "todo", "todo.catchup"), Some(false));
    request(&fx, set("todo.runway", false)).await;
    assert!(!runway(&todo_guide(&fx).await).seen);
}

#[tokio::test]
async fn a_shared_hint_is_one_hint_in_every_mode_it_attaches_in() {
    let fx = Fixture::new().await;
    let ResponseData::ModeGuides { guides } = request(
        &fx,
        Request::SetHintSeen {
            hint: "done_here".to_string(),
            seen: true,
        },
    )
    .await
    else {
        panic!()
    };
    // The answer carries every mode the hint attaches in.
    assert_eq!(
        guides.iter().map(|g| g.mode.as_str()).collect::<Vec<_>>(),
        vec!["now", "messages"]
    );
    let ResponseData::ModeGuides { guides } =
        request(&fx, Request::GetModeGuide { mode: None }).await
    else {
        panic!()
    };
    assert_eq!(hint_seen(&guides, "now", "done_here"), Some(true));
    assert_eq!(hint_seen(&guides, "messages", "done_here"), Some(true));
    assert_eq!(hint_seen(&guides, "now", "now.from_mode"), Some(false));
}

#[tokio::test]
async fn an_unknown_hint_is_refused_with_the_known_ids() {
    let fx = Fixture::new().await;
    let refused = handle_request(
        &fx.state,
        &IpcMessage {
            id: 1,
            source: ::mxr_protocol::ClientKind::default(),
            payload: IpcPayload::Request(Request::SetHintSeen {
                hint: "now.card".to_string(),
                seen: true,
            }),
        },
    )
    .await;
    let IpcPayload::Response(Response::Error { message, .. }) = refused.payload else {
        panic!("an unknown hint is refused")
    };
    assert!(message.contains("No hint \"now.card\""), "{message}");
    assert!(message.contains("todo.runway"), "{message}");
}

#[tokio::test]
async fn ticking_off_leaves_every_hint_alone() {
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
    let todo_id = runway(&fx, now).await.now[0].id.clone();
    request(
        &fx,
        Request::SetTodoState {
            todo_ids: vec![todo_id],
            action: TodoStateActionData::Done,
            dry_run: false,
        },
    )
    .await;
    // Only Esc or acting on a hint's own element dismisses it.
    assert!(todo_guide(&fx).await.hints.iter().all(|hint| !hint.seen));
}

#[tokio::test]
async fn undeciding_puts_kept_and_let_go_rows_back_in_the_catch_up() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    for n in 0..3 {
        put(
            &fx,
            Mail::new(
                ("Priya Shah via DocuSign", "dse@docusign.net"),
                "Please DocuSign: Engagement letter",
                format!("Priya sent you document {n} to review and sign."),
                now - Duration::hours(n + 1),
            ),
        )
        .await;
    }
    finish_first_run(&fx, now).await;
    let ResponseData::TodoCatchup { catchup } =
        request(&fx, Request::GetTodoCatchup { account_id: None }).await
    else {
        panic!()
    };
    let ids: Vec<String> = catchup.todos.iter().map(|todo| todo.id.clone()).collect();
    assert_eq!(ids.len(), 3);
    let decide = |decision, dry_run| Request::SetTodoCatchup {
        account_id: None,
        decision,
        dry_run,
    };
    request(
        &fx,
        decide(
            TodoCatchupDecisionData::Keep {
                todo_ids: vec![ids[0].clone()],
            },
            false,
        ),
    )
    .await;
    request(
        &fx,
        decide(
            TodoCatchupDecisionData::LetGo {
                todo_ids: vec![ids[1].clone()],
            },
            false,
        ),
    )
    .await;
    assert_eq!(runway(&fx, now).await.catchup_count, 1);

    let undecide = |dry_run| {
        decide(
            TodoCatchupDecisionData::Undecide {
                todo_ids: ids.clone(),
            },
            dry_run,
        )
    };
    let ResponseData::TodoChange { change: preview } = request(&fx, undecide(true)).await else {
        panic!()
    };
    let ResponseData::TodoChange { change } = request(&fx, undecide(false)).await else {
        panic!()
    };
    let sorted = |todos: &[TodoData]| {
        let mut ids: Vec<String> = todos.iter().map(|todo| todo.id.clone()).collect();
        ids.sort();
        ids
    };
    assert_eq!(sorted(&preview.changed), sorted(&change.changed));
    assert_eq!(
        change.changed.len(),
        2,
        "the undecided row is left as it is"
    );
    assert_eq!(change.unchanged, vec![ids[2].clone()]);
    assert!(change.changed.iter().all(
        |todo| todo.state == TodoStateData::Open && todo.catchup.as_deref() == Some("pending")
    ));
    assert_eq!(runway(&fx, now).await.catchup_count, 3);
}
