use super::desk::{request, Fixture};
use super::*;
use crate::handler::todos;
use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{
    Address, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
};
use mxr_protocol::{
    TodoActionKindData, TodoCatchupDecisionData, TodoData, TodoResolutionActionData,
    TodoRunwayData, TodoStateActionData, TodoStateData,
};
use mxr_store::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};
use mxr_todo::pass;
use std::collections::HashMap;

mod bank_transfer;
mod html_evidence;
mod provenance_cases;
mod resolution_budget;
mod same_email;

#[tokio::test]
async fn row_seen_is_not_email_read_or_completion_and_old_peeks_do_not_hide_edits() {
    let fx = Fixture::new().await;
    let message = fx
        .message(
            &ThreadId::new(),
            "alex@example.com",
            super::desk::ME,
            Duration::hours(1),
            None,
        )
        .await;
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CreateTodo {
            message_id: message.id.clone(),
            title: "Send the form".into(),
            kind: None,
            due: None,
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!("expected created task")
    };
    let original = change.changed[0].clone();
    assert_eq!(original.unread, Some(true));
    let get = || Request::GetTodo {
        todo_id: original.id.clone(),
    };
    let mark = |revision: &str, dry_run| Request::SetModeRowSeen {
        account_id: original.account_id.clone(),
        mode: mxr_protocol::ModeKindData::Todo,
        row_id: original.id.clone(),
        revision: revision.into(),
        dry_run,
    };
    let rev = original.seen_revision.as_deref().unwrap();
    fx.state
        .store
        .set_read(&message.id, true, mxr_core::EventSource::Sync)
        .await
        .unwrap();
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_eq!(
        todo.unread,
        Some(true),
        "reading email is not opening the row"
    );
    assert_eq!(todo.seen_revision, original.seen_revision);
    request(&fx, mark(rev, true)).await;
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_eq!(
        todo.unread,
        Some(true),
        "preview/list reads do not mark seen"
    );
    request(&fx, mark(rev, false)).await;
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_eq!(todo.unread, Some(false));
    assert_eq!(todo.state, TodoStateData::Open, "seen is not done");
    fx.state
        .store
        .set_read(&message.id, false, mxr_core::EventSource::Sync)
        .await
        .unwrap();
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_eq!(
        todo.unread,
        Some(false),
        "provider unread cannot revive row attention"
    );
    request(
        &fx,
        Request::UpdateTodo {
            todo_id: original.id.clone(),
            edits: vec![mxr_protocol::TodoEditData {
                field: "title".into(),
                value: "Send the revised form".into(),
            }],
            time_zone: None,
            dry_run: false,
        },
    )
    .await;
    request(&fx, mark(rev, false)).await;
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_ne!(todo.seen_revision, original.seen_revision);
    assert_eq!(
        todo.unread,
        Some(true),
        "an old peek cannot calm new content"
    );
    request(&fx, mark(todo.seen_revision.as_deref().unwrap(), false)).await;
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_eq!(todo.unread, Some(false));
    for action in [TodoStateActionData::Done, TodoStateActionData::Undo] {
        request(
            &fx,
            Request::SetTodoState {
                todo_ids: vec![original.id.clone()],
                action,
                dry_run: false,
            },
        )
        .await;
    }
    let ResponseData::Todo { todo } = request(&fx, get()).await else {
        panic!()
    };
    assert_eq!(todo.state, TodoStateData::Open);
    assert_eq!(
        todo.unread,
        Some(false),
        "reopening a task does not unsee its unchanged content"
    );
}

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
    super::projection_diagnostics::trace(&fx.state, &fx.account, "before asks reconciliation")
        .await;
    let mut summary =
        super::super::assessments::reconcile_messages_at(&fx.state, &fx.account, ids, now)
            .await
            .unwrap();
    super::projection_diagnostics::trace(&fx.state, &fx.account, "after asks reconciliation").await;
    let cfg = todos::pass_config(&fx.state, now);
    summary.add(
        &pass::scan_messages(&fx.state.store, &cfg, ids)
            .await
            .unwrap(),
    );
    summary
}

async fn finish_first_run(fx: &Fixture, now: DateTime<Utc>) {
    let mut fingerprints = HashMap::new();
    super::projection_diagnostics::trace(&fx.state, &fx.account, "before first run").await;
    while todos::tick(&fx.state, now, &mut fingerprints)
        .await
        .unwrap()
    {}
    super::projection_diagnostics::trace(&fx.state, &fx.account, "after first run").await;
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

#[tokio::test]
async fn resolution_preview_projects_the_same_task_fields_as_accept_and_undo() {
    let fx = Fixture::new().await;
    let now = Utc::now();
    finish_first_run(&fx, now).await;
    let original = put(
        &fx,
        Mail::new(
            ("Camden Council", "bills@camden.gov.uk"),
            "Your council tax bill",
            "Your Camden Council Tax payment is due.",
            now - Duration::days(2),
        ),
    )
    .await;
    let ResponseData::TodoChange { change } = request(
        &fx,
        Request::CreateTodo {
            message_id: original,
            title: "Pay Camden Council Tax".into(),
            kind: Some("bill".into()),
            due: Some("in 10d".into()),
            time_zone: None,
            dry_run: false,
        },
    )
    .await
    else {
        panic!()
    };
    let todo_id = change.changed[0].id.clone();
    let later = put(
        &fx,
        Mail::new(
            ("Camden Council", "bills@camden.gov.uk"),
            "Payment received",
            "Camden Council Tax payment received.",
            now - Duration::days(1),
        ),
    )
    .await;
    todos::scan_messages(&fx.state, std::slice::from_ref(&later)).await;
    let ResponseData::Todo { todo } = request(
        &fx,
        Request::GetTodo {
            todo_id: todo_id.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    let offer = todo.resolution.expect("later payment evidence");
    let resolve = |action, dry_run| Request::ResolveTodoSuggestion {
        todo_id: todo_id.clone(),
        suggestion_id: offer.id.clone(),
        action,
        dry_run,
    };

    let ResponseData::TodoChange { change: preview } =
        request(&fx, resolve(TodoResolutionActionData::Preview, true)).await
    else {
        panic!()
    };
    assert_eq!(preview.changed[0].state, TodoStateData::Done);
    assert!(preview.changed[0].done_at.is_some());
    assert_eq!(
        fx.state
            .store
            .get_todo(&todo_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        mxr_store::TodoState::Open
    );

    let ResponseData::TodoChange { change: applied } =
        request(&fx, resolve(TodoResolutionActionData::Accept, false)).await
    else {
        panic!()
    };
    assert_eq!(preview.changed[0].state, applied.changed[0].state);
    assert!(applied.changed[0].done_at.is_some());

    let ResponseData::TodoChange {
        change: undo_preview,
    } = request(&fx, resolve(TodoResolutionActionData::Undo, true)).await
    else {
        panic!()
    };
    assert_eq!(undo_preview.changed[0].state, TodoStateData::Open);
    let ResponseData::TodoChange { change: undone } =
        request(&fx, resolve(TodoResolutionActionData::Undo, false)).await
    else {
        panic!()
    };
    assert_eq!(undo_preview.changed[0].state, undone.changed[0].state);
    assert_eq!(
        fx.state
            .store
            .get_todo(&todo_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        mxr_store::TodoState::Open
    );
}

fn invoice_html(due: DateTime<Utc>, link: &str) -> String {
    format!(
        r#"<script type="application/ld+json">{{"@type":"Invoice","provider":{{"name":"Camden Council"}},"description":"Council tax","totalPaymentDue":{{"price":"142.00","priceCurrency":"GBP"}},"paymentDueDate":"{}"}}</script><p>Your council tax is due.</p><p><a href="{link}">Pay now</a></p>"#,
        due.with_timezone(&Local).format("%Y-%m-%d")
    )
}

async fn configure_receiver(fx: &Fixture) {
    let mut config = fx.state.config_snapshot();
    config
        .todo
        .trusted_authentication_services
        .insert(fx.account.as_str(), vec!["mx.google.com".into()]);
    fx.state.set_config_for_test(config).await;
}

async fn camden_bill(fx: &Fixture, now: DateTime<Utc>) -> MessageId {
    configure_receiver(fx).await;
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
    let now = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
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
        row.due_at.unwrap().with_timezone(&Local).date_naive() - Duration::days(3)
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
    configure_receiver(&fx).await;
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
    let envelope = fx.state.store.get_envelope(&id).await.unwrap().unwrap();
    super::alert_todos::authenticated_known_source(&fx, &envelope).await;
    scan(&fx, &[id], now).await;
    let todo = runway(&fx, now).await.now[0].clone();
    assert_eq!(todo.title, "Fix failed payment · spotify.com");
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
        vec!["Fix failed payment · spotify.com"]
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
            by_when_source: None,
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
    for id in &evidence {
        let mut incoming = fx.state.store.get_envelope(id).await.unwrap().unwrap();
        incoming.from.email = "someone-else@example.com".into();
        incoming.flags.remove(MessageFlags::SENT);
        fx.store_envelope(&incoming, MessageDirection::Inbound)
            .await;
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

    crate::handler::assessments::reconcile_messages(&fx.state, &fx.account, &evidence)
        .await
        .unwrap();
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
    request(&fx, set("todo.runway", true)).await;
    let guide = todo_guide(&fx).await;
    assert!(runway(&guide).seen);
    assert_eq!(runway(&guide).seen_at, Some(first));
    assert_eq!(hint_seen(&[guide], "todo", "todo.catchup"), Some(false));
    request(&fx, set("todo.runway", false)).await;
    assert!(!runway(&todo_guide(&fx).await).seen);
}
