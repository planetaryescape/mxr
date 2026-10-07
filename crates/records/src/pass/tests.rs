use super::*;
use chrono::{Duration, TimeZone};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{
    Address, BackendRef, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
    ProviderKind, UnsubscribeMethod,
};
use mxr_core::Account;
use mxr_store::{RecordQuery, TodoState};

fn at(days: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 3, 3, 9, 0, 0)
        .single()
        .expect("time")
        + Duration::days(days)
}

fn cfg() -> PassConfig<Utc> {
    PassConfig {
        now: at(30),
        tz: Utc,
    }
}

struct Fx {
    store: Store,
    account: AccountId,
    next: u32,
}

impl Fx {
    async fn new() -> Self {
        let store = Store::in_memory().await.expect("store");
        let account = Account {
            id: AccountId::new(),
            name: "Test".into(),
            email: "me@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".into(),
            }),
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.expect("account");
        Self {
            store,
            account: account.id,
            next: 0,
        }
    }

    async fn mail(
        &mut self,
        from: (&str, &str),
        subject: &str,
        text: &str,
        html: Option<&str>,
        date: DateTime<Utc>,
    ) -> MessageId {
        self.next += 1;
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: self.account.clone(),
            provider_id: format!("p{}", self.next),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some(from.0.to_string()),
                email: from.1.to_string(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            date,
            flags: MessageFlags::empty(),
            snippet: text.chars().take(80).collect(),
            has_attachments: false,
            size_bytes: 100,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        };
        self.store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .expect("envelope");
        self.store
            .insert_body(&MessageBody {
                message_id: envelope.id.clone(),
                text_plain: Some(text.to_string()),
                text_html: html.map(str::to_string),
                attachments: vec![],
                fetched_at: date,
                metadata: MessageMetadata::default(),
            })
            .await
            .expect("body");
        envelope.id
    }

    async fn records(&self) -> Vec<ArchiveRecord> {
        self.store
            .list_archive_records(&RecordQuery::default())
            .await
            .expect("records")
    }
}

const DELL: (&str, &str) = ("Dell", "orders@dell.co.uk");

#[tokio::test]
async fn the_dell_orders_three_emails_are_one_record() {
    let mut fx = Fx::new().await;
    let html = r#"<script type="application/ld+json">{"@type":"Order","merchant":{"name":"Dell"},
        "orderNumber":"402-118","orderDate":"2025-03-03","price":"1249.00","priceCurrency":"GBP",
        "acceptedOffer":{"itemOffered":{"name":"XPS 14 laptop"}}}</script>"#;
    let ids = vec![
        fx.mail(
            DELL,
            "Your Dell order confirmation",
            "Order number: 402-118",
            Some(html),
            at(0),
        )
        .await,
        fx.mail(
            DELL,
            "Your order 402-118 has shipped",
            "Order #402-118 is on its way.",
            None,
            at(2),
        )
        .await,
        fx.mail(
            ("UPS", "pkginfo@ups.com"),
            "Delivered: your parcel",
            "Order 402-118 was delivered today.",
            None,
            at(4),
        )
        .await,
    ];
    let summary = scan_messages(&fx.store, &cfg(), &ids).await.expect("scan");
    assert_eq!(summary.filed, 1);
    assert_eq!(summary.updated, 2);

    let records = fx.records().await;
    assert_eq!(records.len(), 1);
    let dell = &records[0];
    assert_eq!(dell.kind, "order");
    assert_eq!(dell.issuer.as_deref(), Some("Dell"));
    assert_eq!(dell.title.as_deref(), Some("XPS 14 laptop"));
    assert_eq!(dell.amount_minor, Some(124_900));
    assert!(
        dell.delivered_at.is_none(),
        "a rule saw 'delivered' but no tracker date"
    );
    let sources = fx
        .store
        .archive_record_sources(std::slice::from_ref(&dell.id))
        .await
        .expect("sources");
    let stages: Vec<&str> = sources.iter().map(|s| s.stage.as_str()).collect();
    assert_eq!(stages, vec!["confirmation", "shipped", "delivered"]);
    // Scanning again changes nothing.
    let again = scan_messages(&fx.store, &cfg(), &ids).await.expect("scan");
    assert_eq!((again.filed, again.updated), (0, 0));
}

#[tokio::test]
async fn overlapping_bookings_become_a_trip() {
    let mut fx = Fx::new().await;
    let flight = r#"<script type="application/ld+json">{"@type":"FlightReservation","reservationNumber":"K7QX2M",
        "reservationFor":{"@type":"Flight","flightNumber":"1357","airline":{"name":"TAP Air Portugal","iataCode":"TP"},
        "departureAirport":{"iataCode":"LHR"},"arrivalAirport":{"iataCode":"LIS","address":{"addressLocality":"Lisbon"}},
        "departureTime":"2025-06-12T07:40:00+01:00","arrivalTime":"2025-06-12T10:20:00+01:00"}}</script>"#;
    let hotel = r#"<script type="application/ld+json">{"@type":"LodgingReservation","reservationNumber":"88213",
        "reservationFor":{"name":"Hotel Lisboa Plaza","address":{"addressLocality":"Lisbon"}},
        "checkinTime":"2025-06-12T15:00:00+01:00","checkoutTime":"2025-06-15T11:00:00+01:00"}</script>"#;
    let ids = vec![
        fx.mail(
            ("TAP Air Portugal", "booking@flytap.com"),
            "Your booking",
            "Ref K7QX2M",
            Some(flight),
            at(0),
        )
        .await,
        fx.mail(
            ("Booking.com", "noreply@booking.com"),
            "Your reservation",
            "Ref 88213",
            Some(hotel),
            at(1),
        )
        .await,
    ];
    scan_messages(&fx.store, &cfg(), &ids).await.expect("scan");
    let records = fx.records().await;
    assert_eq!(records.len(), 2);
    let groups = fx.store.list_record_groups(None).await.expect("groups");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].title, "Lisbon, June 2025");
    assert!(records
        .iter()
        .all(|r| r.group_id.as_deref() == Some(groups[0].id.as_str())));
}

#[tokio::test]
async fn three_months_of_bills_are_a_series() {
    let mut fx = Fx::new().await;
    let mut ids = Vec::new();
    for month in 0..3 {
        ids.push(
            fx.mail(
                ("Octopus Energy", "hello@octopus.energy"),
                "Your bill is ready",
                &format!("Account number: A-99312-{month}\nAmount due: £128.40"),
                None,
                at(month * 31),
            )
            .await,
        );
    }
    scan_messages(&fx.store, &cfg(), &ids).await.expect("scan");
    let groups = fx.store.list_record_groups(None).await.expect("groups");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].kind, "series");
    assert_eq!(groups[0].title, "Octopus Energy bills");
}

#[tokio::test]
async fn never_file_wins_and_always_file_files_what_rules_skip() {
    let mut fx = Fx::new().await;
    let receipt = fx
        .mail(
            ("Apple", "no_reply@apple.com"),
            "Your receipt from Apple",
            "Document No. MSXK21 Total £2.99",
            None,
            at(0),
        )
        .await;
    let lease = fx
        .mail(
            ("Sam Okafor", "sam@landlord.example"),
            "Lease for next year",
            "Signed copy attached.",
            None,
            at(0),
        )
        .await;
    fx.store
        .set_record_sender_verdict(
            &fx.account,
            "no_reply@apple.com",
            Some("never"),
            None,
            at(0),
        )
        .await
        .expect("never");
    fx.store
        .set_record_sender_verdict(
            &fx.account,
            "sam@landlord.example",
            Some("always"),
            Some("contract"),
            at(0),
        )
        .await
        .expect("always");
    scan_messages(&fx.store, &cfg(), &[receipt, lease])
        .await
        .expect("scan");
    let records = fx.records().await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].kind, "contract");
    assert_eq!(records[0].origin, "sender");
}

#[tokio::test]
async fn a_ticked_off_council_tax_bill_is_a_record_and_undo_unfiles_it() {
    let mut fx = Fx::new().await;
    let source = fx
        .mail(
            ("Camden Council", "council.tax@camden.gov.uk"),
            "Council tax reminder",
            "Please pay £142.00 by 9 October.",
            None,
            at(0),
        )
        .await;
    let todo = TodoRecord {
        id: "t1".into(),
        account_id: fx.account.clone(),
        thread_id: None,
        source_message_id: Some(source.clone()),
        source_date: Some(at(0)),
        kind: "bill".into(),
        verb: "pay".into(),
        doc_type: None,
        title: "Pay council tax".into(),
        counterparty: Some("Camden Council".into()),
        sender_domain: Some("camden.gov.uk".into()),
        amount_minor: Some(14_200),
        currency: Some("GBP".into()),
        due_at: None,
        due_words: None,
        act_by_at: None,
        surface_at: None,
        scheduled_for: None,
        action_url: None,
        action_domain: None,
        relevant_until: None,
        window_source: None,
        state: TodoState::Done,
        expired_at: None,
        expired_at_birth: false,
        catchup: None,
        looks_done_message_id: None,
        looks_done_reason: None,
        origin: "rule".into(),
        reason: String::new(),
        field_sources: r#"{"amount":{"source":"rule","evidence":"£142.00"}}"#.into(),
        user_edited: false,
        commitment_id: None,
        rules_version: 1,
        dedup_key: "k".into(),
        surfaced_at: None,
        created_at: at(0),
        updated_at: at(5),
        done_at: Some(at(5)),
        dismissed_at: None,
    };
    let filed = file_from_todo(&fx.store, &cfg(), &todo)
        .await
        .expect("file");
    assert!(matches!(filed, Some(RecordFiled::Inserted { .. })));
    let records = fx.records().await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.kind, "invoice");
    assert_eq!(record.title.as_deref(), Some("Council tax"));
    assert_eq!(record.issuer.as_deref(), Some("Camden Council"));
    assert_eq!(record.amount_minor, Some(14_200));
    assert_eq!(
        record.issued_at.map(|d| d.date_naive()),
        Some(at(5).date_naive())
    );
    assert!(
        !record.checked,
        "a rule's amount stays unchecked through the to-do"
    );
    let sources = fx
        .store
        .archive_record_sources(std::slice::from_ref(&record.id))
        .await
        .expect("sources");
    assert_eq!(sources.len(), 1);

    fx.store
        .unfile_todo_record(&fx.account, "t1")
        .await
        .expect("unfile");
    assert!(fx.records().await.is_empty());
    // A to-do that leaves nothing behind files nothing.
    let verify = TodoRecord {
        kind: "verify".into(),
        ..todo
    };
    assert!(file_from_todo(&fx.store, &cfg(), &verify)
        .await
        .expect("file")
        .is_none());
}

#[tokio::test]
async fn the_manual_plan_is_what_gets_filed() {
    let mut fx = Fx::new().await;
    let quote = fx
        .mail(
            ("Kitchen Co", "jo@kitchens.example"),
            "Quote for the kitchen",
            "Total £4,200.00",
            None,
            at(0),
        )
        .await;
    let plan = plan_manual(&fx.store, &cfg(), &quote, Some(RecordKind::Contract))
        .await
        .expect("plan");
    assert_eq!(plan.kind, "contract");
    assert!(fx.records().await.is_empty(), "planning writes nothing");
    let filed = fx.store.file_record(&plan).await.expect("file");
    let record = fx
        .store
        .get_archive_record(filed.id())
        .await
        .expect("get")
        .expect("record");
    assert_eq!(record.dedup_key, plan.dedup_key);
    assert_eq!(record.amount_minor, Some(420_000));
    assert_eq!(record.origin, "manual");
}

#[tokio::test]
async fn the_first_run_reads_history_newest_first_and_finishes() {
    let mut fx = Fx::new().await;
    for day in 0..5 {
        fx.mail(
            DELL,
            "Your receipt",
            &format!("Receipt no. R-{day}000 Total £1{day}.00"),
            None,
            at(day),
        )
        .await;
    }
    let first = run_first_run(&fx.store, &cfg(), &fx.account, 2, 1)
        .await
        .expect("run");
    assert!(!first.complete);
    assert_eq!(first.scanned, 2);
    assert_eq!(
        first.reached.map(|d| d.date_naive()),
        Some(at(3).date_naive())
    );
    let done = run_first_run(&fx.store, &cfg(), &fx.account, 2, 10)
        .await
        .expect("run");
    assert!(done.complete);
    assert_eq!(fx.records().await.len(), 5);
}
