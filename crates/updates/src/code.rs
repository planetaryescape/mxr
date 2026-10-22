//! The one-time code a message gives you: "Your verification code is
//! 482913", "G-123456 is your Google verification code". Found by words
//! and shape, never by a model, so the same mail always gives the same
//! code. A promo, gift or discount code is never one.

use once_cell::sync::Lazy;
use regex::Regex;

/// How much of the body is read. Codes sit at the top of the mail; a long
/// newsletter footer only adds chances of a wrong match.
const BODY_MAX: usize = 4_000;
/// How much text after the words a code may sit ("Your code is: 482913").
/// Counted in visible characters: mail laid out as a box pads every line
/// with spaces and draws rules in box characters, and neither uses it up.
const AFTER: usize = 120;
/// How close before the words a code must end: "482913 is your code".
const BEFORE: usize = 16;

/// The words that say a code follows or precedes.
static TRIGGER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?ix)
        \b(?:verification|security|login|log-?in|sign-?in|one-?time|single-?use|confirmation
           |access|authentication|auth|2fa|two-factor|mfa|otp|account|device|identity|\d-digit)
          \s+(?:code|pin|passcode)\b
        | \bone-?time\s+pass(?:word|code)\b
        | \b(?:otp|passcode)\b
        | \byour\s+(?:\S+\s+){0,3}?code\b
        | \bcode\s*(?:is|:)
        | \benter\s+(?:this|the)(?:\s+following)?\s+(?:\S+\s+){0,3}?code\b
        | \buse\s+(?:this\s+)?code\b",
    )
    .expect("valid code trigger regex")
});

/// A code to spend or a number to quote, not one to sign in with: promo
/// words anywhere in the sentence, or a kind of number named with "code".
static NOT_A_SIGN_IN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?ix)
        \b(?:promo(?:tion(?:al)?)?|discount|coupon|voucher|referral|gift|redeem|reward|loyalty)\b
        | \d+\s?%\s+off\b
        | \b(?:tracking|post|zip|area|dial(?:ling)?|sort|branch|bank|order|booking|reference|invoice|product|error|status|country|offer|sale)\s+code\b
        | \bpostcode\b",
    )
    .expect("valid not-a-sign-in regex")
});

/// Subjects about something other than signing in. Their bodies quote
/// booking PINs and branch codes, so only a code the subject names counts.
static OTHER_SUBJECT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(?:booking|booked|reservation|order|receipt|invoice|statement|discount|sale|offer|deal|reminder|newsletter|digest|shipped|delivery|last\s+chance)\b|\d+\s?%\s+off\b",
    )
    .expect("valid other-subject regex")
});

/// "is" or a colon: what comes right after is the value, so it wins over a
/// number earlier in the sentence ("code for card ending 4242 is 958361").
static MARKER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(?:is|are)\b\s*:?|:").expect("valid marker regex"));

/// What a number right after names it as something else: a card or account
/// ending, an order or reference, an amount in a currency code.
static NAMED_BEFORE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:(?i:\b(?:ending(?:\s+in)?|ends\s+in|last\s+(?:4|four)(?:\s+digits)?|card|account|acct|order|ref(?:erence)?|number|no\.?|ticket|case))\s*[:#]?|\b(?:GBP|USD|EUR|ZAR|CAD|AUD|NZD|INR|JPY|CHF|SEK|NOK|DKK))\s*$",
    )
    .expect("valid named-before regex")
});

/// Whether `before` ends in a label naming the next value as something else.
/// "Account:" is a label; "verify your account:" is an instruction.
fn named_before(before: &str) -> bool {
    NAMED_BEFORE.find(before).is_some_and(|label| {
        let lead = before[..label.start()].trim_end().to_ascii_lowercase();
        !["your", "the", "this", "my", "our"]
            .iter()
            .any(|word| lead.ends_with(&format!(" {word}")) || lead == *word)
    })
}

/// Code-shaped words: digits, groups like 123-456, or capitals with digits.
static CANDIDATE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:[A-Z]-)?(?:\d{3}[ -]\d{3}|[A-Z0-9]{3,5}-[A-Z0-9]{3,5}|[A-Z0-9]{4,10})\b")
        .expect("valid code candidate regex")
});

/// The one-time code in a message, if it gives one.
pub fn one_time_code(subject: &str, body: Option<&str>) -> Option<String> {
    if let Some(code) = in_text(subject) {
        return Some(code);
    }
    // A subject about spending ("Your promo code") names no sign-in code.
    let subject_names_a_code = TRIGGER.is_match(subject) && !NOT_A_SIGN_IN.is_match(subject);
    if OTHER_SUBJECT.is_match(subject) && !subject_names_a_code {
        return None;
    }
    let body = body?;
    let end = floor_char_boundary(body, BODY_MAX);
    let body = &body[..end];
    in_text(body).or_else(|| {
        // The subject says what it is and the body is the code on a line of
        // its own: "Your verification code" / "482913".
        subject_names_a_code
            .then(|| alone_on_a_line(&body[..floor_char_boundary(body, ALONE_MAX)]))
            .flatten()
    })
}

/// How far into the body a code alone on its line is looked for.
const ALONE_MAX: usize = 1_500;

fn alone_on_a_line(text: &str) -> Option<String> {
    let mut offset = 0;
    // The last line with words, which may name the value below it
    // ("Order number:" over "55210").
    let mut above = "";
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let start = offset + (line.len() - line.trim_start().len());
        offset += line.len();
        if trimmed.is_empty() {
            continue;
        }
        let whole = CANDIDATE
            .find(trimmed)
            .filter(|m| m.start() == 0 && m.end() == trimmed.len());
        if let Some(m) = whole {
            if !named_before(above)
                && !NOT_A_SIGN_IN.is_match(above)
                && looks_like_a_code(text, start + m.start(), start + m.end(), Trust::Placed)
            {
                return Some(m.as_str().to_string());
            }
        }
        above = trimmed;
    }
    None
}

fn in_text(text: &str) -> Option<String> {
    for trigger in TRIGGER.find_iter(text) {
        // After the words, in their sentence ("…code is 482913. Order 5521…"),
        // or the next one when they point there ("…the code below. 911110").
        let reach = text[trigger.end()..]
            .char_indices()
            .filter(|&(_, c)| !c.is_whitespace() && !('\u{2500}'..='\u{257F}').contains(&c))
            .nth(AFTER)
            .map_or(text.len(), |(i, _)| trigger.end() + i);
        let mut end = trigger.end() + sentence_end(&text[trigger.end()..reach]);
        if text[trigger.end()..end].trim_end().ends_with("below") {
            let next = ceil_char_boundary(text, end + 1);
            end = next + sentence_end(&text[next..reach.max(next)]);
        }
        // The value after the last "is" or colon in the sentence is the code:
        // that is where a sentence ends up naming it ("…for the card whose
        // last four digits are 4242 is 958361", "…signing in: 2024"). A
        // value named as something else ("Card ending: 4242") gives way to
        // the marker before it. A trigger that ends in "is" or a colon is a
        // marker itself ("Code: 2024").
        let ends_marked = trigger.as_str().trim_end().ends_with(':')
            || trigger
                .as_str()
                .trim_end()
                .to_ascii_lowercase()
                .ends_with(" is");
        let mut markers: Vec<usize> = MARKER
            .find_iter(&text[trigger.end()..end])
            .map(|marker| trigger.end() + marker.end())
            .collect();
        if ends_marked {
            markers.insert(0, trigger.end());
        }
        let marked = markers.into_iter().rev().find_map(|at| {
            let skipped = text[at..end]
                .find(|c: char| !c.is_whitespace() && !('\u{2500}'..='\u{257F}').contains(&c))
                .map_or(end, |i| at + i);
            CANDIDATE
                .find_at(&text[..end], skipped)
                .filter(|m| m.start() == skipped)
                .filter(|m| looks_like_a_code(text, m.start(), m.end(), Trust::Placed))
        });
        let found = marked.or_else(|| candidates(text, trigger.end(), end).next());
        if let Some(code) = found {
            // The whole sentence: "Use this code SAVE2026 for a discount."
            if !spends(text, &trigger, end.max(code.end())) {
                return Some(code.as_str().to_string());
            }
        }
        // Right before them: "482913 is your verification code".
        let from = ceil_char_boundary(text, trigger.start().saturating_sub(BEFORE));
        if let Some(code) = candidates(text, from, trigger.start())
            .filter(|code| !text[code.end()..trigger.start()].contains(['.', '!', '?', '\n']))
            .last()
        {
            if !spends(text, &trigger, trigger.end()) {
                return Some(code.as_str().to_string());
            }
        }
    }
    None
}

/// Code-shaped whole words between `from` and `to` in `text`.
fn candidates(text: &str, from: usize, to: usize) -> impl Iterator<Item = regex::Match<'_>> {
    CANDIDATE.find_iter(&text[..to]).filter(move |m| {
        m.start() >= from && looks_like_a_code(text, m.start(), m.end(), Trust::Nearby)
    })
}

/// How sure the place a word sits in makes it a code.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trust {
    /// Right after "is" or a colon, or alone on its line: even a
    /// year-shaped number is the code (a named one still is not).
    Placed,
    /// Just near the words: a year or a named number is not.
    Nearby,
}

fn looks_like_a_code(text: &str, start: usize, end: usize, trust: Trust) -> bool {
    let word = &text[start..end];
    let digits = word.chars().filter(char::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let plain: String = word.chars().filter(char::is_ascii_alphanumeric).collect();
    if plain.len() < 4 || plain.len() > 10 {
        return false;
    }
    // A year in a footer or a date: "© 2026", "October 2026".
    if trust == Trust::Nearby
        && plain.len() == 4
        && digits == 4
        && (plain.starts_with("19") || plain.starts_with("20"))
    {
        return false;
    }
    // A number the words before it name as something else: "card ending
    // 4242", "order 55210", "GBP 5483".
    let lead = ceil_char_boundary(text, start.saturating_sub(32));
    if named_before(&text[lead..start]) {
        return false;
    }
    // Part of a longer word or number, money or a percentage: "ITMS-90683",
    // "$1234", "1234%", "1,234", "12.50".
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    let after_next = text[end..].chars().nth(1);
    !(before.is_some_and(|c| {
        c.is_alphanumeric() || matches!(c, '-' | '$' | '£' | '€' | '#' | '.' | ',' | '/')
    }) || after.is_some_and(|c| c.is_alphanumeric() || matches!(c, '%' | '/' | '-'))
        // "1,234" and "12.50" continue a number; "482913, valid for…" ends one.
        || matches!(after, Some(',' | '.')) && after_next.is_some_and(|c| c.is_ascii_digit()))
}

/// Whether the sentence around the words says the code is to spend or a
/// number to quote.
fn spends(text: &str, trigger: &regex::Match<'_>, to: usize) -> bool {
    let from = sentence_start(&text[..trigger.start()]);
    let to = floor_char_boundary(text, to);
    NOT_A_SIGN_IN.is_match(&text[from..to])
}

/// Bytes from the start of `text` to the end of its first sentence. Not a
/// blank line: a code often sits on its own line below "Your code is:".
fn sentence_end(text: &str) -> usize {
    text.char_indices()
        .find(|&(i, c)| {
            matches!(c, '.' | '!' | '?') && text[i + 1..].starts_with(|n: char| n.is_whitespace())
        })
        .map_or(text.len(), |(i, _)| i)
}

/// Where the last sentence in `text` starts.
fn sentence_start(text: &str) -> usize {
    text.char_indices()
        .rev()
        .find(|&(i, c)| {
            matches!(c, '.' | '!' | '?' | '\n')
                && text[i + c.len_utf8()..].starts_with(|n: char| n.is_whitespace())
        })
        .map_or(0, |(i, c)| i + c.len_utf8())
}

fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::one_time_code;

    fn code(subject: &str, body: &str) -> Option<String> {
        one_time_code(subject, Some(body))
    }

    #[test]
    fn finds_the_code_in_the_subject() {
        assert_eq!(
            code("Your verification code is 482913", "").as_deref(),
            Some("482913")
        );
        assert_eq!(
            code("G-123456 is your Google verification code", "").as_deref(),
            Some("G-123456")
        );
        assert_eq!(
            code("123456 is your Instagram code", "").as_deref(),
            Some("123456")
        );
        assert_eq!(
            code("Slack confirmation code: ABC-DEF4", "").as_deref(),
            Some("ABC-DEF4")
        );
    }

    #[test]
    fn finds_the_code_in_the_body_when_the_subject_has_none() {
        assert_eq!(
            code(
                "Sign in to Acme",
                "Hi Ada,\n\nYour login code is:\n\n739 204\n\nIt expires in 10 minutes."
            )
            .as_deref(),
            Some("739 204")
        );
        assert_eq!(
            code(
                "Confirm it's you",
                "Use this code to finish signing in: 5F3K9Q\nIf this wasn't you, ignore this email."
            )
            .as_deref(),
            Some("5F3K9Q")
        );
        assert_eq!(
            code(
                "Your one-time passcode",
                "Enter the following code: 0042817"
            )
            .as_deref(),
            Some("0042817")
        );
    }

    #[test]
    fn a_promo_or_gift_code_is_not_one() {
        assert_eq!(code("Your promo code is SAVE20NOW", ""), None);
        assert_eq!(code("20% off: use code SPRING2026 at checkout", ""), None);
        assert_eq!(
            code("Your gift card code", "Redeem code 4821-9932 at checkout."),
            None
        );
    }

    #[test]
    fn an_account_or_reference_number_is_not_one() {
        assert_eq!(
            code("Your statement", "Account number 12345678 is now open."),
            None
        );
        assert_eq!(code("Your order", "Your order code is 55210."), None);
    }

    #[test]
    fn numbers_that_are_not_codes_are_skipped() {
        assert_eq!(code("Your code is ready", "© 2026 Acme Inc."), None);
        assert_eq!(
            code(
                "Your account summary",
                "Your balance is $1234.56 this month."
            ),
            None
        );
        assert_eq!(code("Weekly report", "Visitors up 1234% this week."), None);
        assert_eq!(code("Hello", "No code here, just 482913 apples."), None);
    }

    #[test]
    fn the_code_in_the_same_sentence_wins_over_a_later_number() {
        assert_eq!(
            code(
                "Verify your sign-in",
                "Your security code is 918273. Order 55210 shipped yesterday."
            )
            .as_deref(),
            Some("918273")
        );
    }

    #[test]
    fn real_layouts_from_a_mailbox() {
        // The code on the line after "below".
        assert_eq!(
            code(
                "Your app verification code",
                "Please enter the verification code below.\n\n911110\n\nNote: it expires in 24 hours."
            )
            .as_deref(),
            Some("911110")
        );
        assert_eq!(
            code(
                "DigitalOcean Verification Code",
                "Enter this 6 digit code on the sign in page to confirm your identity: 405742 If you receive"
            )
            .as_deref(),
            Some("405742")
        );
        // A card check for a purchase is a sign-in code, whatever it buys.
        assert_eq!(
            code(
                "Your SafeKey Verification Code",
                "Your verification code for a £548.43 transaction at Booking.com is: 092730 Here's what"
            )
            .as_deref(),
            Some("092730")
        );
        assert_eq!(
            code("Your single-use code", "Your single-use code is: 933871").as_deref(),
            Some("933871")
        );
        assert_eq!(
            code("WhatsApp Verification Code: 659-964", "").as_deref(),
            Some("659-964")
        );
    }

    #[test]
    fn a_rule_of_box_characters_between_the_words_and_the_code_is_skipped() {
        let body = format!(
            "Your verification code for a £548.43 transaction at Booking.com\nis:\n{}\n092730",
            "─".repeat(70)
        );
        assert_eq!(
            code("Your SafeKey Verification Code", &body).as_deref(),
            Some("092730")
        );
    }

    #[test]
    fn padding_in_a_boxed_layout_does_not_use_up_the_reach() {
        let pad = " ".repeat(80);
        let body = format!(
            "Below is your SafeKey Verification Code{pad}\nYour verification code for a £548.43 transaction at Booking.com{pad}\nis:{pad}\n{}{pad}\n092730",
            "─".repeat(71)
        );
        assert_eq!(
            code("Your SafeKey Verification Code", &body).as_deref(),
            Some("092730")
        );
    }

    #[test]
    fn a_sale_subject_keeps_its_codes_to_itself() {
        assert_eq!(
            code(
                "30% off Coconut - last chance today!",
                "Use code SEPT30 at checkout."
            ),
            None
        );
    }

    #[test]
    fn quoted_numbers_in_other_mail_are_not_codes() {
        assert_eq!(
            code(
                "Thanks! Your booking is confirmed at Roslin Beach Hotel",
                "Confirmation number: 5921760267 PIN code: 1391"
            ),
            None
        );
        assert_eq!(
            code(
                "Truworths acc - Discount Reminder",
                "Branch Code: 020909 Account Number: 060551151"
            ),
            None
        );
        assert_eq!(
            code(
                "Action needed: the uploaded build has issues",
                "ITMS-90683: Missing purpose string in Info.plist - Your app's code references APIs."
            ),
            None
        );
    }

    #[test]
    fn the_value_after_is_wins_over_a_number_earlier_in_the_sentence() {
        assert_eq!(
            code(
                "Hi",
                "Your verification code for card ending 4242 is 958361."
            )
            .as_deref(),
            Some("958361")
        );
        assert_eq!(
            code(
                "Hi",
                "Your verification code for a GBP 5483 transaction is 092730"
            )
            .as_deref(),
            Some("092730")
        );
        assert_eq!(
            code("Your verification code is 2024", "").as_deref(),
            Some("2024")
        );
        assert_eq!(code("Your offer code is SAVE20", ""), None);
    }

    #[test]
    fn a_marker_further_on_still_checks_what_follows_it() {
        assert_eq!(
            code(
                "Hi",
                "Your verification code for the device whose model is 2024 is 958361."
            )
            .as_deref(),
            Some("958361")
        );
        assert_eq!(
            code(
                "Hi",
                "Your verification code is: see below\nCard ending: 4242\n958361"
            )
            .as_deref(),
            Some("958361")
        );
        assert_eq!(code("Code: 2024", "").as_deref(), Some("2024"));
        assert_eq!(code("Code is 2024", "").as_deref(), Some("2024"));
    }

    #[test]
    fn round_three_layouts() {
        assert_eq!(
            code(
                "Hi",
                "Your verification code for the card whose last four digits are 4242 is 958361."
            )
            .as_deref(),
            Some("958361")
        );
        assert_eq!(code("Your code", "Discount code:\nSAVE20NOW"), None);
        assert_eq!(
            code("Your verification code", "Gift code:\n4821-9932"),
            None
        );
        assert_eq!(
            code("Welcome to Acme", "Use this code SAVE2026 for a discount."),
            None
        );
        assert_eq!(
            code(
                "Your verification code",
                "Use this code to verify your account:\n958361"
            )
            .as_deref(),
            Some("958361")
        );
        assert_eq!(
            code(
                "Confirm it is you",
                "Use this code to finish signing in: 2024"
            )
            .as_deref(),
            Some("2024")
        );
        // A bare label still names what follows it.
        assert_eq!(code("Your verification code", "Account:\n12345678"), None);
        // The code first, a reference after: the reference gives way.
        assert_eq!(
            code("Hi", "Your code is 482913 (reference: 55210).").as_deref(),
            Some("482913")
        );
    }

    #[test]
    fn a_value_alone_on_its_line_keeps_the_exclusions() {
        assert_eq!(code("Your verification code", "Order number:\n55210"), None);
        assert_eq!(code("Your promo code", "SAVE20NOW"), None);
    }

    #[test]
    fn a_comma_after_the_code_ends_it() {
        assert_eq!(
            code("Hi", "Your code is 482913, valid for 10 minutes.").as_deref(),
            Some("482913")
        );
        assert_eq!(
            code("Weekly report", "Your code is ready: 1,234 visits."),
            None
        );
    }

    #[test]
    fn a_code_alone_under_a_subject_that_names_it() {
        assert_eq!(
            code("Your verification code", "482913\n\nExpires in 5 minutes.").as_deref(),
            Some("482913")
        );
        // Not without a subject saying so.
        assert_eq!(code("Hello", "482913\n\nSee you soon."), None);
    }

    #[test]
    fn multibyte_text_near_the_limits_does_not_panic() {
        let long = format!("{}Your code is 123456", "é".repeat(3_000));
        assert_eq!(code("Hi", &long), None, "past the part of the body read");
        assert_eq!(
            code("Ваш код: 123456 — your code: 654321", "").as_deref(),
            Some("654321")
        );
    }
}
