//! Structured fields supplement one authored bill; they never independently judge the email.
use super::{candidates::inside, sender_auth, votes, AskInput};
use chrono::{TimeZone, Utc};
use mxr_core::asks::*;

pub fn include_html<Tz>(
    answer: &mut AskAssessment,
    input: &AskInput<'_>,
    html: Option<&str>,
    headers: &[String],
    trusted: &[String],
    tz: &Tz,
) where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let Some(html) = html else {
        return;
    };
    let zone = answer.facts.zones.len();
    answer.facts.zones.push(AskZone {
        kind: ZoneKind::Unattributed,
        author: Some(input.from.email.clone()),
        text: html.into(),
    });
    answer.html = Some(AskHtmlEvidence {
        zone,
        alignment: None,
    });
    let Some((header, authserv_id, method, domain)) =
        sender_auth::aligned(headers, trusted, &input.from.email)
    else {
        return;
    };
    let header_zone = answer.facts.zones.len();
    answer.facts.zones.push(AskZone {
        kind: ZoneKind::Unattributed,
        author: None,
        text: header.into(),
    });
    answer.html = Some(AskHtmlEvidence {
        zone,
        alignment: Some(AskSenderAlignment {
            authserv_id,
            method: method.into(),
            domain,
            header_zone,
        }),
    });
    let schema = crate::schema_org::read(Some(html));
    if schema.ambiguous {
        return;
    }
    let (
        Some(crate::schema_org::SchemaTodo::Bill {
            amount: Some(amount),
            due: Some(due),
            due_text: Some(words),
            ..
        }),
        Some(source),
    ) = (schema.todo, schema.source)
    else {
        return;
    };
    if amount.minor <= 0 {
        return;
    }
    let candidates: Vec<_> = answer
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| {
            answer.facts.zones[unit.zone].kind == ZoneKind::Authored
                && unit.document.kind == DocumentKind::Bill
                && matches!(
                    unit.document.status,
                    PaymentStatus::Due | PaymentStatus::PastDue
                )
                && !unit.document.required_fields_present
        })
        .map(|(index, _)| index)
        .collect();
    let [index] = candidates.as_slice() else {
        return;
    };
    let index = *index;
    let raw = &html[source.clone()];
    let mut occurrences = raw.match_indices(&words);
    let Some((offset, _)) = occurrences.next() else {
        return;
    };
    if occurrences.next().is_some() {
        return;
    }
    let at = match due {
        crate::schema_org::SchemaDate::Day(day) => crate::dates::end_of_day(day, tz),
        crate::schema_org::SchemaDate::At(at) => at,
    };
    let due = answer.facts.dates.len();
    answer.facts.dates.push(AskDate {
        zone,
        span: AskSpan {
            start: source.start + offset,
            end: source.start + offset + words.len(),
        },
        at: at.with_timezone(&Utc),
        checked: true,
    });
    let unit = &answer.units[index];
    let amounts: Vec<_> = answer
        .facts
        .amounts
        .iter()
        .enumerate()
        .filter(|(_, a)| inside(unit.zone, &unit.span, a.zone, &a.span))
        .collect();
    if amounts
        .iter()
        .any(|(_, a)| a.minor != amount.minor || a.currency != amount.currency)
    {
        return;
    }
    let amount = if let Some((index, _)) = amounts.first() {
        *index
    } else {
        let mut occurrences = raw.match_indices(&amount.text);
        let Some((offset, _)) = occurrences.next() else {
            return;
        };
        if occurrences.next().is_some() {
            return;
        }
        let index = answer.facts.amounts.len();
        answer.facts.amounts.push(AskAmount {
            zone,
            span: AskSpan {
                start: source.start + offset,
                end: source.start + offset + amount.text.len(),
            },
            minor: amount.minor,
            currency: amount.currency,
        });
        index
    };
    let source_text = &answer.facts.zones[unit.zone].text;
    let mut document = unit.document.clone();
    document.required_fields_present = true;
    let mut readings = votes::label(
        &source_text[unit.span.start..unit.span.end],
        &source_text[..unit.span.start],
        unit.zone,
        &unit.span,
        &answer.facts,
        &document,
        input,
    );
    for vote in &mut readings {
        if vote.tags.action == ActionKind::Pay {
            vote.tags.due = Some(due);
            vote.tags.amount = Some(amount);
            vote.rule = format!("authenticated_schema_{}", vote.rule);
        }
    }
    let tags = readings
        .first()
        .map(|v| v.tags.clone())
        .filter(|first| readings.iter().all(|v| &v.tags == first));
    let resolved = super::resolve(tags.as_ref(), input.owner_replied_afterwards);
    let unit = &mut answer.units[index];
    unit.document = document;
    unit.votes = readings;
    unit.tags = tags;
    unit.answer = resolved;
    if resolved == AskAnswer::AsksYou {
        for unit in &mut answer.units {
            if answer.facts.zones[unit.zone].kind == ZoneKind::Subject
                && unit.document.kind == DocumentKind::Bill
                && !answer.facts.references.iter().any(|r| r.zone == unit.zone)
            {
                unit.answer = AskAnswer::Nothing;
            }
        }
    }
}
