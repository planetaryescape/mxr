//! Reads one message for the records it holds: schema.org first, then the
//! words, then the user's "always file this sender".

use crate::fields::{FieldName, Found};
use crate::rules::{self, issuer_from_sender, title_from_subject, RuleInput, EMAIL_DATE};
use crate::schema_org::{self, SchemaRecord};
use crate::{RecordKind, Source, Stage};
use chrono::{DateTime, TimeZone, Utc};

/// One message as detection sees it.
#[derive(Debug, Clone, Copy)]
pub struct DetectInput<'a> {
    pub subject: &'a str,
    pub body_text: &'a str,
    pub body_html: Option<&'a str>,
    pub from_name: Option<&'a str>,
    pub from_email: &'a str,
    pub sent: DateTime<Utc>,
    pub list_mail: bool,
}

/// How a record was found, for its why line and its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Schema,
    Rule,
    /// The user said to file everything from this sender.
    Sender,
    /// The user filed it by hand.
    Manual,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Rule => "rule",
            Self::Sender => "sender",
            Self::Manual => "manual",
        }
    }
}

/// One record a message holds, with its fields and their provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub kind: RecordKind,
    pub stage: Stage,
    pub origin: Origin,
    /// The why line's evidence: "order confirmation with schema.org
    /// markup", "\"receipt\" in the subject".
    pub reason: String,
    pub issuer: String,
    pub reference: Option<String>,
    /// An invoice's account, for series.
    pub account_ref: Option<String>,
    pub fields: Vec<Found>,
}

impl Detection {
    pub fn field(&self, name: FieldName) -> Option<&Found> {
        self.fields.iter().find(|found| found.field == name)
    }
}

/// What the user decided about a sender.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SenderView<'a> {
    /// "Always file this sender", with the kind to file as.
    pub always: Option<RecordKind>,
    /// The issuer name the user gave this sender.
    pub issuer_name: Option<&'a str>,
}

/// Every record the message holds. Markup gives one per entity (a flight
/// and a hotel in one email are two bookings); words give at most one.
pub fn detect<Tz>(input: &DetectInput<'_>, tz: &Tz, sender: SenderView<'_>) -> Vec<Detection>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let fallback_issuer = issuer_from_sender(input.from_name, input.from_email);
    let rule_input = RuleInput {
        subject: input.subject,
        body_text: input.body_text,
        from_email: input.from_email,
        sent: input.sent,
        list_mail: input.list_mail,
    };
    let schema = schema_org::read(input.body_html);
    let mut detections: Vec<Detection> = if schema.is_empty() {
        rules::read(&rule_input, tz)
            .map(|read| Detection {
                kind: read.kind,
                stage: read.stage,
                origin: Origin::Rule,
                reason: format!("\"{}\" in the subject", read.phrase),
                issuer: fallback_issuer.clone(),
                reference: read.reference,
                account_ref: read.account_ref,
                fields: read.fields,
            })
            .into_iter()
            .collect()
    } else {
        // Words fill what the markup leaves out, such as a return window or
        // a warranty, on the first record only.
        let words = rules::read(&rule_input, tz);
        schema
            .into_iter()
            .enumerate()
            .map(|(index, record)| {
                let mut detection = from_schema(record, &fallback_issuer, input.sent);
                if index == 0 {
                    if let Some(words) = &words {
                        for found in &words.fields {
                            let fills_gap = matches!(
                                found.field,
                                FieldName::ReturnBy
                                    | FieldName::WarrantyUntil
                                    | FieldName::ValidUntil
                            );
                            if fills_gap && detection.field(found.field).is_none() {
                                detection.fields.push(found.clone());
                            }
                        }
                    }
                }
                detection
            })
            .collect()
    };

    if detections.is_empty() {
        if let Some(kind) = sender.always {
            detections.push(always_filed(kind, input, &rule_input, &fallback_issuer, tz));
        }
    }
    for detection in &mut detections {
        if let Some(name) = sender.issuer_name {
            detection.issuer = name.to_string();
        }
        if detection.field(FieldName::Issuer).is_none() {
            detection.fields.push(Found::text(
                FieldName::Issuer,
                detection_issuer_source(detection, sender),
                detection.issuer.clone(),
            ));
        }
    }
    detections
}

fn detection_issuer_source(detection: &Detection, sender: SenderView<'_>) -> Source {
    if sender.issuer_name.is_some() {
        Source::User
    } else if detection.origin == Origin::Schema {
        Source::Schema
    } else {
        Source::Rule
    }
}

fn from_schema(record: SchemaRecord, fallback_issuer: &str, sent: DateTime<Utc>) -> Detection {
    let mut fields = Vec::new();
    if let Some(issuer) = &record.issuer {
        fields.push(Found::text(
            FieldName::Issuer,
            Source::Schema,
            issuer.clone(),
        ));
    }
    if let Some(title) = &record.title {
        fields.push(Found::text(FieldName::Title, Source::Schema, title.clone()));
    }
    if let Some(reference) = &record.reference {
        fields.push(Found::text(
            FieldName::Reference,
            Source::Schema,
            reference.clone(),
        ));
    }
    if let Some(place) = &record.place {
        fields.push(Found::text(FieldName::Place, Source::Schema, place.clone()));
    }
    if let Some(amount) = &record.amount {
        fields.push(Found::money(
            Source::Schema,
            amount.minor,
            &amount.currency,
            amount.text.clone(),
        ));
    }
    for (field, when) in [
        (FieldName::IssuedAt, &record.issued),
        (FieldName::SpanStart, &record.span_start),
        (FieldName::SpanEnd, &record.span_end),
    ] {
        if let Some(when) = when {
            fields.push(Found::at(field, Source::Schema, when.at, when.text.clone()));
        }
    }
    if record.issued.is_none() {
        // The markup said what, not when: the email's date stands in, and
        // says so.
        fields.push(
            Found::at(
                FieldName::IssuedAt,
                Source::Rule,
                crate::fields::day_at(sent.date_naive()),
                EMAIL_DATE,
            )
            .unchecked(),
        );
    }
    if record.stage == Stage::Delivered {
        fields.push(
            Found::at(
                FieldName::DeliveredAt,
                Source::Rule,
                sent,
                "the delivery email's date",
            )
            .unchecked(),
        );
    }
    let what = match (record.kind, record.stage) {
        (RecordKind::Order, Stage::Shipped) => "shipping email",
        (RecordKind::Order, Stage::Delivered) => "delivery email",
        (RecordKind::Order, _) => "order confirmation",
        (RecordKind::Invoice, Stage::Receipt) => "paid bill",
        (RecordKind::Invoice, _) => "bill",
        (RecordKind::Ticket, _) => "event ticket",
        _ => match record.schema_type.as_str() {
            "FlightReservation" => "flight booking",
            "LodgingReservation" => "hotel booking",
            _ => "booking",
        },
    };
    Detection {
        kind: record.kind,
        stage: record.stage,
        origin: Origin::Schema,
        reason: format!("{what} with schema.org markup"),
        issuer: record
            .issuer
            .clone()
            .unwrap_or_else(|| fallback_issuer.to_string()),
        reference: record.reference,
        account_ref: record.account_ref,
        fields,
    }
}

/// A message from a sender the user files every time, read for what the
/// words give.
fn always_filed<Tz>(
    kind: RecordKind,
    input: &DetectInput<'_>,
    rule_input: &RuleInput<'_>,
    issuer: &str,
    tz: &Tz,
) -> Detection
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let text = format!("{}\n{}", input.subject, input.body_text);
    let reference = rules::find_reference(&text, kind);
    let mut fields = Vec::new();
    if let Some((value, words)) = &reference {
        fields.push(Found {
            evidence: Some(words.clone()),
            ..Found::text(FieldName::Reference, Source::Rule, value.clone())
        });
    }
    if let Some(amount) = rules::find_total(input.body_text) {
        fields.push(Found::money(
            Source::Rule,
            amount.minor,
            &amount.currency,
            amount.text,
        ));
    }
    let day = input.sent.with_timezone(tz).date_naive();
    fields.push(
        Found::at(
            FieldName::IssuedAt,
            Source::Rule,
            crate::fields::day_at(day),
            EMAIL_DATE,
        )
        .unchecked(),
    );
    if let Some(title) = title_from_subject(rule_input.subject) {
        fields.push(Found::text(FieldName::Title, Source::Rule, title));
    }
    Detection {
        kind,
        stage: Stage::Other,
        origin: Origin::Sender,
        reason: "you file everything from this sender".to_string(),
        issuer: issuer.to_string(),
        reference: reference.map(|(value, _)| value),
        account_ref: None,
        fields,
    }
}

/// What a message filed by hand holds: the detector's reading when it has
/// one, else the always-file reading as `kind` (a receipt by default).
pub fn manual<Tz>(
    input: &DetectInput<'_>,
    tz: &Tz,
    sender: SenderView<'_>,
    kind: Option<RecordKind>,
) -> Detection
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let detected = detect(input, tz, sender);
    let mut detection = match detected.into_iter().next() {
        Some(found) => found,
        None => {
            let rule_input = RuleInput {
                subject: input.subject,
                body_text: input.body_text,
                from_email: input.from_email,
                sent: input.sent,
                list_mail: input.list_mail,
            };
            let issuer = sender.issuer_name.map_or_else(
                || issuer_from_sender(input.from_name, input.from_email),
                str::to_string,
            );
            let mut found = always_filed(
                kind.unwrap_or(RecordKind::Receipt),
                input,
                &rule_input,
                &issuer,
                tz,
            );
            found
                .fields
                .push(Found::text(FieldName::Issuer, Source::Rule, issuer));
            found
        }
    };
    if let Some(kind) = kind {
        detection.kind = kind;
    }
    detection.origin = Origin::Manual;
    detection.reason = "you filed it".to_string();
    detection
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn input<'a>(subject: &'a str, body: &'a str, html: Option<&'a str>) -> DetectInput<'a> {
        DetectInput {
            subject,
            body_text: body,
            body_html: html,
            from_name: Some("TAP Air Portugal"),
            from_email: "booking@flytap.com",
            sent: Utc
                .with_ymd_and_hms(2025, 1, 20, 9, 0, 0)
                .single()
                .expect("time"),
            list_mail: false,
        }
    }

    #[test]
    fn markup_gives_one_record_per_entity_and_words_fill_the_gaps() {
        let html = r#"<script type="application/ld+json">[
          {"@type":"FlightReservation","reservationNumber":"K7QX2M",
           "reservationFor":{"@type":"Flight","flightNumber":"1357",
             "airline":{"name":"TAP Air Portugal","iataCode":"TP"},
             "departureAirport":{"iataCode":"LHR"},"arrivalAirport":{"iataCode":"LIS","address":{"addressLocality":"Lisbon"}},
             "departureTime":"2025-06-12T07:40:00+01:00"}},
          {"@type":"LodgingReservation","reservationNumber":"88213",
           "reservationFor":{"name":"Hotel Lisboa Plaza","address":{"addressLocality":"Lisbon"}},
           "checkinTime":"2025-06-12","checkoutTime":"2025-06-15"}]</script>"#;
        let found = detect(
            &input(
                "Your trip",
                "Booking reference: K7QX2M. Valid until 12 June 2025.",
                Some(html),
            ),
            &Utc,
            SenderView::default(),
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].reason, "flight booking with schema.org markup");
        assert_eq!(found[0].reference.as_deref(), Some("K7QX2M"));
        assert!(found[0].field(FieldName::ValidUntil).is_some());
        assert_eq!(found[1].issuer, "Hotel Lisboa Plaza");
        // Markup that gives no booking time stands in the email's date,
        // unchecked.
        assert!(!found[1].field(FieldName::IssuedAt).expect("issued").checked);
    }

    #[test]
    fn always_file_takes_a_sender_the_rules_would_skip_and_names_the_issuer() {
        let sender = SenderView {
            always: Some(RecordKind::Contract),
            issuer_name: Some("Sam Okafor (landlord)"),
        };
        let found = detect(
            &input("Lease for next year", "See attached.", None),
            &Utc,
            sender,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, RecordKind::Contract);
        assert_eq!(found[0].origin, Origin::Sender);
        let issuer = found[0].field(FieldName::Issuer).expect("issuer");
        assert_eq!(issuer.source, Source::User);
        assert!(detect(
            &input("Lease for next year", "See attached.", None),
            &Utc,
            SenderView::default()
        )
        .is_empty());
    }

    #[test]
    fn filing_by_hand_files_what_the_rules_skip() {
        let found = manual(
            &input("Quote for the kitchen", "Total £4,200.00", None),
            &Utc,
            SenderView::default(),
            None,
        );
        assert_eq!(found.kind, RecordKind::Receipt);
        assert_eq!(found.origin, Origin::Manual);
        assert!(found.field(FieldName::Amount).is_some());
        assert!(found.field(FieldName::Issuer).is_some());
    }
}
