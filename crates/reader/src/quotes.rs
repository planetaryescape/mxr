use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct QuotedBlock {
    pub from: Option<String>,
    pub date: Option<String>,
    pub content: String,
}

static ON_WROTE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^On .+wrote:\s*$").expect("quoted-reply regex literal should compile")
});

static QUOTE_PREFIX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^>+\s?").expect("quote-prefix regex literal should compile"));

/// Collapse quoted replies into summary blocks.
/// Returns (cleaned_text, extracted_quotes).
pub fn collapse(text: &str) -> (String, Vec<QuotedBlock>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut quotes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Check for "On {date}, {person} wrote:" pattern
        if ON_WROTE.is_match(lines[i]) {
            let header = lines[i];
            let from = extract_from_on_wrote(header);
            let mut quote_lines = Vec::new();
            i += 1;

            while i < lines.len() {
                if QUOTE_PREFIX.is_match(lines[i]) {
                    let stripped = QUOTE_PREFIX.replace(lines[i], "").to_string();
                    quote_lines.push(stripped);
                    i += 1;
                } else if lines[i].trim().is_empty()
                    && i + 1 < lines.len()
                    && QUOTE_PREFIX.is_match(lines[i + 1])
                {
                    quote_lines.push(String::new());
                    i += 1;
                } else {
                    break;
                }
            }

            let label = if let Some(ref f) = from {
                format!("[previous message from {f}]")
            } else {
                "[previous message]".to_string()
            };
            result.push(label);

            quotes.push(QuotedBlock {
                from,
                date: None,
                content: quote_lines.join("\n"),
            });
            continue;
        }

        // Check for standalone > quoted blocks
        if QUOTE_PREFIX.is_match(lines[i]) {
            let mut quote_lines = Vec::new();
            while i < lines.len() && (QUOTE_PREFIX.is_match(lines[i]) || lines[i].trim().is_empty())
            {
                if QUOTE_PREFIX.is_match(lines[i]) {
                    let stripped = QUOTE_PREFIX.replace(lines[i], "").to_string();
                    quote_lines.push(stripped);
                } else {
                    // Only include blank lines if more quoted lines follow
                    if i + 1 < lines.len() && QUOTE_PREFIX.is_match(lines[i + 1]) {
                        quote_lines.push(String::new());
                    } else {
                        break;
                    }
                }
                i += 1;
            }

            result.push("[previous message]".to_string());
            quotes.push(QuotedBlock {
                from: None,
                date: None,
                content: quote_lines.join("\n"),
            });
            continue;
        }

        result.push(lines[i].to_string());
        i += 1;
    }

    (result.join("\n"), quotes)
}

fn extract_from_on_wrote(header: &str) -> Option<String> {
    // "wrote:" is ASCII, so locate it as a byte offset into the ORIGINAL
    // header. `to_lowercase()` can change byte length (e.g. `İ` U+0130 grows
    // when lowercased), so an offset taken from a lowercased copy is not a
    // valid index into `header` and slicing there can panic. ASCII byte
    // positions are always char boundaries, so slicing at the match start is
    // safe and preserves the name's original case.
    const NEEDLE: &[u8] = b"wrote:";
    if let Some(wrote_pos) = header
        .as_bytes()
        .windows(NEEDLE.len())
        .rposition(|w| w.eq_ignore_ascii_case(NEEDLE))
    {
        let before = header[..wrote_pos].trim();
        if let Some(last_comma) = before.rfind(',') {
            let candidate = before[last_comma + 1..].trim();
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_on_wrote_block() {
        let text = "Thanks for the update.\n\nOn Mon, Mar 15, alice@example.com wrote:\n> Original message here\n> Second line";
        let (cleaned, quotes) = collapse(text);
        assert!(cleaned.contains("[previous message from alice@example.com]"));
        assert_eq!(quotes.len(), 1);
        assert!(quotes[0].content.contains("Original message here"));
    }

    #[test]
    fn collapses_bare_quotes() {
        let text = "My reply.\n\n> Some quoted text\n> More quoted text";
        let (cleaned, quotes) = collapse(text);
        assert!(cleaned.contains("[previous message]"));
        assert_eq!(quotes.len(), 1);
    }

    #[test]
    fn preserves_non_quoted_text() {
        let text = "First line.\nSecond line.\nThird line.";
        let (cleaned, quotes) = collapse(text);
        assert_eq!(cleaned, text);
        assert!(quotes.is_empty());
    }

    #[test]
    fn extract_from_on_wrote_ascii_unchanged() {
        let name = extract_from_on_wrote("On Mon, Mar 15, alice@example.com wrote:");
        assert_eq!(name.as_deref(), Some("alice@example.com"));
    }

    #[test]
    fn extract_from_on_wrote_multibyte_does_not_panic() {
        // `İ` (U+0130) lowercases to two chars, growing the byte length. A byte
        // offset taken from a lowercased copy is not a valid index into the
        // original header, so the old code panicked here. Must not panic and
        // must return the name in its original case.
        let header = format!("On Mon, {} wrote:", "İ".repeat(7));
        let name = extract_from_on_wrote(&header);
        assert_eq!(name.as_deref(), Some("İİİİİİİ"));
    }

    #[test]
    fn collapse_handles_multibyte_on_wrote_header() {
        let text = format!("Reply.\n\nOn Mon, {} wrote:\n> quoted line", "İ".repeat(7));
        let (cleaned, quotes) = collapse(&text);
        assert!(cleaned.contains("[previous message from"));
        assert_eq!(quotes.len(), 1);
    }
}
