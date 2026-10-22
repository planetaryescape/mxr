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

/// Numbers (prices, times, dates, counts) in the draft that the model could
/// not have seen: none of their digits appear in `known`, the text of the
/// conversation, the typed text and the instruction. Matching on digits means
/// "$5,000" is known when the conversation has "5000" or "5,000".
pub fn invented_numbers(draft: &str, known: &str) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_the_model_could_not_have_seen() {
        let known = "Can we meet on the 14th at 3pm? Budget is $4,000.";
        let draft = "Yes, the 14th at 3pm works. I can do $5,000 and 20 seats.";
        assert_eq!(invented_numbers(draft, known), vec!["5,000", "20"]);
    }
}
