//! Amounts and currencies in an email's text.
//!
//! Every amount shown appears verbatim in its message: the matched text is
//! kept as evidence. When several different amounts appear and none sits
//! next to a payment word ("amount due", "total", "balance"), no amount is
//! picked, because a wrong amount is worse than none.

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
static PAYMENT_CONTEXT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)(amount\s+(?:due|owed|outstanding|to\s+pay|payable)|total\s+(?:due|amount|to\s+pay|payable)|balance(?:\s+due)?|payment\s+of|minimum\s+payment|you\s+owe|to\s+pay|due\s*:|total\s*:|amount\s*:|outstanding|renewal\s+(?:price|premium|cost)|premium|price)",
    )
    .expect("valid payment-context regex")
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
                    },
                )
            })
        })
        .collect()
}

fn parse_minor(number: &str) -> Option<i64> {
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

/// The amount the email asks for: the one next to a payment word, else the
/// only amount it mentions. `None` when it can't tell.
pub fn pick_amount(text: &str) -> Option<Amount> {
    let amounts = find_amounts(text);
    let in_context = amounts.iter().find(|(start, _)| {
        let from = floor_char_boundary(text, start.saturating_sub(CONTEXT_WINDOW));
        PAYMENT_CONTEXT.is_match(&text[from..*start])
    });
    if let Some((_, amount)) = in_context {
        return Some(amount.clone());
    }
    let first = amounts.first()?;
    amounts
        .iter()
        .all(|(_, amount)| amount.minor == first.1.minor && amount.currency == first.1.currency)
        .then(|| first.1.clone())
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
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
