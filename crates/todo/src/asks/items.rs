//! Item fields are projections of stored tags, never another detector.
use crate::dates::FoundDate;
use crate::detect::{Detection, Origin};
use crate::money::Amount;
use crate::provenance::{FieldProvenance, FieldSource, FieldSources};
use crate::TodoKind;
use mxr_core::asks::*;
use mxr_core::Envelope;

pub fn detections(answer: &AskAssessment, envelope: &Envelope) -> Vec<(usize, Detection)> {
    if answer.pending_body {
        return Vec::new();
    }
    answer
        .units
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| {
            if !matches!(unit.answer, AskAnswer::AsksYou | AskAnswer::YouPromised) {
                return None;
            }
            let tags = unit.tags.as_ref()?;
            let quote = answer.quote(unit).or_else(|| {
                (answer.facts.zones.get(unit.zone)?.kind == ZoneKind::Calendar).then_some("")
            })?;
            let promised = unit.answer == AskAnswer::YouPromised;
            let kind = if promised {
                TodoKind::Promise
            } else {
                match tags.action {
                    ActionKind::Pay => TodoKind::Bill,
                    ActionKind::FixPayment => TodoKind::PaymentFailed,
                    ActionKind::Sign => TodoKind::Sign,
                    ActionKind::Verify => TodoKind::Verify,
                    ActionKind::RenewOrCancel => TodoKind::Renewal,
                    ActionKind::Attend => TodoKind::Rsvp,
                    ActionKind::Reply if is_rsvp(answer, unit) => TodoKind::Rsvp,
                    _ => TodoKind::Other,
                }
            };
            let verb = match tags.action {
                ActionKind::Pay => "pay",
                ActionKind::FixPayment => "fix",
                ActionKind::Sign => "sign",
                ActionKind::Verify => "verify",
                ActionKind::Reply => "reply",
                ActionKind::Send => "send",
                ActionKind::Provide => "provide",
                ActionKind::Upload => "upload",
                ActionKind::Book => "book",
                ActionKind::Attend => "attend",
                ActionKind::RenewOrCancel => "renew",
                ActionKind::Register => "register",
                ActionKind::Contact => "contact",
                ActionKind::Other => "review",
            };
            let counterparty = if promised {
                envelope.to.iter().find(|a| {
                    !answer
                        .facts
                        .owner_addresses
                        .iter()
                        .any(|own| own.eq_ignore_ascii_case(&a.email))
                })
            } else {
                Some(&envelope.from)
            }?;
            let person = crate::detect::counterparty(counterparty.name.as_deref(), None)
                .unwrap_or_else(|| counterparty.email.clone());
            let amount = tags
                .amount
                .and_then(|i| answer.facts.amounts.get(i))
                .map(|a| Amount {
                    minor: a.minor,
                    currency: a.currency.clone(),
                    text: answer.facts.zones[a.zone].text[a.span.start..a.span.end].to_string(),
                    checked: true,
                });
            let due = tags
                .due
                .and_then(|i| answer.facts.dates.get(i))
                .map(|d| FoundDate {
                    at: d.at,
                    words: answer.facts.zones[d.zone].text[d.span.start..d.span.end].to_string(),
                    checked: d.checked,
                });
            let mut fields = FieldSources::default();
            for key in ["kind", "title", "quote"] {
                fields.set(
                    key,
                    FieldProvenance::with_evidence(FieldSource::Rule, quote),
                );
            }
            fields.set(
                "requester",
                FieldProvenance::with_evidence(
                    FieldSource::Rule,
                    if promised {
                        format!("you promised {person}")
                    } else {
                        format!("{person} asked")
                    },
                ),
            );
            fields.set(
                "asks_fingerprint",
                FieldProvenance::with_evidence(FieldSource::Rule, &answer.input_fingerprint),
            );
            fields.set(
                "asks_provenance",
                FieldProvenance::with_evidence(
                    FieldSource::Rule,
                    unit_provenance(answer, index, unit),
                ),
            );
            fields.set(
                "counterparty",
                FieldProvenance::with_evidence(FieldSource::Rule, &counterparty.email),
            );
            if let Some(a) = &amount {
                fields.set(
                    "amount",
                    FieldProvenance::with_evidence(FieldSource::Rule, &a.text),
                );
            }
            if let Some(d) = &due {
                fields.set(
                    "due_at",
                    FieldProvenance::with_evidence(FieldSource::Rule, &d.words),
                );
            }
            let object = tags
                .reference
                .and_then(|i| answer.facts.references.get(i))
                .map_or_else(
                    || quote.to_lowercase(),
                    |r| format!("{}:{}", r.kind, r.value.to_lowercase()),
                );
            // Full counterparty address prevents unrelated accounts on one provider
            // domain from being merged. Explicit references survive reminder wording.
            let action_identity = if matches!(tags.action, ActionKind::Pay | ActionKind::FixPayment)
            {
                "payment".into()
            } else {
                format!("{:?}", tags.action)
            };
            let object_key = format!(
                "{}|{action_identity}|{object}",
                counterparty.email.to_lowercase()
            );
            if answer.facts.zones[unit.zone].kind == ZoneKind::Calendar {
                let calendar = answer.calendar.as_ref()?;
                let organizer = calendar
                    .organizer
                    .as_ref()
                    .map(|p| p.name.as_deref().unwrap_or(&p.email));
                let mut detection = crate::detect::from_invite(
                    calendar.summary.as_deref(),
                    organizer,
                    &envelope.from.email,
                    calendar
                        .starts_at
                        .as_deref()
                        .and_then(crate::pass::parse_ics_time),
                );
                detection.object_key = format!(
                    "{}|calendar|{}|{}",
                    envelope.from.email.to_lowercase(),
                    calendar.uid.clone().unwrap_or_else(|| envelope.id.as_str()),
                    calendar.recurrence_id.as_deref().unwrap_or_default()
                );
                if !quote.is_empty() {
                    detection.reason = quote.to_string();
                }
                for key in ["quote", "requester", "asks_fingerprint", "asks_provenance"] {
                    if let Some(value) = fields.0.get(key) {
                        if key == "quote" && quote.is_empty() {
                            continue;
                        }
                        let mut value = value.clone();
                        value.source = FieldSource::Ics;
                        detection.fields.set(key, value);
                    }
                }
                return Some((index, detection));
            }
            Some((
                index,
                Detection {
                    kind,
                    verb: verb.into(),
                    doc_type: None,
                    title: super::titles::title(
                        if promised {
                            ActionKind::Other
                        } else {
                            tags.action
                        },
                        quote,
                        &envelope.subject,
                        &person,
                    ),
                    object_key,
                    counterparty: Some(person),
                    sender_domain: crate::action_link::email_domain(&counterparty.email),
                    amount,
                    due,
                    event_start: None,
                    link: crate::action_link::pick_link(
                        None,
                        &answer.facts.zones[unit.zone].text,
                        kind,
                    ),
                    origin: Origin::Rule,
                    reason: quote.to_string(),
                    fields,
                },
            ))
        })
        .collect()
}

/// The retained RSVP product category asks for a response, not an attendance promise.
pub fn is_rsvp(answer: &AskAssessment, unit: &AskUnit) -> bool {
    unit.tags
        .as_ref()
        .is_some_and(|t| t.action == ActionKind::Reply)
        && (answer.facts.zones[unit.zone].kind == ZoneKind::Calendar
            || unit.votes.iter().any(|v| v.rule == "authored_rsvp"))
}

pub fn unit_provenance(answer: &AskAssessment, index: usize, unit: &AskUnit) -> String {
    format!(
        "{}:{}:unit:{index}:zone:{}:{}..{}",
        answer.policy_version, answer.message_id, unit.zone, unit.span.start, unit.span.end
    )
}
