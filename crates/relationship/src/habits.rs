//! How the user actually writes, described in plain sentences a model can
//! follow ("usually opens 'Hey {name},'", "signs off 'Cheers, Sam'",
//! "about 40 words"), measured from their real sent mail.
//!
//! Stylometry numbers such as "formality 0.53" mean nothing to a model;
//! the habits a reader would notice do. Everything here is pure text
//! analysis over already-cleaned bodies (see [`clean_for_voice`]).

use mxr_reader::{clean, ReaderConfig};

/// A body cleaned to what the user typed: no quoted history, signature
/// block, boilerplate or tracking.
pub fn clean_for_voice(text: &str) -> String {
    let config = ReaderConfig {
        html_command: None,
        strip_signatures: true,
        collapse_quotes: true,
        strip_boilerplate: true,
        strip_tracking: true,
    };
    let looks_html = text.trim_start().starts_with('<');
    let output = if looks_html {
        clean(None, Some(text), &config)
    } else {
        clean(Some(text), None, &config)
    };
    output.content.trim().to_string()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WritingHabits {
    pub samples: usize,
    pub median_words: u32,
    /// Most common opening line, with the recipient's name as `{name}`.
    pub greeting: Option<Habit>,
    /// Share of messages that open with a greeting at all.
    pub greeting_rate: f64,
    /// Most common closing (one or two short lines joined by a newline).
    pub sign_off: Option<Habit>,
    pub sign_off_rate: f64,
    pub contraction_rate: f64,
    pub lowercase_start_rate: f64,
    pub exclamations_per_message: f64,
    pub uses_emoji: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Habit {
    pub text: String,
    pub count: usize,
}

const GREETING_WORDS: &[&str] = &[
    "hi",
    "hey",
    "hello",
    "dear",
    "morning",
    "afternoon",
    "evening",
    "hiya",
    "yo",
    "howdy",
    "greetings",
];

/// Measure `bodies` (cleaned; see [`clean_for_voice`]). `names` are the
/// people the messages were written to; their first names become `{name}`
/// so a greeting habit carries over to someone new.
pub fn analyse(bodies: &[&str], names: &[&str]) -> WritingHabits {
    let bodies: Vec<&str> = bodies
        .iter()
        .map(|body| body.trim())
        .filter(|body| !body.is_empty())
        .collect();
    if bodies.is_empty() {
        return WritingHabits::default();
    }
    let first_names: Vec<String> = names
        .iter()
        .filter_map(|name| name.split_whitespace().next())
        .map(|name| {
            name.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|name| name.len() > 1)
        .collect();

    let mut words: Vec<u32> = Vec::new();
    let mut greetings: Vec<String> = Vec::new();
    let mut sign_offs: Vec<String> = Vec::new();
    let mut contractions = 0usize;
    let mut total_words = 0usize;
    let mut lowercase_starts = 0usize;
    let mut exclamations = 0usize;
    let mut emoji = false;

    for body in &bodies {
        let lines: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let tokens: Vec<&str> = body.split_whitespace().collect();
        words.push(tokens.len() as u32);
        total_words += tokens.len();
        contractions += tokens.iter().filter(|token| is_contraction(token)).count();
        exclamations += body.matches('!').count();
        emoji |= body.chars().any(is_emoji);

        let greeting = lines.first().copied().filter(|line| is_greeting(line));
        if let Some(line) = greeting {
            greetings.push(normalise(line, &first_names));
        }
        let content_start = if greeting.is_some() {
            lines.get(1)
        } else {
            lines.first()
        };
        if content_start
            .and_then(|line| line.chars().find(|c| c.is_alphabetic()))
            .is_some_and(char::is_lowercase)
        {
            lowercase_starts += 1;
        }
        if lines.len() >= 2 {
            if let Some(closing) = sign_off(&lines) {
                sign_offs.push(normalise(&closing, &first_names));
            }
        }
    }

    words.sort_unstable();
    let n = bodies.len();
    WritingHabits {
        samples: n,
        median_words: words[n / 2],
        greeting: most_common(&greetings),
        greeting_rate: greetings.len() as f64 / n as f64,
        sign_off: most_common(&sign_offs),
        sign_off_rate: sign_offs.len() as f64 / n as f64,
        contraction_rate: if total_words == 0 {
            0.0
        } else {
            contractions as f64 / total_words as f64
        },
        lowercase_start_rate: lowercase_starts as f64 / n as f64,
        exclamations_per_message: exclamations as f64 / n as f64,
        uses_emoji: emoji,
    }
}

impl WritingHabits {
    /// Plain sentences for a prompt. Empty when there is nothing to go on.
    pub fn describe(&self) -> Vec<String> {
        if self.samples == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let of = |count: usize| format!("{count} of {}", self.samples);
        match &self.greeting {
            Some(habit) if self.greeting_rate >= 0.3 => out.push(format!(
                "Opens with \"{}\" ({}).",
                habit.text,
                of(habit.count)
            )),
            _ if self.greeting_rate < 0.2 => {
                out.push("Usually starts straight in, with no greeting.".to_string());
            }
            _ => {}
        }
        match &self.sign_off {
            Some(habit) if self.sign_off_rate >= 0.3 => out.push(format!(
                "Signs off with \"{}\" ({}).",
                habit.text.replace('\n', " / "),
                of(habit.count)
            )),
            _ if self.sign_off_rate < 0.2 => out.push("Usually has no sign-off.".to_string()),
            _ => {}
        }
        out.push(format!(
            "Typical length: about {} words.",
            self.median_words
        ));
        if self.contraction_rate >= 0.02 {
            out.push("Uses contractions (I'm, don't, we'll).".to_string());
        } else if self.samples >= 3 && self.contraction_rate == 0.0 {
            out.push("Avoids contractions.".to_string());
        }
        if self.lowercase_start_rate >= 0.5 {
            out.push("Often starts sentences in lowercase.".to_string());
        }
        if self.exclamations_per_message >= 0.8 {
            out.push("Uses exclamation marks freely.".to_string());
        } else if self.samples >= 3 && self.exclamations_per_message == 0.0 {
            out.push("Never uses exclamation marks.".to_string());
        }
        if self.uses_emoji {
            out.push("Sometimes uses emoji.".to_string());
        } else if self.samples >= 3 {
            out.push("Doesn't use emoji.".to_string());
        }
        out
    }
}

fn is_greeting(line: &str) -> bool {
    let words = line.split_whitespace().count();
    if words == 0 || words > 6 {
        return false;
    }
    let first = line
        .split(|c: char| !c.is_alphanumeric())
        .find(|word| !word.is_empty())
        .unwrap_or("")
        .to_lowercase();
    GREETING_WORDS.contains(&first.as_str())
        || (words <= 3 && (line.ends_with(',') || line.ends_with('!')) && first != "thanks")
}

/// The closing: a short last line, plus the short line before it when that
/// one ends like "Cheers," or "Best,".
fn sign_off(lines: &[&str]) -> Option<String> {
    let last = *lines.last()?;
    if last.split_whitespace().count() > 4
        || last.ends_with('?')
        || last.ends_with('.') && last.len() > 20
    {
        return None;
    }
    let before = lines.len().checked_sub(2).map(|index| lines[index]);
    match before {
        Some(line) if line.split_whitespace().count() <= 4 && line.ends_with(',') => {
            Some(format!("{line}\n{last}"))
        }
        _ if last.split_whitespace().count() <= 3 => Some(last.to_string()),
        _ => None,
    }
}

fn normalise(line: &str, first_names: &[String]) -> String {
    line.split(' ')
        .map(|word| {
            let bare = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if !bare.is_empty() && first_names.contains(&bare) {
                word.replacen(
                    word.trim_matches(|c: char| !c.is_alphanumeric()),
                    "{name}",
                    1,
                )
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn most_common(items: &[String]) -> Option<Habit> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for item in items {
        match counts
            .iter_mut()
            .find(|(text, _)| text.eq_ignore_ascii_case(item))
        {
            Some((_, count)) => *count += 1,
            None => counts.push((item.clone(), 1)),
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(text, count)| Habit { text, count })
}

fn is_contraction(token: &str) -> bool {
    let token = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '’');
    let lower = token.to_lowercase().replace('’', "'");
    ["n't", "'m", "'re", "'ll", "'ve", "'d"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
        || (lower.ends_with("'s")
            && [
                "it's", "that's", "what's", "there's", "here's", "let's", "he's", "she's",
            ]
            .contains(&lower.as_str()))
}

fn is_emoji(c: char) -> bool {
    matches!(u32::from(c), 0x1F300..=0x1FAFF | 0x2600..=0x27BF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_lowercase_writer() {
        let bodies = [
            "yep, works for me",
            "sounds good. i'll send it tonight\ns",
            "can't do thursday, friday?\ns",
            "done, it's in the drive\ns",
        ];
        let habits = analyse(&bodies, &["Alice Smith"]);
        let text = habits.describe().join("\n");
        assert!(text.contains("Usually starts straight in"), "{text}");
        assert!(text.contains("Signs off with \"s\""), "{text}");
        assert!(text.contains("Uses contractions"), "{text}");
        assert!(text.contains("lowercase"), "{text}");
        assert!(text.contains("Never uses exclamation marks"), "{text}");
    }

    #[test]
    fn formal_writer_with_named_greeting() {
        let bodies = [
            "Dear Robert,\nThank you for the update. I will review the contract by Friday.\nKind regards,\nSam Rivers",
            "Dear Robert,\nPlease find the revised figures attached.\nKind regards,\nSam Rivers",
            "Dear Robert,\nI am available on Tuesday at 10:00.\nKind regards,\nSam Rivers",
        ];
        let habits = analyse(&bodies, &["Robert Jones"]);
        assert_eq!(
            habits.greeting.as_ref().map(|h| h.text.as_str()),
            Some("Dear {name},")
        );
        assert_eq!(
            habits.sign_off.as_ref().map(|h| h.text.as_str()),
            Some("Kind regards,\nSam Rivers")
        );
        let text = habits.describe().join("\n");
        assert!(
            text.contains("Opens with \"Dear {name},\" (3 of 3)"),
            "{text}"
        );
        assert!(
            text.contains("Signs off with \"Kind regards, / Sam Rivers\""),
            "{text}"
        );
        assert!(text.contains("Avoids contractions"), "{text}");
    }

    #[test]
    fn cleaning_drops_quotes_and_signature_block() {
        let raw = "Sounds good!\n\nCheers,\nSam\n-- \nSam Rivers | Engines Ltd\n\nOn Mon, Alice wrote:\n> old text";
        let cleaned = clean_for_voice(raw);
        assert!(cleaned.contains("Cheers,\nSam"), "{cleaned}");
        assert!(!cleaned.contains("Engines Ltd"), "{cleaned}");
        assert!(!cleaned.contains("old text"), "{cleaned}");
    }

    #[test]
    fn nothing_to_go_on() {
        assert!(analyse(&[], &[]).describe().is_empty());
    }
}
