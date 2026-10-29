use crate::html::{html_to_match_text, looks_like_html_document};
use crate::pipeline::normalize_whitespace;

/// The text of a message that search and rules match against, joined.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchableText {
    pub text: String,
    /// Parts whose conversion panicked and were replaced by tag-stripped text.
    pub fallbacks: usize,
}

/// The matchable parts of a message: `text_plain` as stored, and `text_html`
/// converted to text. Each part is its own string, so a rule can match one
/// part whole (`^invoice$`, exact equality) without the other part's words
/// joining it. When both parts are identical, one copy is kept. A part that
/// is itself an HTML document is converted rather than matched as markup.
///
/// Conversion renders without display wrapping (see `html_to_match_text`), so
/// a line break in the reader's 80-column view is not a break in the words.
pub fn searchable_parts(text_plain: Option<&str>, text_html: Option<&str>) -> Vec<String> {
    searchable_parts_counted(text_plain, text_html).0
}

/// The matchable parts joined with newlines, for the search index, plus the
/// number of parts that fell back to tag stripping.
pub fn searchable_text(text_plain: Option<&str>, text_html: Option<&str>) -> SearchableText {
    let (parts, fallbacks) = searchable_parts_counted(text_plain, text_html);
    SearchableText {
        text: parts.join("\n"),
        fallbacks,
    }
}

fn searchable_parts_counted(
    text_plain: Option<&str>,
    text_html: Option<&str>,
) -> (Vec<String>, usize) {
    let mut fallbacks = 0;
    let mut parts: Vec<String> = Vec::with_capacity(2);
    for (source, is_html) in [(text_plain, false), (text_html, true)] {
        let Some(source) = source else { continue };
        let (text, fell_back) = match_text(source, is_html);
        fallbacks += usize::from(fell_back);
        if text.trim().is_empty() || parts.contains(&text) {
            continue;
        }
        parts.push(text);
    }
    (parts, fallbacks)
}

/// The matching text of one source. `is_html` marks the HTML part; a plain
/// part that is itself an HTML document is converted the same way.
fn match_text(source: &str, is_html: bool) -> (String, bool) {
    if is_html || looks_like_html_document(source) {
        let (text, fell_back) = html_to_match_text(source, "searchable_text");
        (normalize_whitespace(&text), fell_back)
    } else {
        (normalize_whitespace(source), false)
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
    fn long_html_paragraphs_are_not_broken_at_display_width() {
        // Reproduces a rule on "payment overdue": at 80 columns the reader
        // breaks before "overdue", which must not reach the matching text.
        let html = format!("<p>{} payment overdue</p>", "a".repeat(72));
        let parts = searchable_parts(None, Some(&html));
        assert_eq!(parts.len(), 1);
        assert!(parts[0].contains("payment overdue"), "got {:?}", parts[0]);
    }

    #[test]
    fn identical_plain_and_html_parts_are_kept_once() {
        assert_eq!(
            searchable_parts(Some("invoice"), Some("<p>invoice</p>")),
            vec!["invoice".to_string()]
        );
    }

    #[test]
    fn differing_parts_stay_separate_and_joined_text_is_their_join() {
        let parts = searchable_parts(Some("plain one"), Some("<p>html two</p>"));
        assert_eq!(parts, vec!["plain one".to_string(), "html two".to_string()]);
        assert_eq!(
            searchable_text(Some("plain one"), Some("<p>html two</p>")).text,
            "plain one\nhtml two"
        );
    }

    #[test]
    fn tables_render_at_the_match_width_without_padding_or_fallback() {
        let (text, fell_back) = crate::html::html_to_match_text(
            "<table><tr><td>one</td><td>two</td></tr></table>",
            "test",
        );
        assert!(!fell_back);
        assert!(text.contains("one") && text.contains("two"), "got {text:?}");
        assert!(
            text.len() < 200,
            "padded to the match width: {} bytes",
            text.len()
        );
    }

    #[test]
    fn a_huge_colspan_still_renders_through_the_clamp() {
        // Unclamped, html2text panics on this span and the parts fall back.
        let html = r#"<table><tr><td colspan="18446744073709551615">A</td><td>B</td></tr></table>"#;
        let (parts, fallbacks) = searchable_parts_counted(None, Some(html));
        assert_eq!(fallbacks, 0);
        let text = parts.join(" ");
        assert!(text.contains('A') && text.contains('B'), "got {text:?}");
    }
}
