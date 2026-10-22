//! Text helpers the graders share. Comparisons run on normalised text, so
//! case and spacing never decide a grade; curly apostrophes count as straight
//! ones because models and keyboards differ on them.

use std::collections::HashSet;

/// How many words in a row a draft may copy from the conversation before it
/// counts as quoting the thread back. Shorter runs are ordinary phrases
/// ("the Q3 numbers") that a reply to the thread should reuse.
pub(super) const RECAP_RUN_WORDS: usize = 8;

pub(super) fn normalise(text: &str) -> String {
    text.replace(['\u{2018}', '\u{2019}'], "'")
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words without the punctuation around them, so "me." and "me" are one word.
/// Inner marks stay: "i'll" is one word.
pub(super) fn words(text: &str) -> Vec<String> {
    normalise(text)
        .split(' ')
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// The sentences of what the user typed. Splits on full stops, question and
/// exclamation marks, and line breaks; a stop inside a number ("2.5") is not
/// a break in practice because typed sentences are short.
pub(super) fn sentences(typed: &str) -> Vec<String> {
    typed
        .split(['.', '!', '?', '\n'])
        .map(normalise)
        .filter(|sentence| !sentence.is_empty())
        .collect()
}

/// Whether `phrase` appears in `draft_words` as a run of whole words. "no"
/// does not match inside "now", and "not able" needs both words in order.
pub(super) fn contains_phrase(draft_words: &[String], phrase: &str) -> bool {
    let phrase = words(phrase);
    !phrase.is_empty()
        && draft_words
            .windows(phrase.len())
            .any(|run| run == phrase.as_slice())
}

/// The text inside each `[[?: …]]` gap, normalised.
pub(super) fn gap_texts(text: &str) -> Vec<String> {
    text.split("[[?:")
        .skip(1)
        .filter_map(|rest| rest.split_once("]]").map(|(inside, _)| normalise(inside)))
        .collect()
}

/// The first run of `RECAP_RUN_WORDS` words copied from the conversation, if
/// the draft has one.
pub(super) fn copied_run(draft: &str, conversation: &str) -> Option<String> {
    let source: HashSet<Vec<String>> = words(conversation)
        .windows(RECAP_RUN_WORDS)
        .map(<[String]>::to_vec)
        .collect();
    words(draft)
        .windows(RECAP_RUN_WORDS)
        .find(|run| source.contains(*run))
        .map(|run| run.join(" "))
}

/// Numbers (prices, times, dates, counts) in the draft that no input carries.
/// A draft number is known when its complete token matches one in `known`
/// after normalisation: thousands commas are dropped and case is ignored, so
/// "£1200" matches "£1,200", but "£200" does not, "2pm" does not match "12pm",
/// and "$45" does not match "$450".
pub fn invented_numbers(draft: &str, known: &str) -> Vec<String> {
    let known_keys: HashSet<String> = number_tokens(known).iter().map(|t| number_key(t)).collect();
    let mut out: Vec<String> = Vec::new();
    for token in number_tokens(draft) {
        let key = number_key(token);
        if !known_keys.contains(&key) && !out.iter().any(|seen| number_key(seen) == key) {
            out.push(token.to_string());
        }
    }
    out
}

/// The replay's rule, kept as it was so `mxr draft eval --set mine` reports
/// what it reported before: a number is known when its digits appear
/// anywhere in the known text, so "£200" passes against "£1,200". Quick uses
/// [`invented_numbers`].
pub fn replay_invented_numbers(draft: &str, known: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in
        draft.split(|c: char| c.is_whitespace() || matches!(c, ';' | '(' | ')' | '!' | '?'))
    {
        let token = token.trim_matches(|c: char| !c.is_alphanumeric());
        if token.is_empty() || !token.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let digits: String = token.chars().filter(char::is_ascii_digit).collect();
        if !known.contains(token)
            && !known.contains(&digits)
            && !out.iter().any(|seen| seen == token)
        {
            out.push(token.to_string());
        }
    }
    out
}

/// The tokens of a text that carry a digit, split on the marks that join
/// numbers in ranges and dates ("4,000-5,000", "10/12") so each part is
/// compared on its own.
fn number_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ';' | '(' | ')' | '!' | '?' | '/' | '-'))
        .map(|token| token.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|token| token.chars().any(|c| c.is_ascii_digit()))
        .collect()
}

/// A number in its comparable form: thousands commas removed, case folded,
/// and an ordinal suffix dropped, so "30th" is the number 30. "2pm" keeps its
/// unit, so it stays apart from "12pm".
fn number_key(token: &str) -> String {
    let key = token.replace(',', "").to_lowercase();
    for suffix in ["st", "nd", "rd", "th"] {
        if let Some(digits) = key.strip_suffix(suffix) {
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return digits.to_string();
            }
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_the_model_could_not_have_seen() {
        let known = "Can we meet on the 14th at 3pm? Budget is $4,000.";
        let draft = "Yes, the 14th at 3pm works. I can do $5,000 and 20 seats.";
        assert_eq!(invented_numbers(draft, known), vec!["5,000", "20"]);
    }

    #[test]
    fn a_number_is_known_only_when_its_whole_token_is() {
        let known = "The fee is £1,200 for 12pm sessions.";
        assert_eq!(
            invented_numbers("£1200 is fine", known),
            Vec::<String>::new()
        );
        assert_eq!(invented_numbers("£200 is fine", known), vec!["200"]);
        assert_eq!(invented_numbers("the 2pm slot", "at 12pm"), vec!["2pm"]);
        assert_eq!(invented_numbers("$45", "$450"), vec!["45"]);
        assert_eq!(invented_numbers("$450", "$45"), vec!["450"]);
    }

    #[test]
    fn an_ordinal_is_the_number_it_ends() {
        assert_eq!(
            invented_numbers("on the 30th", "Thursday 30 October"),
            Vec::<String>::new()
        );
        assert_eq!(
            invented_numbers("the 31st", "Thursday 30 October"),
            vec!["31st"]
        );
    }

    #[test]
    fn the_replay_rule_is_unchanged_and_still_matches_digits_inside_a_number() {
        // `--set mine` keeps the rule it has always reported under.
        assert_eq!(
            replay_invented_numbers("£200", "£1,200"),
            Vec::<String>::new()
        );
        assert_eq!(replay_invented_numbers("$45", "$450"), Vec::<String>::new());
    }
}
