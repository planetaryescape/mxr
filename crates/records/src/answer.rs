//! The answer box: a few remembered words ("lisbon booking ref", "dell
//! receipt 2025") matched against record fields, with no model.
//!
//! A query splits into what it asks for (a reference, an amount, a date, a
//! document), kinds it names ("booking", "receipt", "bills"), a year or
//! month, and the remaining words. Every remaining word has to match a
//! field of the record (issuer, title, place, reference, its trip or
//! series), so an answer is never a loose guess; when no record matches
//! every word, the caller falls back to `mxr ask` and says so.

use crate::{RecordKind, Stage};
use chrono::{DateTime, Datelike, Utc};
use mxr_store::ArchiveRecord;

/// What the query asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    Reference,
    Amount,
    Date,
    Document,
    /// Nothing in particular: the card leads with the reference, else the
    /// amount.
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub asked: Asked,
    pub kinds: Vec<RecordKind>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    /// Lowercased words that must each match a field.
    pub terms: Vec<String>,
}

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "my", "for", "of", "in", "on", "at", "to", "from", "with", "what", "whats",
    "what's", "was", "is", "it", "me", "show", "find", "get", "give", "last", "i", "did", "do",
    "and", "or", "that", "this", "which", "where", "our", "your",
];

/// Words that name the field asked for, longest phrases first.
const ASKED_WORDS: &[(&str, Asked)] = &[
    ("booking ref", Asked::Reference),
    ("booking reference", Asked::Reference),
    ("order number", Asked::Reference),
    ("confirmation number", Asked::Reference),
    ("how much", Asked::Amount),
    ("reference", Asked::Reference),
    ("ref", Asked::Reference),
    ("number", Asked::Reference),
    ("code", Asked::Reference),
    ("confirmation", Asked::Reference),
    ("pnr", Asked::Reference),
    ("amount", Asked::Amount),
    ("total", Asked::Amount),
    ("cost", Asked::Amount),
    ("price", Asked::Amount),
    ("paid", Asked::Amount),
    ("spent", Asked::Amount),
    ("date", Asked::Date),
    ("when", Asked::Date),
    ("pdf", Asked::Document),
    ("document", Asked::Document),
    ("copy", Asked::Document),
];

/// Words that name kinds of record.
fn kind_words(word: &str) -> Option<&'static [RecordKind]> {
    use RecordKind as K;
    Some(match word {
        "receipt" | "receipts" => &[K::Receipt, K::Order],
        "order" | "orders" | "purchase" | "purchases" => &[K::Order, K::Receipt],
        "booking" | "bookings" | "reservation" | "reservations" | "flight" | "flights"
        | "hotel" | "hotels" | "trip" | "trips" => &[K::Booking, K::Ticket],
        "ticket" | "tickets" => &[K::Ticket, K::Booking],
        "invoice" | "invoices" => &[K::Invoice],
        "bill" | "bills" => &[K::Invoice, K::Statement],
        "statement" | "statements" => &[K::Statement],
        "contract" | "contracts" | "agreement" | "lease" => &[K::Contract],
        "warranty" | "warranties" | "guarantee" => &[K::Warranty, K::Order],
        "account" | "policy" => &[K::Account],
        _ => return None,
    })
}

fn month_number(word: &str) -> Option<u32> {
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    if word.len() < 3 {
        return None;
    }
    let full = [
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
    ];
    months
        .iter()
        .zip(full.iter())
        .position(|(short, long)| word == *short || (long.starts_with(word) && word.len() >= 3))
        .map(|index| u32::try_from(index).unwrap_or(0) + 1)
}

pub fn parse(text: &str) -> Query {
    let mut lower = format!(
        " {} ",
        text.to_lowercase().replace(['?', ',', '"', '\''], " ")
    );
    let mut asked = Asked::Any;
    for (phrase, field) in ASKED_WORDS {
        let padded = format!(" {phrase} ");
        if lower.contains(&padded) {
            if asked == Asked::Any {
                asked = *field;
            }
            lower = lower.replace(&padded, " ");
        }
    }
    let mut kinds: Vec<RecordKind> = Vec::new();
    let mut year = None;
    let mut month = None;
    let mut terms = Vec::new();
    for word in lower.split_whitespace() {
        if STOPWORDS.contains(&word) {
            continue;
        }
        if let Some(found) = kind_words(word) {
            for kind in found {
                if !kinds.contains(kind) {
                    kinds.push(*kind);
                }
            }
            continue;
        }
        if word.len() == 4 && word.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(value) = word.parse::<i32>() {
                if (1990..=2100).contains(&value) {
                    year = Some(value);
                    continue;
                }
            }
        }
        if let Some(value) = month_number(word) {
            month = Some(value);
            continue;
        }
        terms.extend(tokens(word));
    }
    Query {
        asked,
        kinds,
        year,
        month,
        terms,
    }
}

/// A record and the words around it that a query may match.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub record: &'a ArchiveRecord,
    /// The trip or series it belongs to.
    pub group_title: Option<&'a str>,
    /// The stages its emails reached, for "delivered" and "refund".
    pub stages: &'a [Stage],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub index: usize,
    pub score: f32,
}

fn tokens(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// How well `term` matches `haystack`'s tokens: exact, or a prefix of at
/// least three letters ("lis" for Lisbon).
fn matches(term: &str, haystack: &[String]) -> bool {
    haystack
        .iter()
        .any(|token| token == term || (term.len() >= 3 && token.starts_with(term)))
}

/// Every candidate that matches every term, best first. Score sums each
/// term's best field (reference 5, issuer 3, place 2.5, group 2, title 2),
/// plus 1.5 for a named kind, which also filters.
pub fn rank(query: &Query, candidates: &[Candidate<'_>]) -> Vec<Ranked> {
    let mut ranked: Vec<(Ranked, (Option<DateTime<Utc>>, Option<DateTime<Utc>>))> = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let record = candidate.record;
        let date = record.ledger_date();
        if let Some(year) = query.year {
            if date.is_none_or(|d| d.year() != year)
                && record.span_start.is_none_or(|d| d.year() != year)
            {
                continue;
            }
        }
        if let Some(month) = query.month {
            if date.is_none_or(|d| d.month() != month)
                && record.span_start.is_none_or(|d| d.month() != month)
            {
                continue;
            }
        }
        let kind = RecordKind::parse(&record.kind);
        let kind_hit = kind.is_some_and(|kind| query.kinds.contains(&kind));
        // A named kind is a filter: "octopus bill" is never Octopus Travel's
        // hotel.
        if !query.kinds.is_empty() && !kind_hit {
            continue;
        }
        if query.terms.is_empty() && !kind_hit && query.year.is_none() && query.month.is_none() {
            continue;
        }
        let fields: [(f32, Vec<String>); 5] = [
            (
                5.0,
                record.reference.as_deref().map(tokens).unwrap_or_default(),
            ),
            (
                3.0,
                record.issuer.as_deref().map(tokens).unwrap_or_default(),
            ),
            (2.5, record.place.as_deref().map(tokens).unwrap_or_default()),
            (2.0, candidate.group_title.map(tokens).unwrap_or_default()),
            (2.0, record.title.as_deref().map(tokens).unwrap_or_default()),
        ];
        let stage_words: Vec<String> = candidate
            .stages
            .iter()
            .map(|stage| stage.word().to_string())
            .collect();
        let mut score = 0.0;
        let mut all = true;
        for term in &query.terms {
            let best = fields
                .iter()
                .filter(|(_, haystack)| matches(term, haystack))
                .map(|(weight, _)| *weight)
                .fold(0.0_f32, f32::max);
            let best = if best == 0.0 && matches(term, &stage_words) {
                1.0
            } else {
                best
            };
            if best == 0.0 {
                all = false;
                break;
            }
            score += best;
        }
        if !all {
            continue;
        }
        if kind_hit {
            score += 1.5;
        }
        if query.asked == Asked::Reference && record.reference.is_some() {
            score += 0.5;
        }
        if query.asked == Asked::Amount && record.amount_minor.is_some() {
            score += 0.5;
        }
        ranked.push((Ranked { index, score }, (record.span_start, date)));
    }
    // Ties go to what starts first (a trip's flight before its hotel: what
    // the desk asks for first), then to the newest record.
    ranked.sort_by(|a, b| {
        b.0.score
            .partial_cmp(&a.0.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| match (a.1 .0, b.1 .0) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| b.1 .1.cmp(&a.1 .1))
    });
    ranked.into_iter().map(|(ranked, _)| ranked).collect()
}

/// Which field the answer card leads with.
pub fn answer_field(asked: Asked, record: &ArchiveRecord) -> AnswerField {
    match asked {
        Asked::Reference if record.reference.is_some() => AnswerField::Reference,
        Asked::Amount if record.amount_minor.is_some() => AnswerField::Amount,
        Asked::Date => AnswerField::Date,
        Asked::Document => AnswerField::Document,
        _ if record.reference.is_some() => AnswerField::Reference,
        _ if record.amount_minor.is_some() => AnswerField::Amount,
        _ => AnswerField::Date,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerField {
    Reference,
    Amount,
    Date,
    Document,
}

impl AnswerField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Amount => "amount",
            Self::Date => "issued_at",
            Self::Document => "document",
        }
    }
}

#[cfg(test)]
mod tests;
