//! The headline an issue is shown under: its subject with the newsletter's
//! own boilerplate removed ("[AINews]", "Issue #42 |", the source's name,
//! emoji). A cleaned headline that comes out too short falls back to the
//! subject, because a bad headline is worse than the subject.

use once_cell::sync::Lazy;
use regex::Regex;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("headline regex literals compile")
}

/// "[AINews] ", "(Weekly) ".
static LEADING_TAG: Lazy<Regex> = Lazy::new(|| regex(r"^\s*[\[(][^\])]{1,40}[\])]\s*[:|\-–·]?\s*"));
/// "Issue #42 | ", "No. 7: ", "Vol. 3, Issue 7 - ", "#123: ", "Week 41 · ".
static LEADING_NUMBER: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)^\s*(?:(?:issue|edition|no\.?|vol\.?|volume|week|episode|ep\.?|part)\s*#?\s*\d+[\w.]*(?:\s*,\s*(?:issue|no\.?)\s*#?\d+)?|#\d+)\s*[:|\-–—·]\s*",
    )
});
/// "This week in Rust: ", "Today in Tech - ".
static LEADING_ROUNDUP: Lazy<Regex> = Lazy::new(|| {
    regex(
        r"(?i)^\s*(?:(?:this week|today|this month) in [^:|\-–—]{1,30}|new (?:post|essay|article|episode|issue|video)|now live|just published)\s*[:|\-–—]\s+",
    )
});

/// Pictographs, dingbats and the joiners that glue them together.
fn is_emoji(c: char) -> bool {
    matches!(
        c as u32,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0xFE0F | 0x200D | 0x20E3
    )
}

fn trim_decoration(text: &str) -> &str {
    text.trim_matches(|c: char| {
        c.is_whitespace() || is_emoji(c) || matches!(c, '|' | '-' | '–' | '—' | '·' | ':')
    })
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The headline for an issue with `subject` from `source` (the
/// publication's name, when known).
pub fn clean_headline(subject: &str, source: Option<&str>) -> String {
    let original = collapse_spaces(subject);
    let mut text = original.clone();
    for _ in 0..3 {
        let before = text.clone();
        text = LEADING_TAG.replace(&text, "").into_owned();
        text = LEADING_NUMBER.replace(&text, "").into_owned();
        text = LEADING_ROUNDUP.replace(&text, "").into_owned();
        if let Some(source) = source.map(str::trim).filter(|s| s.len() >= 3) {
            text = strip_source(&text, source);
        }
        text = trim_decoration(&text).to_string();
        if text == before {
            break;
        }
    }
    let text = collapse_spaces(&text);
    // Only a sentence that lost its prefix gets a capital: "iPhone" stays.
    let text = if text == original {
        text
    } else {
        capitalize_first(&text)
    };
    if text.chars().filter(|c| c.is_alphanumeric()).count() < 4 {
        let fallback = collapse_spaces(trim_decoration(&original));
        return if fallback.is_empty() {
            original
        } else {
            fallback
        };
    }
    text
}

/// What was left after a prefix came off starts a sentence.
pub(crate) fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// "Platform Weekly: Shipping a sync engine" and "Shipping a sync engine |
/// Platform Weekly" both lose the source's name.
fn strip_source(text: &str, source: &str) -> String {
    let lower = text.to_lowercase();
    let wanted = source.to_lowercase();
    if lower.starts_with(&wanted) {
        let rest = &text[wanted.len()..];
        let next = rest.trim_start();
        if next.starts_with([':', '|', '-', '–', '—', '·']) || LEADING_NUMBER.is_match(next) {
            return rest.to_string();
        }
    }
    if lower.ends_with(&wanted) {
        let head = &text[..text.len() - wanted.len()];
        if head.trim_end().ends_with(['|', '-', '–', '—', '·', ':']) {
            return head.to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::clean_headline;

    #[test]
    fn newsletter_boilerplate_comes_off_the_subject() {
        let cases = [
            (
                "[AINews] Reflection ships an open model",
                None,
                "Reflection ships an open model",
            ),
            (
                "Issue #42 | The quiet death of the three-pane layout",
                None,
                "The quiet death of the three-pane layout",
            ),
            ("#563: SQLite 3.51 is out", None, "SQLite 3.51 is out"),
            (
                "Vol. 3, Issue 7 - Shipping a sync engine",
                None,
                "Shipping a sync engine",
            ),
            (
                "Platform Weekly: Shipping a sync engine in 2026",
                Some("Platform Weekly"),
                "Shipping a sync engine in 2026",
            ),
            (
                "Shipping a sync engine in 2026 | Platform Weekly",
                Some("Platform Weekly"),
                "Shipping a sync engine in 2026",
            ),
            (
                "🚀 Local-first mail is having a moment 🎉",
                None,
                "Local-first mail is having a moment",
            ),
            (
                "This week in Rust: async closures land",
                None,
                "Async closures land",
            ),
            (
                "New post: Shipping a sync engine in 2026",
                None,
                "Shipping a sync engine in 2026",
            ),
            (
                "Local-first Links #41: six things worth your time",
                Some("Local-first Links"),
                "Six things worth your time",
            ),
        ];
        for (subject, source, want) in cases {
            assert_eq!(clean_headline(subject, source), want, "{subject}");
        }
    }

    #[test]
    fn a_headline_that_would_be_empty_falls_back_to_the_subject() {
        assert_eq!(
            clean_headline("[Platform Weekly] #12", None),
            "[Platform Weekly] #12"
        );
        assert_eq!(clean_headline("Issue #4", None), "Issue #4");
        assert_eq!(clean_headline("  ", None), "");
    }

    #[test]
    fn plain_subjects_are_left_alone() {
        for subject in [
            "Why sync engines need tombstones",
            "The 5-minute guide to WAL mode",
            "Rust 1.95: what changed",
            "iPhone 18 review",
        ] {
            assert_eq!(clean_headline(subject, Some("Some Source")), subject);
        }
    }
}
