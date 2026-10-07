//! Amounts and currencies in an email's text.
//!
//! Every amount shown appears verbatim in its message: the matched text is
//! kept as evidence. Candidates are ranked by the words just before them:
//! "amount due" and "to pay" beat "total" and "premium", which beat a bare
//! amount; "previous balance" or "last paid" never count. When the best
//! rank still holds different amounts, the first is kept but unchecked.
//! When several different unlabelled amounts appear, none is picked,
//! because a wrong amount is worse than none.

use crate::text::floor_char_boundary;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Amount {
    /// In the currency's minor unit (pence, cents).
    pub minor: i64,
    /// ISO 4217 code.
    pub currency: String,
    /// The text it was read from, verbatim.
    pub text: String,
    /// False when another amount in the mail was as likely to be the one.
    pub checked: bool,
}

impl Amount {
    /// "£142.00", "$15,000.00", "EUR 12.50".
    pub fn display(&self) -> String {
        format_amount(self.minor, &self.currency)
    }
}

/// "£142.00" from minor units and a code.
pub fn format_amount(minor: i64, currency: &str) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let minor = minor.unsigned_abs();
    let whole = group_thousands(minor / 100);
    let cents = minor % 100;
    match symbol(currency) {
        Some(symbol) => format!("{sign}{symbol}{whole}.{cents:02}"),
        None => format!("{sign}{currency} {whole}.{cents:02}"),
    }
}

/// "1249.00" from minor units: no symbol and no grouping, for
/// spreadsheets and the clipboard.
pub fn plain_amount(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let minor = minor.unsigned_abs();
    format!("{sign}{}.{:02}", minor / 100, minor % 100)
}

fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn symbol(currency: &str) -> Option<&'static str> {
    match currency {
        "GBP" => Some("£"),
        "USD" => Some("$"),
        "EUR" => Some("€"),
        _ => None,
    }
}

const CODES: &str = "GBP|USD|EUR|ZAR|CAD|AUD|NZD|CHF|SEK|NOK|DKK|INR|JPY";

static AMOUNT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?x)
        (?P<sym>[£$€])\s?(?P<n1>\d{{1,3}}(?:,\d{{3}})+(?:\.\d{{1,2}})?|\d+(?:\.\d{{1,2}})?)
        | \b(?P<code1>{CODES})\s?(?P<n2>\d{{1,3}}(?:,\d{{3}})+(?:\.\d{{1,2}})?|\d+(?:\.\d{{1,2}})?)\b
        | \b(?P<n3>\d{{1,3}}(?:,\d{{3}})+(?:\.\d{{1,2}})?|\d+(?:\.\d{{1,2}})?)\s?(?P<code2>{CODES})\b
        "
    ))
    .expect("valid amount regex")
});

/// Words that put an amount next to what is owed.
/// What is owed now: the strongest label.
static OWED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(amount\s+(?:due|owed|outstanding|to\s+pay|payable)|total\s+(?:due|to\s+pay|payable)|balance\s+(?:due|to\s+pay|outstanding)|payment\s+(?:of|due)|minimum\s+payment|you\s+owe|to\s+pay|due\s*:|outstanding)",
    )
    .expect("valid owed regex")
});
/// A price or total that may be what is owed.
static PRICED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(total|amount\s*:|renewal\s+(?:price|premium|cost)|premium|price|balance)")
        .expect("valid priced regex")
});
/// Words that make an amount history, not what is owed now.
static PAST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(previous|last\s+(?:month|bill|statement|payment)|paid|credit|refund(?:ed)?|was|saved?)\b",
    )
    .expect("valid past regex")
});

/// How far before an amount a payment word may sit and still count.
const CONTEXT_WINDOW: usize = 60;

/// Every amount in `text`, in order.
pub fn find_amounts(text: &str) -> Vec<(usize, Amount)> {
    AMOUNT
        .captures_iter(text)
        .filter_map(|caps| {
            let whole = caps.get(0)?;
            let (currency, number) = if let Some(sym) = caps.name("sym") {
                let code = match sym.as_str() {
                    "£" => "GBP",
                    "$" => "USD",
                    _ => "EUR",
                };
                (code.to_string(), caps.name("n1")?.as_str())
            } else if let Some(code) = caps.name("code1") {
                (code.as_str().to_string(), caps.name("n2")?.as_str())
            } else {
                (
                    caps.name("code2")?.as_str().to_string(),
                    caps.name("n3")?.as_str(),
                )
            };
            let minor = parse_minor(number)?;
            // Zero is a "£0.00 to pay" summary line, never what is owed.
            (minor > 0).then(|| {
                (
                    whole.start(),
                    Amount {
                        minor,
                        currency,
                        text: whole.as_str().to_string(),
                        checked: true,
                    },
                )
            })
        })
        .collect()
}

/// "1,249.00", "1249.5", "1249" to minor units, without floating point.
pub fn parse_minor(number: &str) -> Option<i64> {
    let cleaned = number.replace(',', "");
    let (whole, frac) = cleaned.split_once('.').unwrap_or((&cleaned, ""));
    let whole: i64 = whole.parse().ok()?;
    let frac: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.get(..2)?.parse().ok()?,
    };
    whole.checked_mul(100)?.checked_add(frac)
}

/// The amount the email asks for, ranked by the words before each
/// candidate. `None` when it can't tell.
pub fn pick_amount(text: &str) -> Option<Amount> {
    let mut ranked: Vec<(u8, Amount)> = find_amounts(text)
        .into_iter()
        .filter_map(|(start, amount)| {
            let from = floor_char_boundary(text, start.saturating_sub(CONTEXT_WINDOW));
            // The label closest to the amount decides: "Previous balance
            // £100. Amount due £240" reads "amount due" before £240.
            let before = text[from..start].rsplit(['.', '\n']).next().unwrap_or("");
            let rank = if PAST.is_match(before) {
                0
            } else if OWED.is_match(before) {
                3
            } else if PRICED.is_match(before) {
                2
            } else {
                1
            };
            (rank > 0).then_some((rank, amount))
        })
        .collect();
    let best = ranked.iter().map(|(rank, _)| *rank).max()?;
    ranked.retain(|(rank, _)| *rank == best);
    let (_, mut first) = ranked.first()?.clone();
    let differs = ranked
        .iter()
        .any(|(_, other)| other.minor != first.minor || other.currency != first.currency);
    if differs {
        // Unlabelled amounts that disagree are noise; labelled ones that
        // disagree are a guess to confirm.
        if best == 1 {
            return None;
        }
        first.checked = false;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_symbols_codes_and_thousands() {
        let amounts = find_amounts("Pay £142.00 or $15,000 or 12.5 EUR or GBP 7");
        let got: Vec<_> = amounts
            .iter()
            .map(|(_, amount)| (amount.minor, amount.currency.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (14200, "GBP"),
                (1_500_000, "USD"),
                (1250, "EUR"),
                (700, "GBP")
            ]
        );
    }

    #[test]
    fn picks_the_amount_next_to_a_payment_word() {
        let amount = pick_amount("Last month you paid £30.00. Amount due: £142.00 by 9 October.")
            .expect("amount");
        assert_eq!(amount.minor, 14200);
        assert_eq!(amount.text, "£142.00");
    }

    #[test]
    fn what_is_owed_beats_a_previous_balance() {
        let amount = pick_amount("Previous balance £100.00. Amount due £240.00 by 9 October.")
            .expect("amount");
        assert_eq!(amount.minor, 24000);
        assert!(amount.checked);
    }

    #[test]
    fn unpaid_is_not_paid() {
        let amount = pick_amount("Unpaid amount due £100.00 by 9 October.").expect("amount");
        assert_eq!(amount.minor, 10000);
        assert!(amount.checked);
    }

    #[test]
    fn two_amounts_owed_are_a_guess() {
        let amount = pick_amount("Minimum payment £25.00. Amount due £400.00.").expect("amount");
        assert_eq!(amount.minor, 2500);
        assert!(!amount.checked, "two plausible amounts");
    }

    #[test]
    fn several_unlabelled_amounts_pick_none() {
        assert_eq!(pick_amount("Item one £5.00, item two £7.00"), None);
        assert_eq!(
            pick_amount("Your plan costs £9.99 a month").map(|amount| amount.minor),
            Some(999)
        );
    }

    #[test]
    fn displays_in_the_currency() {
        assert_eq!(format_amount(14200, "GBP"), "£142.00");
        assert_eq!(format_amount(1_500_000, "USD"), "$15,000.00");
        assert_eq!(format_amount(1250, "ZAR"), "ZAR 12.50");
    }
}
