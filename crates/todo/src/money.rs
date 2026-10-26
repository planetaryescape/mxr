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

// A comma or full stop can be the decimal separator. Grouped forms
// consume their entire numeric span, so a prefix cannot become a smaller price.
const NUMBER: &str = r"[+-]?(?:\d{1,3}(?:,\d{3})+(?:\.\d{1,2})?|\d{1,3}(?:\.\d{3})+,\d{1,2}|\d{1,3}(?:[\x20\x{00a0}\x{202f}]\d{3})+(?:[.,]\d{1,2})?|\d+(?:[.,]\d{1,2})?)";
static NUMBER_FORMAT: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!("^(?:{NUMBER})$")).expect("amount number grammar"));
static AMOUNT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?x)
        (?P<sign1>[+-]?)(?P<sym>[£$€]|\bR)\s?(?P<n1>{NUMBER})\b
        | (?P<sign2>[+-]?)\b(?P<code1>{CODES})\s?(?P<n2>{NUMBER})\b
        | (?P<n3>{NUMBER})\s?(?P<code2>{CODES})\b
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
    find_candidates(text)
        .into_iter()
        .filter(|(_, amount)| amount.minor > 0)
        .collect()
}

/// Signed and zero candidates are facts even when they cannot be money owed.
/// The legacy positive-only selector and the asks judge share this parser.
pub fn find_candidates(text: &str) -> Vec<(usize, Amount)> {
    AMOUNT
        .captures_iter(text)
        .filter_map(|caps| {
            let whole = caps.get(0)?;
            // A shorter regex alternative must not turn malformed money
            // such as £1.234 or £1,23 into a valid £1 candidate.
            let tail = text.get(whole.end()..)?;
            if tail.starts_with(['.', ','])
                && tail.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
            {
                return None;
            }
            if tail.starts_with([' ', '\u{00a0}', '\u{202f}'])
                && tail
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                return None;
            }
            let (currency, number) = if let Some(sym) = caps.name("sym") {
                let code = match sym.as_str() {
                    "£" => "GBP",
                    "$" => "USD",
                    "R" => "ZAR",
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
            if matches!(currency.as_str(), "GBP" | "USD" | "CAD" | "AUD" | "NZD")
                && decimal_separator(number).is_some_and(|at| number.as_bytes()[at] == b',')
            {
                return None;
            }
            let mut minor = parse_minor(number)?;
            let prefix = caps.name("sign1").or_else(|| caps.name("sign2"));
            if let Some(sign) = prefix.filter(|sign| !sign.as_str().is_empty()) {
                if number.starts_with(['-', '+']) {
                    return None;
                }
                if sign.as_str() == "-" {
                    minor = minor.checked_neg()?;
                }
            }
            Some({
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
    if !NUMBER_FORMAT.is_match(number) {
        return None;
    }
    let negative = number.starts_with('-');
    let unsigned = number.strip_prefix(['-', '+']).unwrap_or(number);
    let compact = unsigned.replace([' ', '\u{00a0}', '\u{202f}'], "");
    let decimal = decimal_separator(&compact);
    let (whole, frac) = decimal.map_or((compact.as_str(), ""), |at| {
        (&compact[..at], &compact[at + 1..])
    });
    let whole = whole.replace(['.', ','], "");
    let whole: i64 = whole.parse().ok()?;
    let frac: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        2 => frac.parse().ok()?,
        _ => return None,
    };
    let minor = whole.checked_mul(100)?.checked_add(frac)?;
    if negative {
        minor.checked_neg()
    } else {
        Some(minor)
    }
}

fn decimal_separator(number: &str) -> Option<usize> {
    match (number.rfind('.'), number.rfind(',')) {
        (Some(dot), Some(comma)) => Some(dot.max(comma)),
        (Some(dot), None) => Some(dot),
        (None, Some(comma)) if number.len() - comma - 1 <= 2 => Some(comma),
        _ => None,
    }
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

    #[test]
    fn signed_and_zero_candidates_share_the_positive_selector_parser() {
        let text = "-£12.50 £-0.25 €0.00 R1,234.50 +USD 20";
        let candidates = find_candidates(text);
        assert_eq!(
            candidates
                .iter()
                .map(|(_, a)| (a.minor, a.currency.as_str()))
                .collect::<Vec<_>>(),
            [
                (-1250, "GBP"),
                (-25, "GBP"),
                (0, "EUR"),
                (123450, "ZAR"),
                (2000, "USD")
            ]
        );
        assert_eq!(
            find_amounts(text)
                .iter()
                .map(|(_, a)| a.minor)
                .collect::<Vec<_>>(),
            [123450, 2000]
        );
        assert_eq!(parse_minor("-1.25"), Some(-125));
        assert_eq!(parse_minor("-0.25"), Some(-25));
        assert_eq!(parse_minor("1.234"), None);
        assert_eq!(parse_minor("9223372036854775807.00"), None);
    }

    #[test]
    fn malformed_decimal_and_thousands_do_not_become_partial_candidates() {
        for text in ["£1.234", "£1,23", "R2D2", "-£-12", "USD -+12"] {
            assert!(find_candidates(text).is_empty(), "{text}");
        }
    }
    #[test]
    fn european_and_rand_formats_keep_their_full_span_and_signed_value() {
        let text = "€1.234,56 R1 234,56 R450,00 -EUR 0,25 €0,00 £1,234.56";
        let candidates = find_candidates(text);
        assert_eq!(
            candidates
                .iter()
                .map(|(_, a)| (a.text.as_str(), a.minor, a.currency.as_str()))
                .collect::<Vec<_>>(),
            [
                ("€1.234,56", 123456, "EUR"),
                ("R1 234,56", 123456, "ZAR"),
                ("R450,00", 45000, "ZAR"),
                ("-EUR 0,25", -25, "EUR"),
                ("€0,00", 0, "EUR"),
                ("£1,234.56", 123456, "GBP")
            ]
        );
        for (start, a) in &candidates {
            assert_eq!(&text[*start..start + a.text.len()], a.text);
        }
        assert_eq!(find_amounts(text).len(), 4);
        for malformed in ["R1 23,45", "€1,23,45", "£1,23", "€1.2345"] {
            assert!(find_candidates(malformed).is_empty(), "{malformed}");
        }
    }
}
