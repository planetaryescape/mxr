use super::*;
use chrono::{TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{
    RecordAmountData, RecordAnswerCardData, RecordAnswerData, RecordDocumentData,
    RecordFacetCountData, RecordFacetsData, RecordFieldData, RecordFilterData, RecordGroupData,
    RecordKindData, RecordMomentData, RecordMonthData, RecordSourceData, ARCHIVE_GUIDE,
};
use mxr_test_support::render_to_string;

fn at(y: i32, m: u32, d: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 12, 0, 0)
        .single()
        .expect("day")
}

fn field(name: &str, label: &str, value: &str, source: &str, checked: bool) -> RecordFieldData {
    RecordFieldData {
        field: name.into(),
        label: label.into(),
        value: value.into(),
        copy: value.into(),
        source: source.into(),
        source_label: match source {
            "schema" => "schema.org markup".into(),
            "user" => "you".into(),
            _ => "a pattern in the email".into(),
        },
        checked,
        evidence: None,
        message_id: None,
    }
}

pub(crate) fn record(
    id: &str,
    kind: RecordKindData,
    issuer: &str,
    title: &str,
    amount: Option<(i64, &str)>,
    reference: Option<&str>,
    date: chrono::DateTime<Utc>,
) -> RecordData {
    let amount = amount.map(|(minor, display)| RecordAmountData {
        minor,
        currency: "GBP".into(),
        display: display.into(),
    });
    let mut fields = vec![
        field("issuer", "From", issuer, "schema", true),
        field("title", "What", title, "schema", true),
        field(
            "issued_at",
            "Date",
            &date.format("%-d %b %Y").to_string(),
            "schema",
            true,
        ),
    ];
    if let Some(amount) = &amount {
        fields.push(field("amount", "Paid", &amount.display, "schema", true));
    }
    if let Some(reference) = reference {
        fields.push(field("reference", "Order", reference, "schema", true));
    }
    RecordData {
        id: id.into(),
        account_id: AccountId::new(),
        kind,
        kind_label: format!("{kind:?}"),
        issuer: Some(issuer.into()),
        title: Some(title.into()),
        reference: reference.map(str::to_string),
        reference_label: "Order".into(),
        amount,
        date: Some(date),
        span_start: None,
        span_end: None,
        place: None,
        delivered_at: None,
        return_by: None,
        warranty_until: None,
        valid_until: None,
        checked: true,
        unchecked_fields: Vec::new(),
        stage_line: None,
        detail_line: None,
        pdf: None,
        document_count: 0,
        source_count: 1,
        group: None,
        why: "Here because: order confirmation with schema.org markup (checked).".into(),
        origin: "schema".into(),
        thread_id: None,
        message_id: Some(MessageId::new()),
        dismissed: false,
        fields,
        documents: Vec::new(),
        sources: Vec::new(),
        issuer_records: None,
    }
}

fn pdf(message_id: &MessageId, name: &str) -> RecordDocumentData {
    RecordDocumentData {
        message_id: message_id.clone(),
        attachment_id: "5f2e3a1c-0000-4000-8000-000000000001".into(),
        filename: name.into(),
        mime_type: "application/pdf".into(),
        size_bytes: 186_000,
        is_pdf: true,
        on_disk: false,
    }
}

pub(crate) fn dell() -> RecordData {
    let mut dell = record(
        "rec_dell",
        RecordKindData::Order,
        "Dell",
        "XPS 14 laptop",
        Some((124_900, "\u{a3}1,249.00")),
        Some("402-118"),
        at(2025, 3, 3),
    );
    dell.stage_line = Some("ordered \u{b7} shipped \u{b7} delivered 7 Mar".into());
    dell.detail_line = Some("Return by 2 Apr (passed) \u{b7} Warranty to 3 Mar 2027".into());
    let message = dell.message_id.clone().expect("message");
    dell.pdf = Some(pdf(&message, "Invoice-402118.pdf"));
    dell.document_count = 1;
    dell.source_count = 3;
    dell
}

pub(crate) fn apple() -> RecordData {
    let mut apple = record(
        "rec_apple",
        RecordKindData::Receipt,
        "Apple",
        "iCloud+ 200GB",
        Some((299, "\u{a3}2.99")),
        Some("MSXK21"),
        at(2025, 3, 28),
    );
    apple.checked = false;
    apple.unchecked_fields = vec!["amount".into()];
    apple.why = "Here because: \"receipt\" in the subject (unchecked).".into();
    apple
}

pub(crate) fn octopus() -> RecordData {
    record(
        "rec_octopus",
        RecordKindData::Statement,
        "Octopus Energy",
        "Bill, Feb",
        Some((12_840, "\u{a3}128.40")),
        None,
        at(2025, 2, 11),
    )
}

fn month(month: &str, label: &str, count: u32, total: &str) -> RecordMonthData {
    RecordMonthData {
        month: month.into(),
        label: label.into(),
        count,
        totals: vec![RecordAmountData {
            minor: 0,
            currency: "GBP".into(),
            display: total.into(),
        }],
    }
}

pub(crate) fn ledger(records: Vec<RecordData>) -> RecordLedgerData {
    let total = u32::try_from(records.len()).expect("count");
    RecordLedgerData {
        header: ARCHIVE_GUIDE.header.into(),
        total,
        matching: total,
        months: vec![
            month("2025-03", "2025 \u{b7} March", 2, "\u{a3}1,251.99"),
            month("2025-02", "2025 \u{b7} February", 1, "\u{a3}128.40"),
        ],
        records,
        facets: RecordFacetsData {
            years: vec![
                RecordFacetCountData {
                    value: "2025".into(),
                    label: "2025".into(),
                    count: 3,
                },
                RecordFacetCountData {
                    value: "2024".into(),
                    label: "2024".into(),
                    count: 1,
                },
            ],
            ..RecordFacetsData::default()
        },
        coming_up: vec![RecordMomentData {
            kind: "trip".into(),
            record_id: "rec_tap".into(),
            group_id: Some("grp_1".into()),
            at: at(2025, 6, 12),
            label: "Lisbon, June 2025 \u{b7} in 3 days".into(),
        }],
        filter: RecordFilterData::default(),
        issuer: None,
        empty_state: None,
        first_run: None,
    }
}

pub(crate) fn page(records: Vec<RecordData>, card_seen: bool) -> RecordsPageState {
    RecordsPageState {
        ledger: Some(ledger(records)),
        guide: Some(ARCHIVE_GUIDE.to_data(card_seen.then(Utc::now))),
        ..RecordsPageState::default()
    }
}

pub(crate) fn lisbon_answer() -> RecordAnswerData {
    let mut flight = record(
        "rec_tap",
        RecordKindData::Booking,
        "TAP Air Portugal",
        "LHR -> LIS TP1357",
        Some((21_240, "\u{a3}212.40")),
        Some("K7QX2M"),
        at(2025, 1, 20),
    );
    flight.group = Some(RecordGroupData {
        id: "grp_1".into(),
        kind: "trip".into(),
        title: "Lisbon, June 2025".into(),
        count: 3,
        span_start: None,
        span_end: None,
    });
    let message = flight.message_id.clone().expect("message");
    flight.pdf = Some(pdf(&message, "e-ticket-K7QX2M.pdf"));
    RecordAnswerData {
        query: "lisbon booking ref".into(),
        asked: "reference".into(),
        answer: Some(RecordAnswerCardData {
            provenance: Some(field("reference", "Booking ref", "K7QX2M", "schema", true)),
            record: flight,
            field: "reference".into(),
            label: "Booking ref".into(),
            value: "K7QX2M".into(),
            copy: "K7QX2M".into(),
        }),
        also: vec![record(
            "rec_hotel",
            RecordKindData::Booking,
            "Hotel Lisboa Plaza",
            "Lisbon, 3 nights",
            None,
            Some("88213"),
            at(2025, 1, 21),
        )],
        fallback: None,
    }
}

fn render_at(page: &RecordsPageState, width: u16) -> String {
    render_to_string(width, 34, |frame| {
        draw(
            frame,
            Rect::new(0, 0, width, 34),
            &RecordsView {
                page,
                selected_index: 0,
                active_pane: &ActivePane::MailList,
            },
            &crate::theme::Theme::default(),
        );
    })
}

#[test]
fn the_ledger_groups_records_by_month_with_counts_and_totals() {
    let page = page(vec![apple(), dell(), octopus()], true);
    for width in [60u16, 80, 120] {
        let rendered = render_at(&page, width);
        insta::assert_snapshot!(format!("records_lens_ledger_{width}"), rendered);
        assert!(
            rendered.contains("Archive  3 records"),
            "{width}\n{rendered}"
        );
        assert!(rendered.contains("2025 \u{b7} March"), "{width}");
        assert!(rendered.contains("2 records"), "{width}");
        assert!(rendered.contains("\u{a3}1,249.00"), "{width}");
        assert!(rendered.contains("XPS 14"), "{width}");
        // An amount nobody confirmed carries its open mark.
        let apple_row = rendered
            .lines()
            .find(|line| line.contains("iCloud"))
            .unwrap_or_else(|| panic!("{width}: no Apple row\n{rendered}"));
        assert!(apple_row.contains("\u{a3}2.99 \u{b7}?"), "{apple_row}");
        assert!(rendered.contains("Lisbon, June 2025 \u{b7} in 3 days"));
        assert!(!rendered.contains('\u{2014}'), "no em dashes");
    }
    let wide = render_at(&page, 120);
    assert!(wide.contains("Dell"));
    assert!(wide.contains("402-118"));
    assert!(wide.contains(" pdf"));
    assert!(wide.contains("ordered \u{b7} shipped \u{b7} delivered 7 Mar"));
    assert!(wide.contains("[all] receipts orders trips bills docs"));
}

#[test]
fn an_answer_leads_with_the_field_and_where_it_came_from() {
    let mut page = page(vec![dell()], true);
    page.query = "lisbon booking ref".into();
    page.answer = Some(lisbon_answer());
    for width in [60u16, 80, 120] {
        let rendered = render_at(&page, width);
        insta::assert_snapshot!(format!("records_lens_answer_{width}"), rendered);
        assert!(
            rendered.contains("Booking ref  K7QX2M  \u{2713}schema"),
            "{width}\n{rendered}"
        );
        assert!(rendered.contains("Part of trip \"Lisbon, June 2025\" (3)"));
        assert!(rendered.contains("y copy  \u{21b5} pdf  o email"));
    }
    let wide = render_at(&page, 120);
    assert!(wide.contains("\u{2713}schema"));
    assert!(wide.contains("Also matching: Lisbon, 3 nights (88213)"));
}

#[test]
fn the_record_card_shows_every_field_with_its_source_and_an_open_dot_when_unchecked() {
    let mut card = apple();
    card.fields
        .iter_mut()
        .filter(|f| f.field == "amount")
        .for_each(|f| {
            f.checked = false;
            f.source = "rule".into();
            f.source_label = "a pattern in the email".into();
            f.evidence = Some("Total \u{a3}2.99".into());
        });
    card.sources = vec![RecordSourceData {
        message_id: MessageId::new(),
        thread_id: None,
        stage: "receipt".into(),
        date: at(2025, 3, 28),
        subject: "Your receipt from Apple.".into(),
        filed_by: "detector".into(),
    }];
    let mut page = page(vec![apple()], true);
    page.card = Some(card);
    for width in [60u16, 80, 120] {
        let rendered = render_at(&page, width);
        insta::assert_snapshot!(format!("records_lens_card_{width}"), rendered);
        assert!(rendered.contains(" Record "), "{width}");
        assert!(rendered.contains("\u{25cb} Paid"), "{width}\n{rendered}");
    }
    let wide = render_at(&page, 120);
    assert!(
        wide.contains("(a pattern in the email: \"Total \u{a3}2.99\")"),
        "{wide}"
    );
    assert!(wide.contains("From 1 emails"));
    assert!(wide.contains("Your receipt from Apple."));
}

#[test]
fn never_had_any_teaches_the_job_and_the_card_shows_until_seen() {
    let mut empty = page(Vec::new(), false);
    if let Some(ledger) = empty.ledger.as_mut() {
        ledger.empty_state = Some(format!(
            "{} {}",
            ARCHIVE_GUIDE.never_had_any, ARCHIVE_GUIDE.add_one
        ));
        ledger.coming_up.clear();
    }
    for width in [60u16, 80, 120] {
        let rendered = render_at(&empty, width);
        insta::assert_snapshot!(format!("records_lens_empty_{width}"), rendered);
        assert!(rendered.contains("filed here as"), "{width}\n{rendered}");
    }
    // No records yet: the card waits for the first one.
    assert!(!render_at(&empty, 120).contains("Archive keeps records built"));
    let unseen = render_at(&page(vec![dell()], false), 120);
    assert!(
        unseen.contains("Archive keeps records built from your mail"),
        "{unseen}"
    );
    assert!(unseen.contains("/ ask \u{b7} y copy reference \u{b7} Enter open document \u{b7} o the email \u{b7} Esc close"));
    let seen = render_at(&page(vec![dell()], true), 120);
    assert!(!seen.contains("Archive keeps records built"));
}

#[test]
fn mail_text_cannot_reach_the_terminal_as_control_sequences() {
    let mut hostile = dell();
    hostile.title = Some("XPS\u{1b}[2J laptop".into());
    hostile.why = "Here because\u{1b}]0;pwned\u{7}".into();
    let rendered = render_at(&page(vec![hostile], true), 120);
    assert!(
        !rendered
            .chars()
            .any(|c| matches!(c as u32, 0x00..=0x09 | 0x0B..=0x1F | 0x7F..=0x9F)),
        "{rendered:?}"
    );
}
