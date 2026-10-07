//! Records from the words of an email, for mail without markup.
//!
//! Precision over recall: a wrong record in a tax export is worse than a
//! missing one. A message files only when its subject names a kind of
//! record and the email gives a reference or an amount to go with it.
//! Marketing ("20% off", "complete your order") never files. Every value
//! shown is quoted from the email (`fields::verbatim`), and a rule's money
//! and dates are unchecked until the user confirms them.

use crate::fields::{day_at, verbatim, FieldName, Found};
use crate::{RecordKind, Source, Stage};
use chrono::{DateTime, Datelike, Months, NaiveDate, TimeZone, Utc};
use mxr_todo::dates::{order_for, parse_date, YearIntent, DATE_EXPR};
use mxr_todo::money::find_amounts;
use once_cell::sync::Lazy;
use regex::Regex;

/// The evidence of an issued date that fell back to the email's own date.
pub const EMAIL_DATE: &str = "the email's date";

/// What the words of one email say about its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRead {
    pub kind: RecordKind,
    pub stage: Stage,
    /// The subject phrase that named the kind, for the why line.
    pub phrase: String,
    pub reference: Option<String>,
    pub fields: Vec<Found>,
}

/// One message as the rules see it.
#[derive(Debug, Clone, Copy)]
pub struct RuleInput<'a> {
    pub subject: &'a str,
    pub body_text: &'a str,
    pub from_email: &'a str,
    pub sent: DateTime<Utc>,
    /// Mail from a list (List-Id). A list must give a reference and an
    /// amount to file, since newsletters talk about orders and tickets.
    pub list_mail: bool,
}

struct KindRule {
    kind: RecordKind,
    stage: Stage,
    pattern: Regex,
}

fn rule(kind: RecordKind, stage: Stage, pattern: &str) -> KindRule {
    KindRule {
        kind,
        stage,
        pattern: Regex::new(&format!("(?i){pattern}")).expect("valid kind rule"),
    }
}

/// Subject phrases, in priority order: the first match decides.
static KIND_RULES: Lazy<Vec<KindRule>> = Lazy::new(|| {
    use RecordKind as K;
    vec![
        rule(K::Order, Stage::Refund, r"\brefund(?:ed)?\b"),
        rule(
            K::Order,
            Stage::Return,
            r"\breturn (?:received|confirmed|label|request)|\byour return\b",
        ),
        rule(
            K::Order,
            Stage::Delivered,
            r"\b(?:has been|was|been) delivered\b|\bdelivered:|\byour (?:order|parcel|package) (?:has )?arrived\b",
        ),
        rule(
            K::Order,
            Stage::Shipped,
            r"\bhas (?:been )?(?:shipped|dispatched)\b|\b(?:shipped|dispatched):|\bis on (?:its|the) way\b|\bout for delivery\b",
        ),
        rule(
            K::Booking,
            Stage::Cancellation,
            r"\b(?:booking|reservation) (?:cancelled|canceled)\b",
        ),
        rule(
            K::Booking,
            Stage::Booking,
            r"\b(?:booking|reservation) (?:confirmation|confirmed|details)\b|\byour (?:booking|reservation|stay|trip|flight|itinerary)\b|\bitinerary\b|\be-?ticket\b|\bboarding pass\b|\bcheck-in (?:is )?open\b",
        ),
        rule(
            K::Ticket,
            Stage::Booking,
            r"\byour tickets?\b|\btickets? for\b|\btickets? confirmed\b",
        ),
        rule(
            K::Order,
            Stage::Confirmation,
            r"\border (?:confirmation|confirmed|received|placed)\b|\b(?:thanks|thank you) for (?:your )?(?:order|purchase)\b|\byour [\w' ]{0,30}order\b|\border #|\border no\b",
        ),
        rule(K::Invoice, Stage::Invoice, r"\binvoice\b"),
        rule(
            K::Receipt,
            Stage::Receipt,
            r"\breceipt\b|\bpayment (?:received|confirmation|confirmed|successful)\b|\b(?:thanks|thank you) for your payment\b|\byou(?:'ve)? paid\b|\byour payment\b",
        ),
        rule(
            K::Statement,
            Stage::Statement,
            r"\bstatement\b|\byour (?:new |latest |monthly )?bill\b|\bbill is (?:ready|available)\b",
        ),
        rule(
            K::Contract,
            Stage::Other,
            r"\bcontract\b|\bagreement\b|^completed:|\bsigned\b|\btenancy\b|\blease\b",
        ),
        rule(
            K::Warranty,
            Stage::Other,
            r"\bwarranty\b|\bguarantee (?:registration|certificate)\b|\bproduct registration\b",
        ),
        rule(
            K::Account,
            Stage::Other,
            r"\baccount (?:created|opened|number|details)\b|\byour new account\b|\bpolicy (?:documents|schedule)\b",
        ),
    ]
});

/// Marketing and nudges that use record words without being records.
static NOT_A_RECORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\d+\s?% off|\bsale\b|\bdeals?\b|\boffers?\b|\bcomplete your (?:order|purchase|booking)\b|\bin your (?:cart|basket)\b|\bforgot something\b|\brate your\b|\breview your\b|\bhow was\b|\bwebinar\b|\bnewsletter\b|\blast chance\b|\bdon'?t miss\b|\bsave \d|\bfree shipping\b|\bdiscount\b|\bwin\b|\bsurvey\b",
    )
    .expect("valid marketing regex")
});

/// A labelled reference: the label words, then the value. Labels are
/// matched in any case; the value must look like an identifier.
static REFERENCE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?P<label>(?i:order|booking|confirmation|reservation|invoice|receipt|document|reference|ref|account|policy|ticket|transaction|customer|membership|itinerary|pnr))\.?\s*(?:(?i:number|no\.?|num|nr|code|id|ref(?:erence)?)\s*)?(?:(?i:is)\s*)?[:#\-]?\s*#?\s*(?P<value>[A-Z0-9](?:[A-Za-z0-9/\-]*\d[A-Za-z0-9/\-]*|[A-Z0-9]{4,7}))\b",
    )
    .expect("valid reference regex")
});

static HAS_DIGIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d").expect("valid digit regex"));
static UPPER_CODE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Z0-9]{5,8}$").expect("valid code regex"));

/// Label words a total sits behind, strongest first.
static TOTAL_STRONG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(grand total|order total|total paid|amount paid|total charged|amount charged|you paid|payment of|total amount|total due|amount due|balance due|total to pay|total \(inc|total incl|total:)",
    )
    .expect("valid total regex")
});
static TOTAL_WEAK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(total|amount|price|cost|fare|charge)\b").expect("valid weak total regex")
});
/// Lines that hold an amount that is not the total.
static NOT_TOTAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(sub-?total|tax|vat|shipping|delivery|postage|discount|saving|saved|previous|balance brought|credit limit|minimum payment)\b")
        .expect("valid not-total regex")
});

static ISSUED_TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:order date|order placed|ordered on|date of purchase|purchase date|invoice date|date issued|issue date|issued on|transaction date|payment date|booking date|booked on|statement date)(?:\s*(?:on|is|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid issued trigger")
});
static RETURN_TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:return by|return before|returns? accepted until|return (?:it )?(?:until|by)|return window (?:closes|ends)(?: on)?|eligible for return until)(?:\s*(?:on|is|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid return trigger")
});
static WARRANTY_TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\bwarranty (?:valid )?(?:until|expires(?: on)?|ends(?: on)?|to)(?:\s*(?:on|is|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid warranty trigger")
});
static WARRANTY_YEARS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?P<n>\d{1,2}|one|two|three|five)[- ]year(?:s)?(?: limited| manufacturer'?s?)? (?:warranty|guarantee)\b")
        .expect("valid warranty years regex")
});
static VALID_TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:valid until|valid through|valid to|expires on|expiry date|expiration date)(?:\s*(?:on|is|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid validity trigger")
});
static SPAN_START_TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:check-in|check in|arrival|departs?|departure|pick-?up)(?: date)?(?:\s*(?:on|is|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid span start trigger")
});
static SPAN_END_TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:check-out|check out|checkout|drop-?off)(?: date)?(?:\s*(?:on|is|:|-))*\s*(?P<date>{})",
        *DATE_EXPR
    ))
    .expect("valid span end trigger")
});

/// Boilerplate stripped from a subject to make a title.
static TITLE_NOISE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:re|fwd?|fw):\s*|\b(?:your|thanks for your|thank you for your)\b|\b(?:order|booking|reservation) (?:confirmation|confirmed|details)\b|\b(?:has been|has|was) (?:shipped|dispatched|delivered)\b|\breceipt (?:for|from)\b|#\s*[A-Za-z0-9\-]+|\b(?:no\.?|number)\s*[A-Za-z0-9\-]+|[!:|]+")
        .expect("valid title noise regex")
});

/// Reads one message. `None` when it isn't a record by these rules.
pub fn read<Tz>(input: &RuleInput<'_>, tz: &Tz) -> Option<RuleRead>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let subject = input.subject.trim();
    if NOT_A_RECORD.is_match(subject) {
        return None;
    }
    let (kind_rule, phrase) = KIND_RULES.iter().find_map(|rule| {
        rule.pattern
            .find(subject)
            .map(|found| (rule, found.as_str().trim().to_lowercase()))
    })?;
    let text = format!("{subject}\n{}", input.body_text);
    let reference = find_reference(&text, kind_rule.kind);
    let amount = find_total(input.body_text);
    // A kind word with nothing to file is a notification, not a record.
    let enough = match kind_rule.kind {
        RecordKind::Order | RecordKind::Booking | RecordKind::Ticket => reference.is_some(),
        RecordKind::Account => reference.is_some(),
        _ => reference.is_some() || amount.is_some(),
    };
    if !enough || (input.list_mail && (reference.is_none() || amount.is_none())) {
        return None;
    }

    let mut fields = Vec::new();
    if let Some((value, words)) = &reference {
        fields.push(Found {
            evidence: Some(words.clone()),
            ..Found::text(FieldName::Reference, Source::Rule, value.clone())
        });
    }
    if let Some(amount) = amount {
        fields.push(Found::money(
            Source::Rule,
            amount.minor,
            &amount.currency,
            amount.text,
        ));
    }
    let anchor = input.sent.with_timezone(tz);
    let domain = input
        .from_email
        .rsplit_once('@')
        .map_or("", |(_, domain)| domain)
        .to_ascii_lowercase();
    let order = order_for(&domain, None);
    let date_at = |re: &Regex, intent: YearIntent| -> Option<(DateTime<Utc>, String)> {
        re.captures_iter(&text).find_map(|caps| {
            let date = caps.name("date")?.as_str();
            let day = match intent {
                // The time resolver reads ahead only, so a date already
                // gone is read here.
                YearIntent::JustGone => numeric_day(date, &anchor, order)
                    .or_else(|| past_day(date, anchor.date_naive()))?,
                YearIntent::Next => parse_date(date, &anchor, order, intent)?.0,
            };
            Some((day_at(day), caps.get(0)?.as_str().trim().to_string()))
        })
    };
    let issued = date_at(&ISSUED_TRIGGER, YearIntent::JustGone);
    match &issued {
        Some((at, words)) => fields.push(Found::at(
            FieldName::IssuedAt,
            Source::Rule,
            *at,
            words.clone(),
        )),
        None => fields.push(
            Found::at(
                FieldName::IssuedAt,
                Source::Rule,
                day_at(anchor.date_naive()),
                EMAIL_DATE,
            )
            .unchecked(),
        ),
    }
    for (re, field) in [
        (&*RETURN_TRIGGER, FieldName::ReturnBy),
        (&*WARRANTY_TRIGGER, FieldName::WarrantyUntil),
        (&*VALID_TRIGGER, FieldName::ValidUntil),
        (&*SPAN_START_TRIGGER, FieldName::SpanStart),
        (&*SPAN_END_TRIGGER, FieldName::SpanEnd),
    ] {
        if let Some((at, words)) = date_at(re, YearIntent::Next) {
            fields.push(Found::at(field, Source::Rule, at, words));
        }
    }
    if !fields.iter().any(|f| f.field == FieldName::WarrantyUntil) {
        if let Some(found) =
            warranty_from_years(&text, issued.as_ref().map_or(input.sent, |(at, _)| *at))
        {
            fields.push(found);
        }
    }
    if let Some(title) = title_from_subject(subject) {
        fields.push(Found::text(FieldName::Title, Source::Rule, title));
    }
    // Every value must be quotable from the email; a parse that drifted
    // from its words is dropped rather than shown.
    fields.retain(|found| {
        let quoted = matches!(found.field, FieldName::Reference) || found.field.needs_checking();
        !quoted
            || found
                .evidence
                .as_deref()
                .is_none_or(|evidence| evidence == EMAIL_DATE || verbatim(&text, evidence))
    });
    Some(RuleRead {
        kind: kind_rule.kind,
        stage: kind_rule.stage,
        phrase,
        reference: reference.map(|(value, _)| value),
        fields,
    })
}

/// The labelled reference that fits the kind best: an order number on an
/// order, a booking reference on a booking. Returns the value and the words
/// it was read from.
pub fn find_reference(text: &str, kind: RecordKind) -> Option<(String, String)> {
    let preferred: &[&str] = match kind {
        RecordKind::Order | RecordKind::Receipt => &[
            "order",
            "receipt",
            "transaction",
            "document",
            "confirmation",
        ],
        RecordKind::Booking | RecordKind::Ticket => &[
            "booking",
            "confirmation",
            "reservation",
            "pnr",
            "itinerary",
            "ticket",
            "reference",
            "ref",
        ],
        RecordKind::Invoice => &["invoice", "reference", "ref", "account"],
        RecordKind::Statement | RecordKind::Account => {
            &["account", "customer", "policy", "reference", "ref"]
        }
        RecordKind::Contract | RecordKind::Warranty => {
            &["reference", "ref", "policy", "order", "account"]
        }
    };
    let mut best: Option<(usize, String, String)> = None;
    for caps in REFERENCE.captures_iter(text) {
        let (Some(label), Some(value)) = (caps.name("label"), caps.name("value")) else {
            continue;
        };
        let value = value.as_str().trim_end_matches(['-', '/', '.']);
        if !(HAS_DIGIT.is_match(value) || UPPER_CODE.is_match(value)) || is_common_word(value) {
            continue;
        }
        let label = label.as_str().to_ascii_lowercase();
        let rank = preferred
            .iter()
            .position(|p| *p == label)
            .unwrap_or(preferred.len());
        let words = caps.get(0).map_or("", |m| m.as_str()).trim().to_string();
        if best
            .as_ref()
            .is_none_or(|(best_rank, _, _)| rank < *best_rank)
        {
            best = Some((rank, value.to_string(), words));
        }
    }
    best.map(|(_, value, words)| (value, words))
}

fn is_common_word(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "NUMBER" | "DETAILS" | "STATUS" | "CONFIRMED" | "SUMMARY" | "UPDATE" | "HTTPS" | "HTTP"
    )
}

/// The total an email states, by the label just before it. A rule's total
/// is always unchecked; this only picks which number to show.
pub fn find_total(text: &str) -> Option<mxr_todo::money::Amount> {
    let mut ranked: Vec<(u8, mxr_todo::money::Amount)> = Vec::new();
    for (start, amount) in find_amounts(text) {
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let from =
            mxr_todo::text::floor_char_boundary(text, start.saturating_sub(48)).max(line_start);
        let before = &text[from..start];
        if NOT_TOTAL.is_match(before) {
            continue;
        }
        let rank = if TOTAL_STRONG.is_match(before) {
            3
        } else if TOTAL_WEAK.is_match(before) {
            2
        } else {
            1
        };
        ranked.push((rank, amount));
    }
    let best = ranked.iter().map(|(rank, _)| *rank).max()?;
    ranked.retain(|(rank, _)| *rank == best);
    let differs = ranked.iter().any(|(_, other)| {
        other.minor != ranked[0].1.minor || other.currency != ranked[0].1.currency
    });
    if differs && best == 1 {
        // Several unlabelled amounts: none of them is the total.
        return None;
    }
    // Among labelled totals the largest is the one that includes the rest.
    ranked
        .into_iter()
        .map(|(_, amount)| amount)
        .max_by_key(|amount| amount.minor)
        .map(|mut amount| {
            amount.checked = false;
            amount
        })
}

fn warranty_from_years(text: &str, from: DateTime<Utc>) -> Option<Found> {
    let caps = WARRANTY_YEARS.captures(text)?;
    let years: u32 = match caps.name("n")?.as_str().to_ascii_lowercase().as_str() {
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "five" => 5,
        digits => digits.parse().ok()?,
    };
    if years == 0 || years > 25 {
        return None;
    }
    let until = from
        .date_naive()
        .checked_add_months(Months::new(years * 12))?;
    Some(
        Found::at(
            FieldName::WarrantyUntil,
            Source::Rule,
            day_at(until),
            caps.get(0)?.as_str(),
        )
        .unchecked(),
    )
}

/// A subject without its boilerplate, or `None` when nothing is left.
pub fn title_from_subject(subject: &str) -> Option<String> {
    let cleaned = TITLE_NOISE.replace_all(subject, " ");
    let cleaned = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c == '-' || c == ',' || c == '.' || c.is_whitespace())
        .to_string();
    if cleaned.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return None;
    }
    let mut title: String = cleaned.chars().take(80).collect();
    if let Some(first) = title.get(..1) {
        let upper = first.to_uppercase();
        title.replace_range(..1, &upper);
    }
    Some(title)
}

/// The issuer from the sender: the display name without "via", "no-reply"
/// and quotes, else the sending domain's name ("dell.com" is "Dell").
pub fn issuer_from_sender(from_name: Option<&str>, from_email: &str) -> String {
    let name = from_name
        .map(|name| {
            let name = name.trim().trim_matches('"').trim();
            let name = name.split(" via ").next().unwrap_or(name);
            name.split(['|', '<'])
                .next()
                .unwrap_or(name)
                .trim()
                .trim_matches('"')
                .trim()
                .to_string()
        })
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            !name.is_empty()
                && !name.contains('@')
                && ![
                    "no-reply",
                    "noreply",
                    "no reply",
                    "do not reply",
                    "donotreply",
                    "notifications",
                    "info",
                    "orders",
                    "billing",
                    "support",
                    "receipts",
                ]
                .contains(&lower.as_str())
        });
    if let Some(name) = name {
        return name;
    }
    let domain = from_email
        .rsplit_once('@')
        .map_or(from_email, |(_, domain)| domain)
        .to_ascii_lowercase();
    let registrable = psl::domain_str(&domain).unwrap_or(&domain);
    let label = registrable.split('.').next().unwrap_or(registrable);
    let mut chars = label.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

static WORDED_DAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:[a-z]+,?\s+)?(?:(?P<d1>\d{1,2})(?:st|nd|rd|th)?\s+(?:of\s+)?(?P<m1>[a-z]+)\.?|(?P<m2>[a-z]+)\.?\s+(?P<d2>\d{1,2})(?:st|nd|rd|th)?)(?:,?\s+(?P<y>\d{4}))?$",
    )
    .expect("valid worded day regex")
});

/// A numeric or ISO date, which the shared parser reads without the time
/// resolver.
fn numeric_day<Tz>(
    expr: &str,
    anchor: &DateTime<Tz>,
    order: mxr_todo::dates::DateOrder,
) -> Option<NaiveDate>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let expr = expr.trim();
    if !expr.starts_with(|c: char| c.is_ascii_digit()) || !expr.contains(['/', '-', '.']) {
        return None;
    }
    if expr.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    parse_date(expr, anchor, order, YearIntent::JustGone).map(|(day, _)| day)
}

/// A worded day on or before `anchor` ("3 March 2025", "Mar 1st", "Friday
/// 28 February"). A yearless day after the anchor is last year's.
pub fn past_day(expr: &str, anchor: NaiveDate) -> Option<NaiveDate> {
    let expr = expr.trim();
    if let Ok(day) = NaiveDate::parse_from_str(expr, "%Y-%m-%d") {
        return Some(day);
    }
    let caps = WORDED_DAY.captures(expr)?;
    let (day, month) = match (
        caps.name("d1"),
        caps.name("m1"),
        caps.name("m2"),
        caps.name("d2"),
    ) {
        (Some(d), Some(m), _, _) | (_, _, Some(m), Some(d)) => (d.as_str(), m.as_str()),
        _ => return None,
    };
    let month = month_of(month)?;
    let day: u32 = day.parse().ok()?;
    match caps.name("y") {
        Some(year) => NaiveDate::from_ymd_opt(year.as_str().parse().ok()?, month, day),
        None => {
            let this_year = NaiveDate::from_ymd_opt(anchor.year(), month, day)?;
            if this_year > anchor {
                NaiveDate::from_ymd_opt(anchor.year() - 1, month, day)
            } else {
                Some(this_year)
            }
        }
    }
}

fn month_of(word: &str) -> Option<u32> {
    let word = word.to_ascii_lowercase();
    if word.len() < 3 {
        return None;
    }
    [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    .iter()
    .position(|name| {
        name.starts_with(&word[..3])
            && (word.len() == 3 || name.starts_with(&word) || word == "sept")
    })
    .and_then(|index| u32::try_from(index + 1).ok())
}

/// The year a date falls in, for ledger grouping.
pub fn year_of(at: DateTime<Utc>) -> i32 {
    at.year()
}

#[cfg(test)]
mod tests;
