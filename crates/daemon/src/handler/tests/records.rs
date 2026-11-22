use super::desk::{request, Fixture};
use super::*;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody, MessageDirection, MessageFlags};
use mxr_protocol::{
    RecordAnswerData, RecordChangeData, RecordData, RecordEditData, RecordFilterData,
    RecordKindData, RecordLedgerData, TodoStateActionData,
};

pub(super) async fn put(
    fx: &Fixture,
    from: (&str, &str),
    subject: &str,
    text: &str,
    html: Option<String>,
    sent: DateTime<Utc>,
) -> MessageId {
    put_in(fx, &fx.account, from, subject, text, html, sent).await
}

pub(super) async fn put_in(
    fx: &Fixture,
    account: &mxr_core::AccountId,
    from: (&str, &str),
    subject: &str,
    text: &str,
    html: Option<String>,
    sent: DateTime<Utc>,
) -> MessageId {
    let id = MessageId::new();
    let envelope = Envelope {
        id: id.clone(),
        account_id: account.clone(),
        provider_id: format!("rec-{id}"),
        thread_id: ThreadId::new(),
        message_id_header: Some(format!("<{id}@example.com>")),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some(from.0.to_string()),
            email: from.1.to_string(),
        },
        to: vec![Address {
            name: None,
            email: super::desk::ME.to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: subject.to_string(),
        date: sent,
        flags: MessageFlags::READ,
        snippet: text.chars().take(100).collect(),
        has_attachments: false,
        size_bytes: 10,
        unsubscribe: UnsubscribeMethod::None,
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: vec![],
        keywords: std::collections::BTreeSet::new(),
    };
    fx.store_envelope(&envelope, MessageDirection::Inbound)
        .await;
    fx.state
        .store
        .insert_body(&MessageBody {
            message_id: id.clone(),
            text_plain: Some(text.to_string()),
            text_html: html,
            attachments: vec![],
            fetched_at: sent,
            metadata: Default::default(),
        })
        .await
        .unwrap();
    id
}

fn ld(json: &str) -> String {
    format!(
        r#"<html><head><script type="application/ld+json">{json}</script></head><body>x</body></html>"#
    )
}

async fn ledger(fx: &Fixture, filter: RecordFilterData) -> RecordLedgerData {
    match request(
        fx,
        Request::ListRecords {
            account_id: None,
            filter,
            limit: 200,
            offset: 0,
        },
    )
    .await
    {
        ResponseData::RecordLedger { ledger } => ledger,
        other => panic!("expected a ledger, got {other:?}"),
    }
}

async fn ask(fx: &Fixture, query: &str) -> RecordAnswerData {
    ask_page(fx, query, false, 0, 200).await
}

async fn ask_page(
    fx: &Fixture,
    query: &str,
    list: bool,
    offset: u32,
    list_limit: u32,
) -> RecordAnswerData {
    match request(
        fx,
        Request::AnswerFromRecords {
            query: query.to_string(),
            account_id: None,
            fallback: true,
            limit: 4,
            list,
            offset,
            list_limit,
        },
    )
    .await
    {
        ResponseData::RecordAnswer { answer } => answer,
        other => panic!("expected an answer, got {other:?}"),
    }
}

fn change(data: ResponseData) -> RecordChangeData {
    match data {
        ResponseData::RecordChange { change } => change,
        other => panic!("expected a change, got {other:?}"),
    }
}

async fn seed(fx: &Fixture) -> Vec<MessageId> {
    let now = Utc::now();
    let ordered = now - Duration::days(40);
    let flight = now + Duration::days(3);
    let lisbon = r#"{"addressLocality":"Lisbon"}"#;
    let ids = vec![
        put(
            fx,
            ("Dell", "orders@dell.co.uk"),
            "Your Dell order confirmation",
            "Order number: 402-118",
            Some(ld(&format!(
                r#"{{"@type":"Order","merchant":{{"name":"Dell"}},"orderNumber":"402-118","orderDate":"{}","price":"1249.00","priceCurrency":"GBP","acceptedOffer":{{"itemOffered":{{"name":"XPS 14 laptop"}}}}}}"#,
                ordered.format("%Y-%m-%d")
            ))),
            ordered,
        )
        .await,
        put(
            fx,
            ("Dell", "orders@dell.co.uk"),
            "Your order 402-118 has shipped",
            "Order #402-118 is on its way.",
            None,
            ordered + Duration::days(2),
        )
        .await,
        put(
            fx,
            ("DPD", "notifications@dpd.co.uk"),
            "Delivered: your Dell parcel",
            "Order 402-118 was delivered today.",
            None,
            ordered + Duration::days(4),
        )
        .await,
        put(
            fx,
            ("TAP Air Portugal", "booking@flytap.com"),
            "Your booking is confirmed",
            "Booking reference: K7QX2M",
            Some(ld(&format!(
                r#"{{"@type":"FlightReservation","reservationNumber":"K7QX2M","reservationFor":{{"@type":"Flight","flightNumber":"1357","airline":{{"name":"TAP Air Portugal","iataCode":"TP"}},"departureAirport":{{"iataCode":"LHR"}},"arrivalAirport":{{"iataCode":"LIS","address":{lisbon}}},"departureTime":"{}"}}}}"#,
                flight.to_rfc3339()
            ))),
            now - Duration::days(60),
        )
        .await,
        put(
            fx,
            ("Hotel Lisboa Plaza", "reservations@lisboaplaza.pt"),
            "Your reservation",
            "Reservation number: 88213",
            Some(ld(&format!(
                r#"{{"@type":"LodgingReservation","reservationNumber":"88213","reservationFor":{{"name":"Hotel Lisboa Plaza","address":{lisbon}}},"checkinTime":"{}","checkoutTime":"{}"}}"#,
                (flight + Duration::hours(6)).to_rfc3339(),
                (flight + Duration::days(3)).to_rfc3339()
            ))),
            now - Duration::days(59),
        )
        .await,
    ];
    crate::handler::records::scan_messages(&fx.state, &ids).await;
    ids
}

#[tokio::test]
async fn lisbon_booking_ref_answers_with_the_reference_and_its_provenance() {
    let fx = Fixture::new().await;
    seed(&fx).await;
    let answer = ask(&fx, "lisbon booking ref").await;
    let card = answer.answer.expect("an answer card");
    assert_eq!(card.label, "Booking ref");
    assert_eq!(card.value, "K7QX2M");
    assert_eq!(card.copy, "K7QX2M");
    let provenance = card.provenance.expect("provenance");
    assert_eq!(provenance.source, "schema");
    assert!(provenance.checked);
    let trip = card.record.group.expect("part of a trip");
    assert_eq!(trip.kind, "trip");
    assert!(trip.title.starts_with("Lisbon, "), "{}", trip.title);
    assert_eq!(trip.count, 2);
    assert!(answer.fallback.is_none(), "a field matched, so no model");
}

#[tokio::test]
async fn the_dell_orders_emails_are_one_row_and_the_ledger_totals_its_month() {
    let fx = Fixture::new().await;
    seed(&fx).await;
    let all = ledger(&fx, RecordFilterData::default()).await;
    assert_eq!(all.total, 3, "Dell, the flight and the hotel");
    let orders = ledger(
        &fx,
        RecordFilterData {
            kinds: vec![RecordKindData::Order],
            ..RecordFilterData::default()
        },
    )
    .await;
    assert_eq!(orders.records.len(), 1);
    let dell = &orders.records[0];
    assert_eq!(dell.source_count, 3);
    assert_eq!(
        dell.amount.as_ref().map(|a| a.display.as_str()),
        Some("£1,249.00")
    );
    assert_eq!(
        dell.stage_line.as_deref(),
        Some("ordered · shipped · delivered")
    );
    assert_eq!(orders.months.len(), 1);
    assert_eq!(orders.months[0].count, 1);
    assert_eq!(orders.months[0].totals[0].display, "£1,249.00");
    assert!(all.coming_up.iter().any(|moment| moment.kind == "trip"));
}

#[tokio::test]
async fn a_correction_wins_over_every_rescan_and_its_preview_equals_it() {
    let fx = Fixture::new().await;
    let ids = seed(&fx).await;
    let dell = ledger(
        &fx,
        RecordFilterData {
            kinds: vec![RecordKindData::Order],
            ..RecordFilterData::default()
        },
    )
    .await
    .records
    .remove(0);
    let edit = |dry_run| Request::SetRecordField {
        record_id: dell.id.clone(),
        edit: RecordEditData::Set {
            field: "amount".to_string(),
            value: "£1,199.00".to_string(),
        },
        apply_to_sender: false,
        dry_run,
    };
    let preview = change(request(&fx, edit(true)).await);
    assert!(preview.dry_run);
    let unchanged = ledger(&fx, RecordFilterData::default()).await;
    assert!(unchanged
        .records
        .iter()
        .any(|r| r.amount.as_ref().is_some_and(|a| a.minor == 124_900)));
    let applied = change(request(&fx, edit(false)).await);
    let strip = |record: &RecordData| {
        record
            .fields
            .iter()
            .map(|f| {
                (
                    f.field.clone(),
                    f.value.clone(),
                    f.source.clone(),
                    f.checked,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(strip(&preview.records[0]), strip(&applied.records[0]));
    assert_eq!(
        applied.records[0].amount.as_ref().map(|a| a.minor),
        Some(119_900)
    );

    crate::handler::records::scan_messages(&fx.state, &ids).await;
    let after = ledger(&fx, RecordFilterData::default()).await;
    let dell_after = after
        .records
        .iter()
        .find(|r| r.id == dell.id)
        .expect("dell");
    assert_eq!(dell_after.amount.as_ref().map(|a| a.minor), Some(119_900));
    let amount = dell_after
        .fields
        .iter()
        .find(|f| f.field == "amount")
        .expect("amount");
    assert_eq!(amount.source, "user");
}

#[tokio::test]
async fn a_ticked_off_council_tax_bill_is_filed_and_undo_takes_the_tick_back() {
    let fx = Fixture::new().await;
    let due = Utc::now() + Duration::days(5);
    let bill = put(
        &fx,
        ("Camden Council", "council.tax@camden.gov.uk"),
        "Your council tax bill",
        &format!(
            "Your council tax payment of £142.00 is due on {}.",
            due.format("%-d %B %Y")
        ),
        None,
        Utc::now() - Duration::hours(20),
    )
    .await;
    assert!(
        fx.state
            .store
            .asks_for_messages(&fx.account, std::slice::from_ref(&bill))
            .await
            .unwrap()
            .is_empty(),
        "a cached body has not yet settled its asks facet"
    );
    crate::handler::todos::scan_messages(&fx.state, std::slice::from_ref(&bill)).await;
    let answers = fx
        .state
        .store
        .asks_for_messages(&fx.account, std::slice::from_ref(&bill))
        .await
        .unwrap();
    assert!(
        answers[&bill]
            .units
            .iter()
            .any(|unit| unit.answer == mxr_core::asks::AskAnswer::AsksYou),
        "canonical settlement stores the bill ask before projecting it"
    );
    let todo = fx
        .state
        .store
        .open_todos_for_threads(
            &fx.account,
            &[fx.state
                .store
                .get_envelope(&bill)
                .await
                .unwrap()
                .unwrap()
                .thread_id],
        )
        .await
        .unwrap()
        .remove(0);
    assert_eq!(ledger(&fx, RecordFilterData::default()).await.total, 0);
    let tick = |action| Request::SetTodoState {
        todo_ids: vec![todo.id.clone()],
        action,
        dry_run: false,
    };
    request(&fx, tick(TodoStateActionData::Done)).await;
    let filed = ledger(&fx, RecordFilterData::default()).await;
    assert_eq!(filed.total, 1);
    let record = &filed.records[0];
    assert_eq!(record.kind, RecordKindData::Invoice);
    assert_eq!(record.issuer.as_deref(), Some("Camden Council"));
    assert_eq!(record.amount.as_ref().map(|a| a.minor), Some(14_200));

    request(&fx, tick(TodoStateActionData::Undo)).await;
    assert_eq!(ledger(&fx, RecordFilterData::default()).await.total, 0);
}

#[tokio::test]
async fn the_prefetch_writes_no_more_than_the_budget_whatever_a_pdf_declares() {
    let fx = Fixture::new().await;
    let message = put(
        &fx,
        ("Octopus Energy", "hello@octopus.energy"),
        "Your bill is ready",
        "Account number: A-99312\nAmount due: £128.40",
        None,
        Utc::now() - Duration::days(2),
    )
    .await;
    let mut body = fx.state.store.get_body(&message).await.unwrap().unwrap();
    body.attachments.push(mxr_core::types::AttachmentMeta {
        id: mxr_core::id::AttachmentId::new(),
        message_id: message.clone(),
        filename: "bill.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        disposition: Default::default(),
        content_id: None,
        content_location: None,
        size_bytes: 10,
        local_path: None,
        provider_id: "att-1".to_string(),
    });
    fx.state.store.insert_body(&body).await.unwrap();
    crate::handler::records::scan_messages(&fx.state, std::slice::from_ref(&message)).await;
    let pdf_on_disk = |fx: &Fixture| {
        let state = fx.state.clone();
        let message = message.clone();
        async move {
            state
                .store
                .get_body(&message)
                .await
                .unwrap()
                .unwrap()
                .attachments[0]
                .local_path
                .clone()
        }
    };

    let fetched = crate::handler::records::prefetch_pdfs_within(&fx.state, 20, 1024).await;
    assert_eq!(fetched, 0);
    assert!(
        pdf_on_disk(&fx).await.is_none(),
        "nothing written over the cap"
    );
    let (_, on_disk) = fx.state.store.record_pdfs_on_disk().await.unwrap();
    assert_eq!(on_disk, 0);
    let size = fx
        .state
        .store
        .get_body(&message)
        .await
        .unwrap()
        .unwrap()
        .attachments[0]
        .size_bytes;
    assert_eq!(size, 23);

    let fetched = crate::handler::records::prefetch_pdfs_within(&fx.state, 1024, 1024).await;
    assert_eq!(fetched, 1);
    assert!(pdf_on_disk(&fx).await.is_some());
    let (bytes, count) = fx.state.store.record_pdfs_on_disk().await.unwrap();
    assert_eq!((bytes, count), (23, 1));
}

#[tokio::test]
async fn a_field_query_is_an_answer_and_show_all_lists_its_matches() {
    let fx = Fixture::new().await;
    seed(&fx).await;
    let answer = ask(&fx, "lisbon booking ref").await;
    assert_eq!(answer.mode, mxr_protocol::RecordAnswerModeData::Answer);
    assert!(answer.list.is_none());
    assert_eq!(answer.matching, 2, "the flight and the hotel");
    let all = ask_page(&fx, "lisbon booking ref", true, 0, 200).await;
    assert_eq!(all.mode, mxr_protocol::RecordAnswerModeData::List);
    let list = all.list.expect("a list");
    assert_eq!(list.count, 2);
    assert_eq!(list.top_record_id, answer.answer.expect("card").record.id);
    assert!(list.issuer.is_none());
    assert!(
        list.header
            .starts_with("\"lisbon booking ref\" \u{b7} 2 records"),
        "{}",
        list.header
    );
}
