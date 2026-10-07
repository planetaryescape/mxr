use super::*;
use chrono::TimeZone;
use mxr_core::id::AccountId;

fn record(
    id: &str,
    kind: &str,
    issuer: &str,
    title: &str,
    reference: Option<&str>,
    place: Option<&str>,
    year: i32,
) -> ArchiveRecord {
    let at = Utc
        .with_ymd_and_hms(year, 3, 3, 12, 0, 0)
        .single()
        .expect("time");
    ArchiveRecord {
        id: id.to_string(),
        account_id: AccountId::new(),
        dedup_key: id.to_string(),
        kind: kind.to_string(),
        issuer: Some(issuer.to_string()),
        issuer_key: Some(issuer.to_lowercase()),
        title: Some(title.to_string()),
        reference: reference.map(str::to_string),
        amount_minor: Some(1000),
        currency: Some("GBP".to_string()),
        issued_at: Some(at),
        span_start: None,
        span_end: None,
        place: place.map(str::to_string),
        delivered_at: None,
        return_by: None,
        warranty_until: None,
        valid_until: None,
        checked: true,
        group_id: None,
        origin: "schema".to_string(),
        reason: String::new(),
        thread_id: None,
        last_message_at: Some(at),
        rules_version: 1,
        dismissed_at: None,
        created_at: at,
        updated_at: at,
    }
}

fn ids(query: &str, records: &[ArchiveRecord], groups: &[Option<&str>]) -> Vec<String> {
    let candidates: Vec<Candidate<'_>> = records
        .iter()
        .zip(groups)
        .map(|(record, group)| Candidate {
            record,
            group_title: *group,
            stages: &[],
        })
        .collect();
    rank(&parse(query), &candidates)
        .into_iter()
        .map(|ranked| records[ranked.index].id.clone())
        .collect()
}

#[test]
fn a_query_splits_into_the_field_asked_kinds_dates_and_words() {
    let query = parse("What was my Lisbon booking ref?");
    assert_eq!(query.asked, Asked::Reference);
    assert_eq!(query.terms, vec!["lisbon"]);
    let query = parse("dell receipt march 2025");
    assert_eq!(query.asked, Asked::Any);
    assert!(query.kinds.contains(&RecordKind::Receipt));
    assert_eq!((query.year, query.month), (Some(2025), Some(3)));
    assert_eq!(query.terms, vec!["dell"]);
    assert_eq!(parse("how much was the octopus bill").asked, Asked::Amount);
}

#[test]
fn lisbon_booking_ref_finds_the_flight_by_its_place_and_trip() {
    let records = [
        record(
            "flight",
            "booking",
            "TAP Air Portugal",
            "LHR -> LIS TP1357",
            Some("K7QX2M"),
            Some("Lisbon"),
            2025,
        ),
        record(
            "porto",
            "booking",
            "Airbnb",
            "Porto, 2 nights",
            Some("HMX8"),
            Some("Porto"),
            2025,
        ),
        record(
            "dell",
            "order",
            "Dell",
            "XPS 14 laptop",
            Some("402-118"),
            None,
            2025,
        ),
    ];
    let got = ids(
        "lisbon booking ref",
        &records,
        &[Some("Lisbon, June 2025"), None, None],
    );
    assert_eq!(got, vec!["flight"]);
}

#[test]
fn every_word_must_match_a_field_and_issuers_beat_titles() {
    let records = [
        record(
            "dell",
            "order",
            "Dell",
            "XPS 14 laptop",
            Some("402-118"),
            None,
            2025,
        ),
        record(
            "case",
            "order",
            "Amazon",
            "Dell laptop case",
            Some("204-77"),
            None,
            2025,
        ),
    ];
    assert_eq!(
        ids("dell laptop", &records, &[None, None]),
        vec!["dell", "case"]
    );
    assert!(ids("dell monitor", &records, &[None, None]).is_empty());
    assert_eq!(ids("402-118", &records, &[None, None]), vec!["dell"]);
}

#[test]
fn a_year_and_a_kind_narrow_the_answer() {
    let records = [
        record(
            "old",
            "statement",
            "Octopus Energy",
            "Bill, Feb",
            None,
            None,
            2024,
        ),
        record(
            "new",
            "statement",
            "Octopus Energy",
            "Bill, Feb",
            None,
            None,
            2025,
        ),
        record(
            "tap",
            "booking",
            "Octopus Travel",
            "Hotel",
            Some("X12345"),
            None,
            2025,
        ),
    ];
    assert_eq!(
        ids("octopus bill 2024", &records, &[None, None, None]),
        vec!["old"]
    );
    assert_eq!(
        ids("octopus bill", &records, &[None, None, None]),
        vec!["new", "old"]
    );
    assert!(ids("", &records, &[None, None, None]).is_empty());
}

#[test]
fn the_card_leads_with_what_was_asked_or_the_reference() {
    let dell = record("dell", "order", "Dell", "XPS", Some("402-118"), None, 2025);
    assert_eq!(answer_field(Asked::Amount, &dell), AnswerField::Amount);
    assert_eq!(answer_field(Asked::Any, &dell), AnswerField::Reference);
    let mut no_ref = dell.clone();
    no_ref.reference = None;
    assert_eq!(answer_field(Asked::Reference, &no_ref), AnswerField::Amount);
}
