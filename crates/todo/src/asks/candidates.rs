use crate::dates::{find_due, order_for, trigger_regex};
use chrono::{DateTime, TimeZone};
use mxr_core::asks::*;
use once_cell::sync::Lazy;
use regex::Regex;

static DUE: Lazy<Regex> = Lazy::new(|| {
    trigger_regex(
        r"due|pay(?:ment)?\s+by|payable\s+by|no\s+later\s+than|deadline|on\s+or\s+before|by|before|expires|renew\s+by",
    )
});
static REFERENCE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)\b(?P<kind>account|invoice|policy|order)\s*(?:number|no\.?|reference|ref\.?|\#|:)\s*[:\#]?\s*(?P<value>[a-z0-9][a-z0-9/-]{1,40})\b"
).expect("reference regex")
});

pub(super) fn extract<Tz>(facts: &mut AskFacts, anchor: DateTime<Tz>, email: &str)
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    for (zone, source) in facts.zones.iter().enumerate() {
        for (start, amount) in crate::money::find_candidates(&source.text) {
            facts.amounts.push(AskAmount {
                zone,
                span: AskSpan {
                    start,
                    end: start + amount.text.len(),
                },
                minor: amount.minor,
                currency: amount.currency,
            });
        }
        for matched in DUE.find_iter(&source.text) {
            if let Some(date) = find_due(
                matched.as_str(),
                &DUE,
                &anchor,
                order_for(
                    &crate::action_link::email_domain(email).unwrap_or_default(),
                    facts
                        .amounts
                        .iter()
                        .find(|a| a.zone == zone)
                        .map(|a| a.currency.as_str()),
                ),
            ) {
                facts.dates.push(AskDate {
                    zone,
                    span: AskSpan {
                        start: matched.start(),
                        end: matched.end(),
                    },
                    at: date.at,
                    checked: date.checked,
                });
            }
        }
        for captures in REFERENCE.captures_iter(&source.text) {
            let (Some(whole), Some(kind), Some(value)) = (
                captures.get(0),
                captures.name("kind"),
                captures.name("value"),
            ) else {
                continue;
            };
            let normalized = value.as_str().to_ascii_lowercase();
            if [
                "amount", "due", "total", "payment", "your", "our", "the", "this", "none",
            ]
            .contains(&normalized.as_str())
            {
                continue;
            }
            facts.references.push(AskReference {
                zone,
                span: AskSpan {
                    start: whole.start(),
                    end: whole.end(),
                },
                kind: kind.as_str().to_ascii_lowercase(),
                value: normalized,
            });
        }
    }
    // R-prefixed reference values are not rand prices. Keep the single
    // parser family, then use the explicit reference facts to disambiguate.
    let references = &facts.references;
    facts.amounts.retain(|amount| {
        !references
            .iter()
            .any(|reference| inside(reference.zone, &reference.span, amount.zone, &amount.span))
    });
}

pub(super) fn inside(
    zone: usize,
    span: &AskSpan,
    candidate_zone: usize,
    candidate: &AskSpan,
) -> bool {
    zone == candidate_zone && candidate.start >= span.start && candidate.end <= span.end
}
