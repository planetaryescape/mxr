use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

const SYMBOLS: &[(&str, &str)] = &[
    ("US$", "USD"),
    ("CA$", "CAD"),
    ("C$", "CAD"),
    ("AU$", "AUD"),
    ("A$", "AUD"),
    ("NZ$", "NZD"),
    ("R$", "BRL"),
    ("$", "USD"),
    ("£", "GBP"),
    ("€", "EUR"),
    ("¥", "JPY"),
    ("₹", "INR"),
    ("₩", "KRW"),
    ("₦", "NGN"),
    ("₽", "RUB"),
    ("₺", "TRY"),
    ("₪", "ILS"),
    ("฿", "THB"),
];

const CODES: &[&str] = &[
    "USD", "EUR", "GBP", "JPY", "CNY", "INR", "ZAR", "CAD", "AUD", "NZD", "CHF", "SEK", "NOK",
    "DKK", "PLN", "BRL", "MXN", "KRW", "NGN", "RUB", "TRY", "ILS", "THB", "AED", "SGD", "HKD",
];

const NUMBER_WORDS: &[(&str, &str)] = &[
    ("two", "2"),
    ("three", "3"),
    ("four", "4"),
    ("five", "5"),
    ("six", "6"),
    ("seven", "7"),
    ("eight", "8"),
    ("nine", "9"),
    ("ten", "10"),
    ("eleven", "11"),
    ("twelve", "12"),
    ("twenty", "20"),
    ("thirty", "30"),
    ("forty", "40"),
    ("fifty", "50"),
    ("sixty", "60"),
    ("seventy", "70"),
    ("eighty", "80"),
    ("ninety", "90"),
    ("hundred", "100"),
    ("thousand", "1000"),
    ("million", "1000000"),
    ("billion", "1000000000"),
];

static FIGURE: Lazy<Regex> = Lazy::new(|| {
    let symbols = SYMBOLS
        .iter()
        .map(|(symbol, _)| regex::escape(symbol))
        .collect::<Vec<_>>()
        .join("|");
    let mark = format!(r"(?:{symbols}|\b(?:{})\b)", CODES.join("|"));
    Regex::new(&format!(
        r"(?P<pre>{mark})?[ \t\u{{a0}}\u{{202f}}]?(?P<num>[0-9]+(?:[.,'’\u{{a0}}\u{{202f}}][0-9]+)*)(?:[ \t\u{{a0}}\u{{202f}}]?(?P<post>{mark}))?"
    ))
    .expect("valid figure regex")
});

fn currency(marker: &str) -> &'static str {
    SYMBOLS
        .iter()
        .find(|(symbol, _)| *symbol == marker)
        .map(|(_, code)| *code)
        .or_else(|| CODES.iter().copied().find(|code| *code == marker))
        .unwrap_or("")
}

const fn is_separator(c: char) -> bool {
    matches!(c, '.' | ',' | '\'' | '’' | '\u{a0}' | '\u{202f}')
}

fn canonical(whole: &str, fraction: Option<&str>) -> String {
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    match fraction.map(|f| f.trim_end_matches('0')) {
        Some(fraction) if !fraction.is_empty() => format!("{whole}.{fraction}"),
        _ => whole.to_string(),
    }
}

fn readings(raw: &str) -> Vec<Vec<String>> {
    let parts: Vec<&str> = raw.split(is_separator).collect();
    let separators: Vec<char> = raw.chars().filter(|c| is_separator(*c)).collect();
    if parts.len() == 1 {
        return vec![vec![canonical(parts[0], None)]];
    }
    let grouped = |parts: &[&str]| {
        parts
            .first()
            .is_some_and(|first| (1..=3).contains(&first.len()))
            && parts[1..].iter().all(|part| part.len() == 3)
    };
    let last = parts.len() - 1;
    let same = |seps: &[char]| seps.windows(2).all(|pair| pair[0] == pair[1]);

    if grouped(&parts) && same(&separators) {
        let joined = canonical(&parts.concat(), None);
        if parts.len() == 2 && separators[0] == '.' {
            return vec![vec![joined, canonical(parts[0], Some(parts[1]))]];
        }
        return vec![vec![joined]];
    }
    if last >= 2
        && grouped(&parts[..last])
        && same(&separators[..last - 1])
        && matches!(separators[last - 1], '.' | ',')
        && separators[last - 1] != separators[0]
    {
        return vec![vec![canonical(&parts[..last].concat(), Some(parts[last]))]];
    }
    if parts.len() == 2 && matches!(separators[0], '.' | ',') {
        return vec![vec![canonical(parts[0], Some(parts[1]))]];
    }
    parts
        .iter()
        .map(|part| vec![canonical(part, None)])
        .collect()
}

#[derive(Debug, Default)]
pub(super) struct NumberSource {
    values: HashSet<String>,
    priced: HashSet<(String, &'static str)>,
    bare: HashSet<String>,
    currencies: HashSet<&'static str>,
}

impl NumberSource {
    pub(super) fn new<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut source = Self::default();
        for text in texts {
            source.read(text);
        }
        source
    }

    fn read(&mut self, text: &str) {
        for found in FIGURE.captures_iter(text) {
            let marker = found
                .name("pre")
                .or_else(|| found.name("post"))
                .map(|m| currency(m.as_str()))
                .filter(|code| !code.is_empty());
            let raw = &found["num"];
            for value in readings(raw).into_iter().flatten() {
                match marker {
                    Some(code) => {
                        self.currencies.insert(code);
                        self.priced.insert((value.clone(), code));
                    }
                    None => {
                        self.bare.insert(value.clone());
                    }
                }
                self.values.insert(value);
            }
        }
        for word in words(text) {
            if let Some((_, value)) = NUMBER_WORDS.iter().find(|(name, _)| *name == word) {
                self.values.insert((*value).to_string());
            }
        }
        for (symbol, code) in SYMBOLS {
            if text.contains(symbol) {
                self.currencies.insert(code);
            }
        }
        for token in text.split(|c: char| !c.is_ascii_alphabetic()) {
            if let Some(code) = CODES.iter().copied().find(|code| *code == token) {
                self.currencies.insert(code);
            }
        }
    }

    pub(super) fn verifies(&self, claim: &str) -> bool {
        if claim.chars().any(|c| c.is_numeric() && !c.is_ascii_digit()) {
            return false;
        }
        let figures_ok = FIGURE.captures_iter(claim).all(|found| {
            let marker = found
                .name("pre")
                .or_else(|| found.name("post"))
                .map(|m| currency(m.as_str()))
                .filter(|code| !code.is_empty());
            readings(&found["num"])
                .iter()
                .all(|alternatives| self.has_any(alternatives, marker))
        });
        figures_ok
            && words(claim).all(|word| {
                NUMBER_WORDS
                    .iter()
                    .find(|(name, _)| *name == word)
                    .is_none_or(|(_, value)| self.values.contains(*value))
            })
    }

    fn has_any(&self, alternatives: &[String], marker: Option<&'static str>) -> bool {
        alternatives.iter().any(|value| match marker {
            None => self.values.contains(value),
            Some(code) => {
                self.priced.contains(&(value.clone(), code))
                    || (self.bare.contains(value) && self.currencies.contains(code))
            }
        })
    }
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> NumberSource {
        NumberSource::new([text])
    }

    #[test]
    fn a_gist_whose_numbers_are_all_in_the_mail_is_verified() {
        let mail =
            "Receipt from Perplexity\nPro plan: $24.00\nRenews on 14 November 2026\nOrder #48213";
        let source = source(mail);
        assert!(source.verifies("Receipt: $24.00 for Perplexity Pro, renews 14 Nov"));
        assert!(source.verifies("Order 48213 renews in 2026"));
    }

    #[test]
    fn an_invented_amount_is_rejected_even_beside_verified_ones() {
        let source = source("Total $24.00 charged on 14 November");
        assert!(!source.verifies("Receipt: $20.00 for Pro"));
        assert!(!source.verifies("$24.00 charged on 14 Nov, 3 attempts"));
        assert!(source.verifies("$24.00 charged on 14 Nov"));
    }

    #[test]
    fn a_reformatted_amount_is_still_the_same_amount() {
        let source = source("Amount due: $1,234.50 (24.00 for the add-on, 007 seats)");
        for claim in [
            "$1,234.50 due",
            "1234.5 due",
            "$1234.50 due",
            "1234.50 USD due",
            "$24 add-on",
            "24.0 add-on",
            "7 seats",
        ] {
            assert!(source.verifies(claim), "{claim}");
        }
        let european = super::NumberSource::new(["Gesamt: 1.234,50 EUR"]);
        assert!(european.verifies("1,234.50 EUR"));
        assert!(european.verifies("1234.5 EUR"));
        assert!(!european.verifies("1,243.50 EUR"));
    }

    #[test]
    fn an_invented_date_is_rejected() {
        let source = source("Your plan renews on 14 November 2026.");
        assert!(source.verifies("Renews 14 Nov"));
        assert!(!source.verifies("Renews 15 Nov"));
        assert!(!source.verifies("Renews 14 Nov 2027"));
        let numeric = super::NumberSource::new(["Due 14.11.2026 at 09:30"]);
        assert!(numeric.verifies("Due 14 Nov at 9:30"));
        assert!(numeric.verifies("Due 14.11.2026"));
        assert!(!numeric.verifies("Due 11.14.2026 at 09:45"));
    }

    #[test]
    fn text_with_no_numbers_passes() {
        let source = source("Your payment failed. Update your card.");
        assert!(source.verifies("Payment failed; the card needs updating"));
        assert!(source.verifies(""));
        assert!(NumberSource::default().verifies("Payment failed"));
        assert!(!NumberSource::default().verifies("Payment of $5 failed"));
    }

    #[test]
    fn a_currency_the_mail_does_not_name_is_rejected() {
        let pounds = source("Total £24.00");
        assert!(pounds.verifies("£24 charged"));
        assert!(pounds.verifies("24.00 GBP charged"));
        assert!(!pounds.verifies("$24 charged"));
        assert!(!pounds.verifies("24 EUR charged"));
        let table = source("Amount: 24.00\nCurrency: USD");
        assert!(table.verifies("$24.00"));
        assert!(!table.verifies("£24.00"));
        let mixed = source("Total £24.00, or $30.00");
        assert!(!mixed.verifies("$24.00"));
        assert!(mixed.verifies("$30.00"));
    }

    #[test]
    fn a_piece_of_another_figure_does_not_verify_a_claim() {
        for (mail, claim) in [
            ("Total $24.50", "24 charged"),
            ("Total $24.00", "0 failures"),
            ("Total $24.00", "1 invoice"),
            ("1,000 requests", "1 request"),
            ("1,200 requests", "12 requests"),
            ("Invoice 2026-0042", "Invoice 42 and 2026 and 0"),
        ] {
            assert!(!source(mail).verifies(claim), "{mail} / {claim}");
        }
        assert!(source("Invoice 2026-0042").verifies("Invoice 42 of 2026"));
    }

    #[test]
    fn percentages_ids_and_counts_are_figures() {
        let source = source("Usage is at 85% of 1,000 requests. Ticket TKT-90417.");
        assert!(source.verifies("85% of 1000 requests used, ticket 90417"));
        assert!(!source.verifies("95% of requests used"));
        assert!(!source.verifies("ticket 90471"));
    }

    #[test]
    fn spelled_out_counts_need_the_same_figure_in_the_mail() {
        let source = source("Payment failed 3 times. We will retry in 2 days.");
        assert!(source.verifies("Three failed payments, retrying in two days"));
        assert!(!source.verifies("Five failed payments"));
        let spelled = super::NumberSource::new(["Three payments failed"]);
        assert!(spelled.verifies("3 payments failed"));
        assert!(spelled.verifies("No one needs to act"));
    }

    #[test]
    fn digits_the_check_cannot_read_fail_closed() {
        let source = source("Total 24 and 5 and 12");
        for claim in [
            "２４ charged",
            "24² charged",
            "½ charged",
            "Ⅻ charged",
            "٢٤ charged",
        ] {
            assert!(!source.verifies(claim), "{claim}");
        }
    }

    #[test]
    fn a_comma_between_numbers_is_not_a_thousands_group() {
        let source = source("Seats 5, 6 and 7");
        assert!(source.verifies("seats 5, 6, 7"));
        assert!(!source.verifies("5,607 seats"));
        let thousands = super::NumberSource::new(["5,607 seats"]);
        assert!(thousands.verifies("5607 seats"));
        assert!(!thousands.verifies("5,706 seats"));
    }
}
