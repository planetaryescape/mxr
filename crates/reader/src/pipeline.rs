use crate::{boilerplate, html, quotes, signatures, tracking};

/// Configuration for the reader pipeline.
#[derive(Debug, Clone)]
pub struct ReaderConfig {
    /// External command for HTML-to-text conversion (e.g., "w3m -T text/html -dump").
    /// If None, uses built-in html2text.
    pub html_command: Option<String>,
    /// Whether to strip signatures.
    pub strip_signatures: bool,
    /// Whether to collapse quoted replies.
    pub collapse_quotes: bool,
    /// Whether to strip boilerplate/disclaimers.
    pub strip_boilerplate: bool,
    /// Whether to strip tracking/footer junk.
    pub strip_tracking: bool,
}

impl Default for ReaderConfig {
    fn default() -> Self {
        Self {
            html_command: None,
            strip_signatures: true,
            collapse_quotes: true,
            strip_boilerplate: true,
            strip_tracking: true,
        }
    }
}

/// Output from the reader pipeline.
#[derive(Debug, Clone)]
pub struct ReaderOutput {
    /// Cleaned content: just the human-written text.
    pub content: String,
    /// Quoted messages that were stripped (available for expansion).
    pub quoted_messages: Vec<quotes::QuotedBlock>,
    /// The signature that was stripped.
    pub signature: Option<String>,
    /// Line count stats for UI display.
    pub original_lines: usize,
    pub cleaned_lines: usize,
}

/// Run the full reader pipeline on a message body.
///
/// Accepts either plain text or HTML. If HTML, converts to plain text first.
pub fn clean(text: Option<&str>, html: Option<&str>, config: &ReaderConfig) -> ReaderOutput {
    // 1. Resolve to plain text
    let raw = match (text, html) {
        (Some(t), _) => t.to_string(),
        (None, Some(h)) => html::to_plain_text(h, config),
        (None, None) => String::new(),
    };

    let original_lines = raw.lines().count();
    let mut content = raw;
    let mut quoted_messages = Vec::new();
    let mut signature = None;

    // 2. Extract and collapse quoted replies
    if config.collapse_quotes {
        let (cleaned, q) = quotes::collapse(&content);
        content = cleaned;
        quoted_messages = q;
    }

    // 3. Strip signatures
    if config.strip_signatures {
        let (cleaned, sig) = signatures::strip(&content);
        content = cleaned;
        signature = sig;
    }

    // 4. Strip boilerplate
    if config.strip_boilerplate {
        content = boilerplate::strip(&content);
    }

    // 5. Strip tracking junk
    if config.strip_tracking {
        content = tracking::strip(&content);
    }

    // 6. Clean up excessive whitespace
    content = normalize_whitespace(&content);

    let cleaned_lines = content.lines().count();

    ReaderOutput {
        content,
        quoted_messages,
        signature,
        original_lines,
        cleaned_lines,
    }
}

fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut blank_count = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_email_with_signature() {
        let text =
            "Hey,\n\nCan we meet tomorrow at 3pm?\n\nThanks,\n-- \nAlice\nSenior Engineer\n+1 555-0123\nalice@company.com";
        let output = clean(Some(text), None, &ReaderConfig::default());
        assert_eq!(
            output.content.trim(),
            "Hey,\n\nCan we meet tomorrow at 3pm?\n\nThanks,"
        );
        assert!(output.signature.is_some());
    }

    #[test]
    fn reader_mode_stats_correct() {
        let text = "Content here.\n\nOn Mon, alice wrote:\n> Long quote\n> Another line\n> And more\n\n-- \nSig line\nPhone: 555-0123";
        let output = clean(Some(text), None, &ReaderConfig::default());
        assert!(output.original_lines > output.cleaned_lines);
    }

    #[test]
    fn empty_input_returns_empty() {
        let output = clean(None, None, &ReaderConfig::default());
        assert!(output.content.is_empty());
        assert_eq!(output.original_lines, 0);
        assert_eq!(output.cleaned_lines, 0);
    }

    #[test]
    fn html_preferred_over_none() {
        let html = "<p>Hello world</p>";
        let output = clean(None, Some(html), &ReaderConfig::default());
        assert!(output.content.contains("Hello world"));
    }

    #[test]
    fn text_preferred_over_html() {
        let text = "Plain text version";
        let html = "<p>HTML version</p>";
        let output = clean(Some(text), Some(html), &ReaderConfig::default());
        assert!(output.content.contains("Plain text"));
    }

    #[test]
    fn config_disables_stripping() {
        let text = "Content.\n-- \nMy Signature";
        let config = ReaderConfig {
            strip_signatures: false,
            ..Default::default()
        };
        let output = clean(Some(text), None, &config);
        assert!(output.content.contains("My Signature"));
        assert!(output.signature.is_none());
    }
}
