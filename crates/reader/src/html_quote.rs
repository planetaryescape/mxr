//! The quoted conversation and signature inside an HTML body, found by the
//! markers the big clients leave: Gmail's `gmail_quote` and
//! `gmail_signature`, Apple Mail's `blockquote type=cite`, Outlook's
//! `divRplyFwdMsg` and `appendonsend`, Yahoo's `yahoo_quoted` and
//! Thunderbird's `moz-cite-prefix`. These markers vanish once HTML becomes
//! text, so they are removed here, before conversion, and every client
//! (CLI, TUI, web, MCP) sees the same text.

use once_cell::sync::Lazy;
use scraper::{ElementRef, Html, Selector};

/// Markers that wrap the quote itself: removing the element removes the
/// quote, and anything written after it (an inline reply) stays.
const QUOTE_SELECTORS: &[&str] = &[
    ".gmail_quote",
    "blockquote[type='cite']",
    ".yahoo_quoted",
    ".moz-cite-prefix",
];

/// Markers that only head the quote: Outlook-style clients put an `<hr>`
/// or a header block here and the whole prior conversation after it,
/// unwrapped.
const QUOTE_HEADER_SELECTORS: &[&str] = &[
    "#appendonsend",
    "div[id^='divRplyFwdMsg']",
    "#divRplyFwdMsg",
    "#mail-editor-reference-message-container",
    "#reply-intro",
];

const SIGNATURE_SELECTORS: &[&str] = &[
    ".gmail_signature",
    "[data-smartmail='gmail_signature']",
    "#Signature",
    ".moz-signature",
];

fn compile(selectors: &[&str]) -> Vec<Selector> {
    selectors
        .iter()
        .map(|selector| Selector::parse(selector).expect("quote marker selector should parse"))
        .collect()
}

static QUOTES: Lazy<Vec<Selector>> = Lazy::new(|| compile(QUOTE_SELECTORS));
static QUOTE_HEADERS: Lazy<Vec<Selector>> = Lazy::new(|| compile(QUOTE_HEADER_SELECTORS));
static SIGNATURES: Lazy<Vec<Selector>> = Lazy::new(|| compile(SIGNATURE_SELECTORS));
static IMG: Lazy<Selector> = Lazy::new(|| Selector::parse("img").expect("img selector"));

/// An HTML body split at its client markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlParts {
    /// The body with the marked quote and signature removed.
    pub main: String,
    pub has_quote: bool,
    pub has_signature: bool,
}

/// Remove the marked quote and signature from `html`. A body that is
/// nothing but quote (a bare forward) keeps its quote.
pub fn split_html_quote(html: &str) -> HtmlParts {
    let unchanged = || HtmlParts {
        main: html.to_string(),
        has_quote: false,
        has_signature: false,
    };
    // Cheap check first: most mail carries none of the markers.
    let lower = html.to_ascii_lowercase();
    let maybe = [
        "gmail_quote",
        "type=\"cite\"",
        "type='cite'",
        "type=cite",
        "yahoo_quoted",
        "moz-cite-prefix",
        "appendonsend",
        "divrplyfwdmsg",
        "mail-editor-reference-message-container",
        "reply-intro",
        "gmail_signature",
        "id=\"signature\"",
        "moz-signature",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if !maybe {
        return unchanged();
    }

    let mut doc = Html::parse_document(html);
    let mut has_signature = false;
    let mut has_quote = false;
    let mut doomed = Vec::new();

    for selector in SIGNATURES.iter() {
        for element in doc.select(selector) {
            has_signature = true;
            doomed.push(element.id());
        }
    }
    if let Some(header) = first_in_document_order(&doc, &QUOTE_HEADERS) {
        has_quote = true;
        doomed.extend(from_here_on(header));
    }
    for selector in QUOTES.iter() {
        for element in doc.select(selector) {
            has_quote = true;
            doomed.push(element.id());
        }
    }
    for id in doomed {
        if let Some(mut node) = doc.tree.get_mut(id) {
            node.detach();
        }
    }

    let body_text: String = doc.root_element().text().collect();
    if has_quote && body_text.trim().is_empty() && doc.select(&IMG).next().is_none() {
        // A bare forward: the quote is the message.
        return HtmlParts {
            has_signature,
            ..unchanged()
        };
    }
    HtmlParts {
        main: doc.root_element().html(),
        has_quote,
        has_signature,
    }
}

fn first_in_document_order<'a>(doc: &'a Html, selectors: &[Selector]) -> Option<ElementRef<'a>> {
    // `select` walks in document order, so the first match across all
    // selectors is the one whose position in a full walk comes first.
    let order: Vec<_> = doc.root_element().descendants().map(|n| n.id()).collect();
    selectors
        .iter()
        .filter_map(|selector| doc.select(selector).next())
        .min_by_key(|element| {
            order
                .iter()
                .position(|id| *id == element.id())
                .unwrap_or(usize::MAX)
        })
}

/// `node`, then every later sibling of it and of each ancestor up to the
/// body: the Outlook header and the unwrapped conversation after it.
fn from_here_on(node: ElementRef<'_>) -> Vec<ego_tree::NodeId> {
    let mut ids = vec![node.id()];
    let mut current = Some(*node);
    while let Some(here) = current {
        let is_body = here
            .value()
            .as_element()
            .is_some_and(|element| element.name() == "body");
        if is_body {
            break;
        }
        let mut sibling = here.next_sibling();
        while let Some(next) = sibling {
            ids.push(next.id());
            sibling = next.next_sibling();
        }
        current = here.parent();
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_quote_and_signature_are_removed() {
        let html = r#"<div dir="ltr">Sounds good, Thursday works.<div><br></div><div class="gmail_signature">Samir Patel<br>Launchpad</div></div><br><div class="gmail_quote"><div class="gmail_attr">On Mon, Alex wrote:</div><blockquote class="gmail_quote">Are you around Thursday?</blockquote></div>"#;
        let parts = split_html_quote(html);
        assert!(parts.has_quote);
        assert!(parts.has_signature);
        assert!(parts.main.contains("Thursday works"));
        assert!(!parts.main.contains("Are you around"));
        assert!(!parts.main.contains("Launchpad"));
    }

    #[test]
    fn apple_cite_blockquote_is_removed_and_inline_reply_kept() {
        let html = r#"<html><body><div>Yes, send it.</div><blockquote type="cite"><div>Shall I send the deck?</div></blockquote><div>One more thing: copy Ruth.</div></body></html>"#;
        let parts = split_html_quote(html);
        assert!(parts.has_quote);
        assert!(parts.main.contains("Yes, send it."));
        assert!(parts.main.contains("copy Ruth"));
        assert!(!parts.main.contains("Shall I send"));
    }

    #[test]
    fn outlook_header_removes_everything_after_it() {
        let html = r#"<html><body><div>Approved.</div><div><div id="appendonsend"></div><hr><div id="divRplyFwdMsg"><b>From:</b> Alex<br><b>Sent:</b> Monday</div><div>Can you approve the budget?</div></div><p>trailing old text</p></body></html>"#;
        let parts = split_html_quote(html);
        assert!(parts.has_quote);
        assert!(parts.main.contains("Approved."));
        assert!(!parts.main.contains("approve the budget"));
        assert!(!parts.main.contains("trailing old text"));
    }

    #[test]
    fn a_bare_forward_keeps_its_quote() {
        let html =
            r#"<div class="gmail_quote">---------- Forwarded message ---------<br>The deck</div>"#;
        let parts = split_html_quote(html);
        assert!(!parts.has_quote);
        assert!(parts.main.contains("The deck"));
    }

    #[test]
    fn plain_html_is_untouched() {
        let html = "<p>Just a note.</p>";
        let parts = split_html_quote(html);
        assert_eq!(parts.main, html);
        assert!(!parts.has_quote && !parts.has_signature);
    }
}
