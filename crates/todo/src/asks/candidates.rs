use crate::dates::{order_for, resolve_due_match, trigger_regex};
use chrono::{DateTime, TimeZone};
use mxr_core::asks::*;
use once_cell::sync::Lazy;
use regex::Regex;
use std::ops::Range;

static DUE: Lazy<Regex> = Lazy::new(|| {
    trigger_regex(
        r"due|pay(?:ment)?\s+by|payable\s+by|no\s+later\s+than|deadline|on\s+or\s+before|by|before|expires|renew\s+by",
    )
});
static ALTERNATIVE_DATE: Lazy<Regex> = Lazy::new(|| trigger_regex("or"));
static RELATIVE_DUE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bwithin\s+(?:[0-9]{1,3}|an?|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve)\s+(?:hours?|days?|weeks?)\b").expect("relative due candidate")
});
static REFERENCE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
    r"(?i)\b(?P<kind>(?:account|invoice|policy|order)\b|(?:(?:payment|customer|beneficiary|your)\s+)?ref(?:erence)?\b\.?)\s*(?:(?P<marker>number|no\.?|reference|ref\.?|\#|:)\s*[:\#]?\s*)?(?P<value>[a-z0-9](?:[a-z0-9/-]|\.[a-z0-9]){1,40}(?: +[0-9]{2,12})*)\b"
).expect("reference regex")
});
/// A line that opens the biller's own banking details, which run to the next
/// blank line.
static BANK_DETAILS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?im)^[ \t]*(?:(?:banking|bank|payment)\s+(?:details|information|info)\b|pay(?:ments?)?\s+(?:in)?to\b|(?:bank|branch(?:\s+code)?|sort\s+code|beneficiary|payee|account\s+(?:holder|name)|swift|iban)\b)[^\n]*$",
    )
    .expect("bank details regex")
});
static BANK_WORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:bank|branch|sort\s+code|iban|swift|beneficiary|payee|bacs|eft|deposit(?:ed)?|into)\b")
        .expect("bank word regex")
});
static CENTS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.,][0-9]{2}$").expect("cents regex"));

/// The kind and value a reference match names. Every kind but a bare reference
/// may trust its label; "reference: please" is prose, so that kind needs a digit.
fn reference_parts(captures: &regex::Captures<'_>) -> Option<(String, String)> {
    let kind = captures.name("kind")?.as_str().to_ascii_lowercase();
    let kind = if kind.contains("ref") {
        "reference".to_string()
    } else {
        kind
    };
    let value = captures.name("value")?.as_str().to_ascii_lowercase();
    let labelled = captures.name("marker").is_some() && kind != "reference";
    (labelled || value.chars().any(|c| c.is_ascii_digit())).then_some((kind, value))
}

/// Byte ranges of the banking-details blocks in `text`.
fn bank_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    for opening in BANK_DETAILS.find_iter(text) {
        if blocks.last().is_some_and(|(_, end)| opening.start() < *end) {
            continue;
        }
        // Mail may end lines with CRLF, and a "blank" line may hold spaces.
        let rest = &text[opening.end()..];
        let mut line_start = opening.end() + rest.find('\n').map_or(rest.len(), |at| at + 1);
        let mut end = text.len();
        for line in text[line_start..].split_inclusive('\n') {
            if line.trim().is_empty() {
                end = line_start;
                break;
            }
            line_start += line.len();
        }
        blocks.push((opening.start(), end));
    }
    blocks
}

/// An account number where the owner is told to pay is the biller's. Every
/// customer of that biller shares it, so it cannot identify one bill.
fn is_billers_account(text: &str, blocks: &[(usize, usize)], label: usize) -> bool {
    let line = text[..label].rfind('\n').map_or(0, |at| at + 1);
    blocks
        .iter()
        .any(|(start, end)| *start <= label && label < *end)
        || BANK_WORD.is_match(&text[line..label])
}

/// "Invoice R953.63" states a price. Only a whole number with no cents, like
/// "Payment reference: R123456", is an identifier.
fn reads_as_money(text: &str, value: Range<usize>, amount: Range<usize>) -> bool {
    amount.start < value.start || amount.end > value.end || CENTS.is_match(&text[amount])
}

fn states_amount(text: &str) -> bool {
    crate::money::find_candidates(text)
        .into_iter()
        .any(|(start, amount)| {
            let span = start..start + amount.text.len();
            !REFERENCE
                .captures_iter(text)
                .filter_map(|captures| captures.name("value"))
                .any(|value| {
                    span.start < value.end()
                        && value.start() < span.end
                        && !reads_as_money(text, value.range(), span.clone())
                })
        })
}

/// A second amount or deadline means `line` is another bill, not more detail
/// about the bill in `before`.
pub(super) fn restates_billing_terms(before: &str, line: &str) -> bool {
    states_amount(before) && states_amount(line) || has_due(before) && has_due(line)
}

pub(super) fn billing_fields_conflict(before: &str, after: &str) -> bool {
    let amounts = |text: &str| {
        crate::money::find_candidates(text)
            .into_iter()
            .map(|(_, amount)| (amount.currency, amount.minor))
            .collect::<Vec<_>>()
    };
    let references = |text: &str| {
        REFERENCE
            .captures_iter(text)
            .filter_map(|c| reference_parts(&c))
            .collect::<Vec<_>>()
    };
    let dates = |text: &str| {
        DUE.captures_iter(text)
            .filter_map(|c| c.name("date").map(|d| d.as_str().to_ascii_lowercase()))
            .collect::<Vec<_>>()
    };
    fn different<T: PartialEq>(before: Vec<T>, after: Vec<T>) -> bool {
        !before.is_empty() && after.iter().any(|value| !before.contains(value))
    }
    different(amounts(before), amounts(after))
        || different(references(before), references(after))
        || different(dates(before), dates(after))
}

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
        // Enumerate an alternative only inside the same unit as an existing
        // deadline. It is evidence of ambiguity, not another independent ask.
        let units = super::facts::unit_spans(&source.text);
        for captures in ALTERNATIVE_DATE.captures_iter(&source.text) {
            let Some(matched) = captures.get(0) else {
                continue;
            };
            let Some(unit) = units
                .iter()
                .find(|unit| matched.start() >= unit.start && matched.end() <= unit.end)
            else {
                continue;
            };
            if !facts.dates.iter().any(|date| {
                inside(zone, unit, date.zone, &date.span) && date.span.end <= matched.start()
            }) {
                continue;
            }
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
        let blocks = bank_blocks(&source.text);
        for captures in REFERENCE.captures_iter(&source.text) {
            let (Some(whole), Some(value), Some((kind, normalized))) = (
                captures.get(0),
                captures.name("value"),
                reference_parts(&captures),
            ) else {
                continue;
            };
            if kind == "account" && is_billers_account(&source.text, &blocks, whole.start()) {
                continue;
            }
            let is_money = facts.amounts.iter().any(|amount| {
                let span = amount.span.start..amount.span.end;
                amount.zone == zone
                    && span.start < value.end()
                    && value.start() < span.end
                    && reads_as_money(&source.text, value.range(), span)
            });
            if is_money {
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
                kind,
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
