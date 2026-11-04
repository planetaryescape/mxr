//! The quoted conversation and signature inside an HTML body, found by the
//! markers the big clients leave: Gmail's `gmail_quote` and
//! `gmail_signature`, Apple Mail's `blockquote type=cite`, Outlook's
//! `divRplyFwdMsg` and `appendonsend`, Yahoo's `yahoo_quoted` and
//! Thunderbird's `moz-cite-prefix`. These markers vanish once HTML becomes
//! text, so they are removed here, before conversion, and every client
//! (CLI, TUI, web, MCP) sees the same text.

use mxr_html_worker::Job;
use once_cell::sync::Lazy;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HtmlParts {
    /// The body with the marked quote and signature removed.
    pub main: String,
    pub has_quote: bool,
    pub has_signature: bool,
    /// A marked quote was kept because authored text follows it.
    pub has_preserved_quote: bool,
}

/// Remove the marked quote and signature from `html`. A body that is
/// nothing but quote (a bare forward) keeps its quote.
pub fn split_html_quote(html: &str) -> HtmlParts {
    split(html, false)
}

/// [`split_html_quote`] for a view that shows the signature: only the
/// quote goes, and `has_signature` still says whether there is one.
pub fn split_html_quote_keeping_signature(html: &str) -> HtmlParts {
    split(html, true)
}

fn split(html: &str, keep_signature: bool) -> HtmlParts {
    // Cheap check first: most mail carries none of the markers, and those
    // documents never need the parser or the helper process.
    if !may_hold_markers(html) {
        return unchanged(html);
    }
    // A lost helper (timeout, crash) leaves the body whole, as an over-budget
    // one does: no quote or signature is recognised.
    SPLIT
        .call(html, &keep_signature)
        .unwrap_or_else(|| unchanged(html))
}

fn unchanged(html: &str) -> HtmlParts {
    HtmlParts {
        main: html.to_string(),
        has_quote: false,
        has_signature: false,
        has_preserved_quote: false,
    }
}

fn may_hold_markers(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    [
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
    .any(|marker| lower.contains(marker))
}

/// The parse, which is what runs in the HTML helper process.
pub(crate) static SPLIT: Job<bool, HtmlParts> = Job::new("reader.split_quote", split_in_process);

fn split_in_process(html: &str, keep_signature: &bool) -> HtmlParts {
    let keep_signature = *keep_signature;
    let untouched = || unchanged(html);

    // Over budget, no parse: the message keeps its markup-free fallback text and
    // no quote or signature is recognised.
    if !crate::html_limits::within_budget(html) {
        return untouched();
    }
    let mut doc = Html::parse_document(html);
    let mut has_signature = false;
    let mut has_quote = false;
    let mut has_preserved_quote = false;
    let mut doomed = Vec::new();

    for selector in SIGNATURES.iter() {
        for element in doc.select(selector) {
            has_signature = true;
            if !keep_signature {
                doomed.push(element.id());
            }
        }
    }
    if let Some(header) = first_in_document_order(&doc, &QUOTE_HEADERS) {
        has_quote = true;
        doomed.extend(from_here_on(header));
    }
    for selector in QUOTES.iter() {
        for element in doc.select(selector) {
            if has_text_after(element) || contains_forwarded_headers(element) {
                // A reply after the citation may answer it inline. Keep the
                // marked block so HTML-to-text conversion preserves its
                // position beside the answer.
                has_preserved_quote = true;
                continue;
            }
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
            has_preserved_quote,
            ..untouched()
        };
    }
    HtmlParts {
        main: doc.root_element().html(),
        has_quote,
        has_signature,
        has_preserved_quote,
    }
}

fn contains_forwarded_headers(element: ElementRef<'_>) -> bool {
    let text = element.text().collect::<String>().to_ascii_lowercase();
    text.contains("forwarded message")
        && text.contains("from:")
        && (text.contains("date:") || text.contains("subject:"))
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

fn has_text_after(element: ElementRef<'_>) -> bool {
    let mut current = Some(*element);
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
            if has_authored_text(next) {
                return true;
            }
            sibling = next.next_sibling();
        }
        current = here.parent();
    }
    false
}

fn has_authored_text(node: ego_tree::NodeRef<'_, scraper::node::Node>) -> bool {
    node.descendants()
        .filter_map(|descendant| descendant.value().as_text().map(|text| (descendant, text)))
        .any(|(descendant, text)| {
            if text.trim().is_empty() {
                return false;
            }
            !descendant
                .ancestors()
                .filter_map(ElementRef::wrap)
                .any(|ancestor| {
                    let element = ancestor.value();
                    let class = element.attr("class").unwrap_or("");
                    let id = element.attr("id").unwrap_or("");
                    (element.name() == "blockquote" && element.attr("type") == Some("cite"))
                        || [
                            "gmail_quote",
                            "yahoo_quoted",
                            "moz-cite-prefix",
                            "gmail_signature",
                            "moz-signature",
                        ]
                        .iter()
                        .any(|marker| class.split_whitespace().any(|class| class == *marker))
                        || ["Signature", "divRplyFwdMsg", "appendonsend"]
                            .iter()
                            .any(|marker| id.eq_ignore_ascii_case(marker))
                })
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
    fn keeping_the_signature_removes_only_the_quote() {
        let html = r#"<div dir="ltr">Sounds good, Thursday works.<div><br></div><div class="gmail_signature">Samir Patel<br>Launchpad</div></div><br><div class="gmail_quote"><div class="gmail_attr">On Mon, Alex wrote:</div><blockquote class="gmail_quote">Are you around Thursday?</blockquote></div>"#;
        let parts = split_html_quote_keeping_signature(html);
        assert!(parts.has_quote);
        assert!(parts.has_signature);
        assert!(parts.main.contains("Thursday works"));
        assert!(!parts.main.contains("Are you around"));
        assert!(parts.main.contains("Launchpad"));
    }

    #[test]
    fn apple_cite_blockquote_stays_when_authored_text_follows() {
        let html = r#"<html><body><div>Yes, send it.</div><blockquote type="cite"><div>Shall I send the deck?</div></blockquote><div>One more thing: copy Ruth.</div></body></html>"#;
        let parts = split_html_quote(html);
        assert!(!parts.has_quote);
        assert!(parts.main.contains("Yes, send it."));
        assert!(parts.main.contains("copy Ruth"));
        assert!(parts.main.contains("Shall I send"));
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

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn an_over_budget_document_is_not_parsed_for_quotes_or_signatures() {
        let html = format!(
            "<div class=\"gmail_quote\">quoted</div>{}",
            "<br>".repeat(200_000)
        );
        let parts = split_html_quote(&html);
        assert!(!parts.has_quote);
        assert_eq!(parts.main, html);
    }
}

#[cfg(test)]
mod lost_helper_tests {
    use super::*;
    use mxr_html_worker::{with_supervisor, Config, Supervisor};
    use std::sync::Arc;

    #[test]
    fn a_lost_helper_leaves_the_body_whole_and_recognises_no_quote() {
        let html = "<div>reply</div><div class=\"gmail_quote\">older words</div>";
        let lost = Arc::new(Supervisor::new(Config::new(
            "/nonexistent/mxr-helper",
            Vec::new(),
        )));
        let parts = with_supervisor(lost, || split_html_quote(html));
        assert_eq!(parts.main, html);
        assert!(!parts.has_quote && !parts.has_signature);
    }

    #[test]
    fn a_body_with_no_markers_never_reaches_the_helper() {
        // The marker check runs first, so a plain message costs no round trip
        // and survives a helper that cannot start.
        let lost = Arc::new(Supervisor::new(Config::new(
            "/nonexistent/mxr-helper",
            Vec::new(),
        )));
        let html = "<p>no markers here</p>";
        let parts = with_supervisor(Arc::clone(&lost), || split_html_quote(html));
        assert_eq!(parts.main, html);
        assert_eq!(lost.stats().requests, 0);
    }
}
