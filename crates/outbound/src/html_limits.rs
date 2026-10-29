//! Copy of `mxr_reader::html_limits`. mxr-outbound depends only on mxr-core,
//! so the bounds are kept in step by hand: change both files together.
//!
//! Bounds on what html2text is given. html2text builds a full DOM, and its cost
//! grows with depth (html5ever checks the open-element stack on every element)
//! and with table width and span size (it pads each row to the column widths
//! and creates placeholder cells for spans). Some inputs cost gigabytes, and an
//! allocation failure aborts the process, which `catch_unwind` cannot catch. So
//! the input is measured before the render, and documents over the caps get the
//! tag-strip fallback instead.
//!
//! The measure is a single pass of html5ever's tokenizer, with no tree builder:
//! the tokenizer knows about comments, CDATA, quoted attributes that contain
//! `>`, and raw-text elements, so it agrees with the parser on what is markup.
//! A regex over the bytes does not (a `<!-- </table> -->` comment fooled one).

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::LocalName;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::cell::RefCell;

/// HTML larger than this skips html2text and gets the tag-strip fallback.
/// Ordinary mail is tens of kilobytes; 2 MiB is a generous ceiling above that,
/// and a judgement rather than a measured threshold.
pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

/// Open elements at once. Real mail nests a few dozen levels at most; 256 is
/// far beyond that and keeps html5ever's per-element ancestor checks short.
pub const MAX_DEPTH: usize = 256;

/// `td` and `th` cells in the whole document. A 20,000-cell table is already
/// past anything a reader can use; this bounds the total work.
pub const MAX_CELLS: usize = 20_000;

/// Cells in the widest row. html2text clones the column widths into every row,
/// so a row's width multiplies the cost of every row after it.
pub const MAX_ROW_CELLS: usize = 64;

/// Largest `rowspan` or `colspan` after clamping. html2text makes placeholder
/// cells for spans, so a span's size costs memory in every row it crosses.
pub const MAX_SPAN: u64 = 100;

/// Grid slots in the document: the sum of colspan times rowspan over all cells.
/// This is the cost html2text pays for spans, and it can exceed the cell count
/// by the span factor. The limit is a judgement: an ordinary table has about one
/// slot per cell.
pub const MAX_GRID_SLOTS: u64 = 200_000;

/// Clamps `colspan` and `rowspan` attribute values to [`MAX_SPAN`]. Copy of
/// `mxr_reader::clamp_table_spans`, which outbound cannot depend on.
pub fn clamp_table_spans(html: &str) -> Cow<'_, str> {
    static TABLE_SPAN_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r#"(?i)\b(colspan|rowspan)(\s*=\s*["']?\s*)(\d+)"#)
            .expect("table span regex should compile")
    });
    TABLE_SPAN_RE.replace_all(html, |caps: &Captures<'_>| {
        let value = caps[3].parse::<u64>().map_or(MAX_SPAN, |n| n.min(MAX_SPAN));
        format!("{}{}{value}", &caps[1], &caps[2])
    })
}

/// The HTML to give html2text, or `None` when the document must not be rendered:
/// over [`MAX_HTML_BYTES`], or over one of the structure caps. Spans are clamped
/// to [`MAX_SPAN`] first, so a clamped table still renders.
pub fn bounded_html(html: &str) -> Option<Cow<'_, str>> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    let clamped = clamp_table_spans(html);
    if exceeds_structure_caps(&clamped) {
        return None;
    }
    Some(clamped)
}

/// Elements that never hold children, so the tree builder never pushes them.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

fn is_void(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

/// The raw-text state html5ever's tree builder would put the tokenizer in for
/// this start tag, so markup inside it is not read as tags. html2text parses
/// with scripting disabled, so `noscript` is ordinary markup and not listed.
fn raw_text_state(name: &str) -> Option<TokenSinkResult<()>> {
    match name {
        "script" => Some(TokenSinkResult::RawData(RawKind::ScriptData)),
        "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
            Some(TokenSinkResult::RawData(RawKind::Rawtext))
        }
        "textarea" | "title" => Some(TokenSinkResult::RawData(RawKind::Rcdata)),
        "plaintext" => Some(TokenSinkResult::Plaintext),
        _ => None,
    }
}

#[derive(Default)]
struct Scan {
    /// Open non-void elements, innermost last. An end tag closes back to its
    /// nearest match, as the tree builder does for ordinary elements. Implicit
    /// closes the tree builder performs are not modelled, so this can count
    /// more open elements than html5ever holds: the caps are then a little
    /// stricter than needed, never looser.
    open: Vec<LocalName>,
    cells: usize,
    row_cells: usize,
    widest_row: usize,
    grid_slots: u64,
    /// Set at the first cap that is exceeded. Later tokens are then ignored.
    over: bool,
}

impl Scan {
    fn start(&mut self, tag: &Tag) {
        let name: &str = &tag.name;
        match name {
            "tr" => self.row_cells = 0,
            "td" | "th" => {
                self.cells += 1;
                self.row_cells += 1;
                self.widest_row = self.widest_row.max(self.row_cells);
                let colspan = span_attr(tag, "colspan");
                let rowspan = span_attr(tag, "rowspan");
                if colspan > MAX_SPAN || rowspan > MAX_SPAN {
                    self.over = true;
                }
                self.grid_slots = self
                    .grid_slots
                    .saturating_add(colspan.saturating_mul(rowspan));
            }
            _ => {}
        }
        if !is_void(name) {
            self.open.push(tag.name.clone());
            if self.open.len() > MAX_DEPTH {
                self.over = true;
            }
        }
        if self.cells > MAX_CELLS
            || self.widest_row > MAX_ROW_CELLS
            || self.grid_slots > MAX_GRID_SLOTS
        {
            self.over = true;
        }
    }

    fn end(&mut self, name: &LocalName) {
        if is_void(name) {
            return;
        }
        if let Some(position) = self.open.iter().rposition(|open| open == name) {
            self.open.truncate(position);
        }
    }
}

/// A span attribute's value; 1 when absent or not a number, as the tree
/// builder's consumers read it. A value too big for u64 saturates.
fn span_attr(tag: &Tag, name: &str) -> u64 {
    tag.attrs
        .iter()
        .find(|attr| &*attr.name.local == name)
        .and_then(|attr| attr.value.trim().parse::<u64>().ok())
        .unwrap_or(1)
}

struct ScanSink(RefCell<Scan>);

impl TokenSink for ScanSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        let Token::TagToken(tag) = token else {
            return TokenSinkResult::Continue;
        };
        let mut scan = self.0.borrow_mut();
        if scan.over {
            return TokenSinkResult::Continue;
        }
        match tag.kind {
            TagKind::StartTag => {
                scan.start(&tag);
                raw_text_state(&tag.name).unwrap_or(TokenSinkResult::Continue)
            }
            TagKind::EndTag => {
                scan.end(&tag.name);
                TokenSinkResult::Continue
            }
        }
    }
}

/// True when the document is over a structure cap. One tokenizer pass; linear
/// in the input, and the open-element stack is bounded by [`MAX_DEPTH`] + 1.
fn exceeds_structure_caps(html: &str) -> bool {
    let tokenizer = Tokenizer::new(
        ScanSink(RefCell::new(Scan::default())),
        TokenizerOpts::default(),
    );
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(html));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let over = tokenizer.sink.0.borrow().over;
    over
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide_table() -> String {
        let mut html = String::from("<table><tr>");
        for n in 0..20_000 {
            html.push_str(&format!("<td>c{n}</td>"));
        }
        html.push_str("</tr>");
        for n in 0..20_000 {
            html.push_str(&format!("<tr><td>r{n}</td></tr>"));
        }
        html.push_str("</table>");
        html
    }

    #[test]
    fn a_normal_table_is_left_exactly_as_it_is() {
        let html = "<table><tr><td>a</td><td>b</td><td>c</td></tr>\
                    <tr><td>d</td><td>e</td><td>f</td></tr>\
                    <tr><td>g</td><td>h</td><td>i</td></tr></table>";
        assert!(matches!(bounded_html(html), Some(Cow::Borrowed(_))));
    }

    #[test]
    fn a_document_without_tables_is_left_exactly_as_it_is() {
        let html = "<p>hello <b>world</b></p>";
        assert!(matches!(bounded_html(html), Some(Cow::Borrowed(_))));
    }

    #[test]
    fn a_wide_table_is_over_the_structure_caps() {
        assert!(bounded_html(&wide_table()).is_none());
    }

    #[test]
    fn a_comment_cannot_hide_a_wide_table_from_the_scan() {
        // The tokenizer reads `<!-- ... -->` as a comment, so the `</table>`
        // inside it is not a close and the wide table is still counted.
        let html = wide_table().replacen("<tr>", "<!-- </table> --><tr>", 1);
        assert!(bounded_html(&html).is_none());
    }

    #[test]
    fn markup_inside_a_script_is_not_read_as_a_table() {
        let html = "<script>var s = '<table><tr><td>x</td></tr>';</script><p>ok</p>";
        assert!(bounded_html(html).is_some());
    }

    #[test]
    fn a_quoted_gt_in_an_attribute_does_not_end_the_tag() {
        // The tokenizer reads the attribute value whole, so the span is 1.
        let html = r#"<table><tr><td title="a>b" colspan="1">x</td></tr></table>"#;
        assert!(bounded_html(html).is_some());
    }

    #[test]
    fn a_deep_nest_is_over_the_depth_cap() {
        let html = format!(
            "{}{}",
            "<div>".repeat(MAX_DEPTH + 1),
            "</div>".repeat(MAX_DEPTH + 1)
        );
        assert!(bounded_html(&html).is_none());
    }

    #[test]
    fn a_span_over_the_clamp_is_rejected_when_the_clamp_missed_it() {
        // An entity-encoded value is not digits to the byte clamp, so the
        // tokenizer sees its real value and the document falls back.
        let html = r#"<table><tr><td colspan="&#53;0000">x</td></tr></table>"#;
        assert!(bounded_html(html).is_none());
    }

    #[test]
    fn an_oversized_document_gets_no_render() {
        let html = format!("<p>{}</p>", "word ".repeat(MAX_HTML_BYTES / 5 + 1));
        assert!(html.len() > MAX_HTML_BYTES);
        assert!(bounded_html(&html).is_none());
    }
}
