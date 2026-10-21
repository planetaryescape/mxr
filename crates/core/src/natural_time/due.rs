//! Due words a model copied out of an email, checked and resolved by code.
//!
//! A model never writes a date (D111, D115). It copies the words that say
//! when, such as "by Friday"; the words must appear in the text it read, and
//! this parser turns them into an instant against the time the text was
//! written. Words that aren't in the text, are too long to be a time, or
//! don't parse leave the item undated rather than trusting the model.

use chrono::{DateTime, TimeZone};

use super::{resolve_time, TimePrefs, TimeResolution};
use crate::text::find_verbatim;

/// Longer than any real due phrase: a longer match is a sentence, not a time.
const DUE_MAX_CHARS: usize = 60;

/// Words in front of a due phrase that the time parser doesn't read.
const DUE_FILLERS: &[&str] = &[
    "by", "on", "before", "until", "till", "no", "later", "than", "at", "the", "this", "for",
    "sometime", "around", "within",
];

/// Due words found in the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueWords {
    /// The words as the source writes them, whitespace collapsed.
    pub phrase: String,
    /// `None` when the parser can't read them.
    pub resolution: Option<TimeResolution>,
}

/// `model_words` checked against `source` and resolved against `now` in
/// `now`'s zone, or `None` when the source doesn't say them.
pub fn due_words<Tz>(
    source: &str,
    model_words: &str,
    now: &DateTime<Tz>,
    prefs: &TimePrefs,
) -> Option<DueWords>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let phrase = find_verbatim(source, model_words, DUE_MAX_CHARS)?;
    let resolution = resolve_due_phrase(&phrase, now, prefs);
    Some(DueWords { phrase, resolution })
}

/// People write "by Friday" or "first thing Monday", so leading words the
/// parser doesn't read are dropped until what's left resolves.
fn resolve_due_phrase<Tz>(
    phrase: &str,
    now: &DateTime<Tz>,
    prefs: &TimePrefs,
) -> Option<TimeResolution>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let words: Vec<&str> = phrase
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| matches!(c, ',' | '.' | '!' | '?' | ';')))
        .filter(|word| !word.is_empty())
        .collect();
    let first_kept = words
        .iter()
        .position(|word| !DUE_FILLERS.contains(&word.to_ascii_lowercase().as_str()))
        .unwrap_or(words.len());
    // "within 2 days" reads as "in 2 days".
    let within = first_kept > 0 && words[first_kept - 1].eq_ignore_ascii_case("within");
    (first_kept..words.len()).find_map(|start| {
        let mut candidate = words[start..].join(" ");
        if within && start == first_kept {
            candidate = format!("in {candidate}");
        }
        resolve_time(&candidate, now, prefs).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Utc, Weekday};
    use chrono_tz::Europe::London;

    /// Tuesday 29 September 2026, 15:00 in London (14:00 UTC).
    fn anchor() -> DateTime<chrono_tz::Tz> {
        London.with_ymd_and_hms(2026, 9, 29, 15, 0, 0).unwrap()
    }

    fn at(source: &str, words: &str) -> Option<DateTime<Utc>> {
        due_words(source, words, &anchor(), &TimePrefs::default())?
            .resolution
            .map(|resolution| resolution.at)
    }

    fn friday_nine() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 2, 8, 0, 0).unwrap()
    }

    #[test]
    fn words_the_text_says_resolve_with_the_parser() {
        let found = due_words(
            "Thanks. I'll send the deck\nBY   friday.",
            "by Friday",
            &anchor(),
            &TimePrefs::default(),
        )
        .unwrap();
        assert_eq!(
            found.phrase, "BY friday",
            "the text's spelling, not the model's"
        );
        assert_eq!(found.resolution.unwrap().at, friday_nine());
    }

    #[test]
    fn leading_fillers_are_dropped_until_the_words_resolve() {
        for words in ["by Friday", "no later than friday.", "first thing Friday"] {
            assert_eq!(at(words, words), Some(friday_nine()), "{words}");
        }
        assert_eq!(
            at("within 2 days", "within 2 days"),
            Some(Utc.with_ymd_and_hms(2026, 10, 1, 14, 0, 0).unwrap())
        );
    }

    #[test]
    fn words_the_text_never_says_are_not_due_words() {
        assert_eq!(
            due_words(
                "I'll send it soon.",
                "by Monday",
                &anchor(),
                &TimePrefs::default()
            ),
            None
        );
    }

    #[test]
    fn a_model_written_date_is_never_trusted() {
        let source = "I'll send the deck by Friday.";
        for words in ["2026-10-09T17:00:00Z", "2026-10-09", "9 October"] {
            assert_eq!(
                due_words(source, words, &anchor(), &TimePrefs::default()),
                None,
                "{words}"
            );
        }
    }

    #[test]
    fn found_words_that_do_not_parse_stay_undated() {
        let found = due_words(
            "I'll get to it at some point.",
            "at some point",
            &anchor(),
            &TimePrefs::default(),
        )
        .unwrap();
        assert_eq!(found.phrase, "at some point");
        assert_eq!(found.resolution, None);
    }

    #[test]
    fn a_long_match_is_a_sentence_not_a_time() {
        let sentence = "by the time the whole team is back from the offsite in the mountains";
        assert!(sentence.chars().count() > DUE_MAX_CHARS);
        assert_eq!(
            due_words(sentence, sentence, &anchor(), &TimePrefs::default()),
            None
        );
    }

    #[test]
    fn words_resolve_against_when_the_text_was_written() {
        // Written on Monday 7 September: that week's Friday, already past now.
        let written = London.with_ymd_and_hms(2026, 9, 7, 10, 0, 0).unwrap();
        let due = due_words(
            "I'll send it by Friday.",
            "by Friday",
            &written,
            &TimePrefs::default(),
        )
        .unwrap()
        .resolution
        .unwrap()
        .at;
        assert_eq!(due, Utc.with_ymd_and_hms(2026, 9, 11, 8, 0, 0).unwrap());
        assert_eq!(due.with_timezone(&London).weekday(), Weekday::Fri);
    }
}
