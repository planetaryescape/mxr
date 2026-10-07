//! What one message says that its thread didn't already: the body minus
//! quoted history and signature, for Messages and every client that shows
//! "what they wrote".
//!
//! Syntax alone misses most quotes in business mail: a line-initial `>`
//! finds under 10% of reply lines in the Enron corpus (Lampert, Dale and
//! Paris, EMNLP 2009). mxr has the whole thread locally, and a quote is by
//! definition text an earlier message in the thread already holds, so a
//! trailing block is matched against the earlier messages (word shingles,
//! so re-wrapping and `>` prefixes don't matter). Client markers (HTML
//! classes, `>` prefixes, "On ... wrote:", Outlook's header block) then
//! only have to find the boundary.
//!
//! Every removal is reported, so clients can say "trimmed: quote, sig" and
//! offer the message as sent.

use crate::html_quote::split_html_quote;
use crate::pipeline::ReaderConfig;
use crate::{boilerplate, html, signatures, tracking};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

/// Words per shingle when matching against earlier messages.
const SHINGLE: usize = 4;
/// Share of a long line's shingles an earlier message must hold for the
/// line to count as quoted.
const LINE_MATCH_SHARE: f64 = 0.6;
/// Quoted lines a trailing block needs before it is cut without a header.
const MIN_MATCHED_LINES: usize = 2;
/// Lines from the end of an author's earlier messages that their signature
/// is looked for in.
const SIGNATURE_TAIL_LINES: usize = 8;

static ON_WROTE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:>\s*)*on\s.+\bwrote:\s*$").expect("on-wrote regex should compile")
});
static QUOTE_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*>").expect("quote-prefix regex should compile"));
static ORIGINAL_MESSAGE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*-{2,}\s*(original message|forwarded message|reply message)\s*-{2,}\s*$")
        .expect("original-message regex should compile")
});
static HEADER_FROM: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*\*?from:\*?\s+\S").expect("from-header regex"));
static HEADER_FOLLOW: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*\*?(sent|date|to|subject|cc):\*?\s").expect("header-follow regex")
});

/// An earlier message in the same thread, as plain text with its own
/// quotes still in (see [`plain_text`]).
#[derive(Debug, Clone, Copy)]
pub struct EarlierMessage<'a> {
    pub text: &'a str,
    /// The same person wrote it, so a repeated tail is their signature.
    pub same_author: bool,
}

/// What `new_text` removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trimmed {
    pub quote: bool,
    pub signature: bool,
}

impl Trimmed {
    pub fn any(self) -> bool {
        self.quote || self.signature
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewText {
    pub text: String,
    pub trimmed: Trimmed,
}

/// A body as plain text, everything kept: what earlier messages are
/// matched as.
pub fn plain_text(text: Option<&str>, html_body: Option<&str>) -> String {
    let config = ReaderConfig::default();
    match (text, html_body) {
        (Some(t), _) if !t.trim().is_empty() && !html::looks_like_html_document(t) => t.to_string(),
        (Some(t), _) if !t.trim().is_empty() => html::to_plain_text(t, &config),
        (_, Some(h)) => html::to_plain_text(h, &config),
        _ => String::new(),
    }
}

/// The new text of one message, given the earlier messages of its thread
/// (any order).
pub fn new_text(
    text: Option<&str>,
    html_body: Option<&str>,
    earlier: &[EarlierMessage<'_>],
) -> NewText {
    let config = ReaderConfig::default();
    let mut trimmed = Trimmed::default();

    // HTML markers only survive while the body is HTML: use the HTML when
    // it carries them, else the plain part.
    let html_part = html_body
        .or_else(|| text.filter(|t| html::looks_like_html_document(t)))
        .map(split_html_quote);
    let raw = match &html_part {
        Some(parts) if parts.has_quote || parts.has_signature => {
            trimmed.quote |= parts.has_quote;
            trimmed.signature |= parts.has_signature;
            html::to_plain_text(&parts.main, &config)
        }
        _ => plain_text(text, html_body),
    };

    let (body, quoted) = strip_marked_quotes(&raw);
    trimmed.quote |= quoted;
    let (body, matched) = strip_thread_quote(&body, earlier);
    trimmed.quote |= matched;

    let (mut body, signature) = signatures::strip(&body);
    trimmed.signature |= signature.is_some_and(|sig| !sig.trim().is_empty());
    let (repeated, removed) = strip_repeated_tail(&body, earlier);
    if removed {
        body = repeated;
        trimmed.signature = true;
    }
    let body = tracking::strip(&boilerplate::strip(&body));
    let body = normalize_whitespace(&body);

    if body.is_empty() {
        // All quote (a bare forward) or nothing at all: the message as it
        // came is better than an empty one.
        return NewText {
            text: normalize_whitespace(&raw),
            trimmed: Trimmed::default(),
        };
    }
    NewText {
        text: body,
        trimmed,
    }
}

/// Remove `>`-prefixed lines, the "On ... wrote:" line heading them, and
/// everything from a strong quote header ("-----Original Message-----", an
/// Outlook "From: / Sent:" block) on. Inline replies between quoted lines
/// stay.
fn strip_marked_quotes(text: &str) -> (String, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let cut = lines
        .iter()
        .enumerate()
        .find(|(index, line)| is_strong_header(&lines, *index, line))
        .map_or(lines.len(), |(index, _)| index);
    let mut removed = cut < lines.len();
    let mut kept = Vec::with_capacity(cut);
    for line in &lines[..cut] {
        if QUOTE_PREFIX.is_match(line) || ON_WROTE.is_match(line) {
            removed = true;
            continue;
        }
        kept.push(*line);
    }
    (kept.join("\n"), removed)
}

fn is_strong_header(lines: &[&str], index: usize, line: &str) -> bool {
    if ORIGINAL_MESSAGE.is_match(line) {
        return true;
    }
    if ON_WROTE.is_match(line) {
        // An "On ... wrote:" with no `>` lines after it heads a quote that
        // runs to the end (Gmail plain text without prefixes).
        let next = lines[index + 1..]
            .iter()
            .find(|line| !line.trim().is_empty());
        return next.is_none_or(|next| !QUOTE_PREFIX.is_match(next));
    }
    HEADER_FROM.is_match(line)
        && lines[index + 1..]
            .iter()
            .take(4)
            .filter(|line| HEADER_FOLLOW.is_match(line))
            .count()
            >= 2
}

/// The words of `text`, lowercased, punctuation and `>` gone.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn shingle_hash(words: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    words.hash(&mut hasher);
    hasher.finish()
}

struct EarlierIndex {
    shingles: HashSet<u64>,
    short_lines: HashSet<String>,
}

impl EarlierIndex {
    fn new(earlier: &[EarlierMessage<'_>]) -> Self {
        let mut shingles = HashSet::new();
        let mut short_lines = HashSet::new();
        for message in earlier {
            let all = words(message.text);
            for window in all.windows(SHINGLE) {
                shingles.insert(shingle_hash(window));
            }
            for line in message.text.lines() {
                let line_words = words(line);
                if !line_words.is_empty() && line_words.len() < SHINGLE {
                    short_lines.insert(line_words.join(" "));
                }
            }
        }
        Self {
            shingles,
            short_lines,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineClass {
    Blank,
    Header,
    /// A line long enough to judge: quoted or not.
    Long(bool),
    /// Too short to judge alone ("Thanks,", "Sam").
    Short(bool),
}

fn classify(line: &str, index: &EarlierIndex) -> LineClass {
    if line.trim().is_empty() {
        return LineClass::Blank;
    }
    if ON_WROTE.is_match(line) || ORIGINAL_MESSAGE.is_match(line) || HEADER_FOLLOW.is_match(line) {
        return LineClass::Header;
    }
    let line_words = words(line);
    if line_words.len() < SHINGLE {
        return LineClass::Short(index.short_lines.contains(&line_words.join(" ")));
    }
    let windows = line_words.windows(SHINGLE);
    let total = windows.len();
    let hits = windows
        .filter(|window| index.shingles.contains(&shingle_hash(window)))
        .count();
    LineClass::Long(hits as f64 >= total as f64 * LINE_MATCH_SHARE)
}

/// Cut a trailing block whose lines earlier messages already hold. One
/// stray unmatched line inside the block is tolerated (a header the
/// earlier message never had, a wrapped fragment); two end it.
fn strip_thread_quote(text: &str, earlier: &[EarlierMessage<'_>]) -> (String, bool) {
    if earlier.is_empty() {
        return (text.to_string(), false);
    }
    let index = EarlierIndex::new(earlier);
    let lines: Vec<&str> = text.lines().collect();
    let mut start = None;
    let mut matched = 0usize;
    let mut header = false;
    let mut misses = 0usize;
    for (at, line) in lines.iter().enumerate().rev() {
        match classify(line, &index) {
            LineClass::Blank | LineClass::Short(true) => {}
            LineClass::Header => {
                header = true;
                start = Some(at);
                misses = 0;
            }
            LineClass::Long(true) => {
                matched += 1;
                start = Some(at);
                misses = 0;
            }
            LineClass::Long(false) | LineClass::Short(false) => {
                misses += 1;
                if misses >= 2 {
                    break;
                }
            }
        }
    }
    let enough = matched >= MIN_MATCHED_LINES || (matched >= 1 && header);
    match start {
        Some(start) if enough => (lines[..start].join("\n"), true),
        _ => (text.to_string(), false),
    }
}

/// The author's signature, found as the tail their earlier messages also
/// end with. Needs three lines, or two when one looks like contact
/// details, so a plain "Thanks, / Sam" sign-off stays.
fn strip_repeated_tail(text: &str, earlier: &[EarlierMessage<'_>]) -> (String, bool) {
    let mut tails: HashSet<String> = HashSet::new();
    for message in earlier.iter().filter(|m| m.same_author) {
        let lines: Vec<&str> = message
            .text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        for line in lines.iter().rev().take(SIGNATURE_TAIL_LINES) {
            tails.insert(line.trim().to_lowercase());
        }
    }
    if tails.is_empty() {
        return (text.to_string(), false);
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut cut = lines.len();
    let mut taken = Vec::new();
    for (at, line) in lines.iter().enumerate().rev() {
        if line.trim().is_empty() {
            continue;
        }
        if tails.contains(&line.trim().to_lowercase()) {
            cut = at;
            taken.push(*line);
        } else {
            break;
        }
    }
    let contact_like = taken
        .iter()
        .any(|line| line.contains('@') || line.contains("http") || line.contains("www."));
    let has_body = lines[..cut].iter().any(|line| !line.trim().is_empty());
    if has_body && (taken.len() >= 3 || (taken.len() >= 2 && contact_like)) {
        (lines[..cut].join("\n"), true)
    } else {
        (text.to_string(), false)
    }
}

fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blanks = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 1 {
                out.push('\n');
            }
        } else {
            blanks = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests;
