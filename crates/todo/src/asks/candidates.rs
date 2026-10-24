use crate::dates::{order_for, resolve_due_match, trigger_regex};
use chrono::{DateTime, TimeZone};
use mxr_core::asks::*;
use once_cell::sync::Lazy;
use regex::Regex;

static DUE: Lazy<Regex> = Lazy::new(|| {
    trigger_regex(
        r"due|pay(?:ment)?\s+by|payable\s+by|no\s+later\s+than|deadline|on\s+or\s+before|by|before|expires|renew\s+by",
    )
});
static RELATIVE_DUE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bwithin\s+(?:[0-9]{1,3}|an?|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve)\s+(?:hours?|days?|weeks?)\b").expect("relative due candidate")
});
static REFERENCE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)\b(?P<kind>account|invoice|policy|order)\b\s*(?:(?P<marker>number|no\.?|reference|ref\.?|\#|:)\s*[:\#]?\s*)?(?P<value>[a-z0-9][a-z0-9/-]{1,40}(?: +[0-9]{2,12})*)\b"
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
        for captures in DUE.captures_iter(&source.text) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            if let Some(date) = resolve_due_match(
                &source.text,
                &captures,
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
        for matched in RELATIVE_DUE.find_iter(&source.text) {
            if let Some(at) = mxr_core::natural_time::due_words(
                &source.text,
                matched.as_str(),
                &anchor,
                &mxr_core::natural_time::TimePrefs::default(),
            )
            .and_then(|d| d.unambiguous_at())
            {
                facts.dates.push(AskDate {
                    zone,
                    span: AskSpan {
                        start: matched.start(),
                        end: matched.end(),
                    },
                    at,
                    checked: true,
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
            if captures.name("marker").is_none() && !normalized.chars().any(|c| c.is_ascii_digit())
            {
                continue;
            }
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

pub(super) fn has_due(text: &str) -> bool {
    DUE.is_match(text) || RELATIVE_DUE.is_match(text)
}
