use crate::html::{html_to_match_text, looks_like_html_document};
use crate::pipeline::normalize_whitespace;

/// The text of a message that the search index holds, joined.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchableText {
    pub text: String,
    /// Parts whose conversion panicked and were replaced by tag-stripped text.
    pub fallbacks: usize,
}

/// The parts a body rule matches.
///
/// Part 0 is `text_plain` exactly as stored: rules that matched plain mail
/// before keep their exact results (no trim, no normalisation, and an empty
/// `Some("")` is still a part). Part 1 is the HTML part converted to text with
/// every run of whitespace, newlines included, collapsed to one space, so a
/// phrase the reader wraps across lines still matches as one phrase.
///
/// `None` only when the message has neither part, which rules report as an
/// unknown body, as they did before HTML was matched.
pub fn body_match_parts(text_plain: Option<&str>, text_html: Option<&str>) -> Option<Vec<String>> {
    if text_plain.is_none() && text_html.is_none() {
        return None;
    }
    let mut parts = Vec::with_capacity(2);
    if let Some(plain) = text_plain {
        parts.push(plain.to_string());
    }
    if let Some(html) = text_html {
        parts.push(html_match_part(html).0);
    }
    Some(parts)
}

/// The text the search index holds for a message: the plain part and the
/// converted HTML part, each normalised and blank parts dropped, joined with
/// newlines. A plain part that is itself an HTML document is converted rather
/// than indexed as markup.
pub fn searchable_text(text_plain: Option<&str>, text_html: Option<&str>) -> SearchableText {
    let mut fallbacks = 0;
    let mut parts: Vec<String> = Vec::with_capacity(2);
    if let Some(plain) = text_plain {
        let text = if looks_like_html_document(plain) {
            let (text, fell_back) = html_match_part(plain);
            fallbacks += usize::from(fell_back);
            text
        } else {
            normalize_whitespace(plain)
        };
        parts.push(text);
    }
    if let Some(html) = text_html {
        let (text, fell_back) = html_match_part(html);
        fallbacks += usize::from(fell_back);
        parts.push(text);
    }
    parts.retain(|part| !part.trim().is_empty());
    SearchableText {
        text: parts.join("\n"),
        fallbacks,
    }
}

/// HTML converted to text with whitespace collapsed to single spaces. The
/// output is never larger than the converter's output, which is bounded by
/// the input (the reader's width caps each line).
fn html_match_part(html: &str) -> (String, bool) {
    let (text, fell_back) = html_to_match_text(html, "searchable_text");
    (
        text.split_whitespace().collect::<Vec<_>>().join(" "),
        fell_back,
    )
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
    fn plain_text_passes_through_and_blank_parts_are_dropped() {
        assert_eq!(
            searchable_text(Some("hello there"), None).text,
            "hello there"
        );
        assert_eq!(searchable_text(Some("  "), Some("")).text, "");
        assert_eq!(searchable_text(None, None), SearchableText::default());
    }

    #[test]
    fn a_long_html_paragraph_collapses_its_wrap_into_one_phrase() {
        // The reader breaks this paragraph at 80 columns; the phrase must still match.
        let html = format!("<p>{} payment overdue</p>", "a".repeat(72));
        let parts = body_match_parts(None, Some(html.as_str())).unwrap_or_default();
        assert_eq!(parts.len(), 1);
        assert!(parts[0].contains("payment overdue"), "got {:?}", parts[0]);
    }

    #[test]
    fn a_big_cell_table_converts_in_bounded_output() {
        // Rendering at a huge width padded every row to that width: one
        // 524,288-character cell and 1,000 rows made gigabytes.
        let mut html = String::from("<table><tr><td>");
        html.push_str(&"x".repeat(524_288));
        html.push_str("</td></tr>");
        for n in 0..1_000 {
            html.push_str(&format!("<tr><td>row {n}</td></tr>"));
        }
        html.push_str("</table>");
        let text = searchable_text(None, Some(html.as_str())).text;
        assert!(
            text.len() < 4 * html.len(),
            "output {} bytes for {} input bytes",
            text.len(),
            html.len()
        );
    }

    #[test]
    fn a_huge_colspan_still_renders_through_the_clamp() {
        // Unclamped, html2text panics on this span and the part falls back.
        let html = r#"<table><tr><td colspan="18446744073709551615">A</td><td>B</td></tr></table>"#;
        let searchable = searchable_text(None, Some(html));
        assert_eq!(searchable.fallbacks, 0);
        assert!(searchable.text.contains('A') && searchable.text.contains('B'));
    }

    #[test]
    fn rule_plain_part_is_exactly_as_stored() {
        // Pre-HTML rules matched the stored plain text, so nothing may change it.
        assert_eq!(
            body_match_parts(Some("invoice\n"), None),
            Some(vec!["invoice\n".to_string()])
        );
        assert_eq!(body_match_parts(Some(""), None), Some(vec![String::new()]));
    }

    #[test]
    fn rule_html_part_is_converted_and_whitespace_collapsed() {
        let parts =
            body_match_parts(Some("plain"), Some("<p>Hello\n\n  there</p>")).unwrap_or_default();
        assert_eq!(parts, vec!["plain".to_string(), "Hello there".to_string()]);
    }

    #[test]
    fn no_parts_at_all_is_unknown() {
        assert_eq!(body_match_parts(None, None), None);
    }
}
