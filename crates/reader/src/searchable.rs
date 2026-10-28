use crate::html::{guard_html2text, strip_html_tags};
use crate::pipeline::{clean, ReaderConfig};

/// The text of a message that search and rules match against.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchableText {
    pub text: String,
    /// Parts whose conversion panicked and were replaced by tag-stripped text.
    pub fallbacks: usize,
}

/// The reader's text for a message with every removal step off, so quoted
/// replies, signatures and boilerplate stay in: each removal can drop words
/// the reader shows. Markup, CSS and scripts are dropped by the conversion.
/// The built-in converter is always used, never `render.html_command`.
fn conversion_only_config() -> ReaderConfig {
    ReaderConfig {
        html_command: None,
        collapse_quotes: false,
        strip_signatures: false,
        strip_boilerplate: false,
        strip_tracking: false,
    }
}

/// The text search indexes and rules match: `text_plain` as stored, and
/// `text_html` converted to text, joined with newlines. A part that is itself
/// an HTML document is converted rather than matched as markup. Both paths
/// (index and rules preview or apply) call this, so they see the same words.
pub fn searchable_text(text_plain: Option<&str>, text_html: Option<&str>) -> SearchableText {
    let config = conversion_only_config();
    let mut parts = Vec::with_capacity(2);
    let mut fallbacks = 0;
    if let Some(plain) = text_plain {
        let (text, fell_back) =
            convert_or_strip(plain, || clean(Some(plain), None, &config).content);
        parts.push(text);
        fallbacks += usize::from(fell_back);
    }
    if let Some(html) = text_html {
        let (text, fell_back) = convert_or_strip(html, || clean(None, Some(html), &config).content);
        parts.push(text);
        fallbacks += usize::from(fell_back);
    }
    parts.retain(|part| !part.trim().is_empty());
    SearchableText {
        text: parts.join("\n"),
        fallbacks,
    }
}

/// Runs one conversion. A panic inside it (html2text panics on some malformed
/// tables) gives the tag-stripped source instead, and reports the fallback.
fn convert_or_strip(source: &str, convert: impl FnOnce() -> String) -> (String, bool) {
    match guard_html2text("searchable_text", convert) {
        Some(text) => (text, false),
        None => (strip_html_tags(source), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_contents_are_not_searchable_text() {
        let html = "<html><head><style>.x{color:red} zzcssword{a:b}</style></head>\
                    <body><p>Visible sentence</p></body></html>";
        let text = searchable_text(None, Some(html)).text;
        assert!(text.contains("Visible sentence"));
        assert!(!text.contains("zzcssword"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn html_visible_text_is_searchable_when_the_plain_part_lacks_it() {
        let text = searchable_text(
            Some("Plain part without the word"),
            Some("<p>Only the html has zzhtmlword here</p>"),
        )
        .text;
        assert!(text.contains("zzhtmlword"));
        assert!(text.contains("Plain part"));
    }

    #[test]
    fn plain_text_passes_through_and_empty_parts_are_dropped() {
        assert_eq!(
            searchable_text(Some("hello there"), None).text,
            "hello there"
        );
        assert_eq!(searchable_text(Some("  "), Some("")).text, "");
        assert_eq!(searchable_text(None, None), SearchableText::default());
    }

    #[test]
    fn a_panicking_conversion_falls_back_to_tag_stripped_text_and_is_reported() {
        let source = "<p>keepme</p><script>var hidden=1;</script><b>also</b>";
        let (text, fell_back) = convert_or_strip(source, || {
            std::panic::resume_unwind(Box::new("converter failed"))
        });

        assert!(fell_back);
        assert!(text.contains("keepme"));
        assert!(text.contains("also"));
        assert!(!text.contains("hidden"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn a_normal_conversion_is_not_reported_as_a_fallback() {
        let (text, fell_back) = convert_or_strip("<p>fine</p>", || "fine".to_string());
        assert_eq!(text, "fine");
        assert!(!fell_back);
    }
}
