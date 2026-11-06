//! Copy of `mxr_reader::html_limits`. mxr-outbound depends only on mxr-core,
//! so the budget is kept in step by hand: change both files together.
//!
//! Budget for untrusted mail HTML, checked before any parser sees it.
//!
//! html5ever and html2text are super-linear on some inputs that pass a
//! depth or structure check: a tag with 100,000 attributes costs the tokenizer
//! quadratic time, a table full of `<br>` makes foster-parenting quadratic, and
//! a table of `<td rowspan=…>` makes html2text allocate gigabytes. Measuring
//! with a parser cannot bound a parser, so this module measures with a
//! hand-written byte scan that is linear and does not build a tree.
//!
//! The scan counts a tag at every `<` followed by a letter, `/` or `!`. It
//! over-counts markup inside comments, scripts and text, which can only cause a
//! fallback. Attributes are the tokens of a tag split on whitespace and `/`
//! (HTML also treats `/` between attributes as a separator), so quoted values
//! with spaces over-count too. Every over-count errs toward the fallback.
//!
//! The budget bounds every super-linear path we know of. Each parser's
//! quadratic work is at most about the square of the tag count, so with at most
//! 20,000 tags it is about 4 × 10^8 cheap steps at worst. Depth is not capped
//! separately: nesting needs tags, so the tag cap covers it.

use std::borrow::Cow;

/// Documents larger than this are never parsed. Ordinary mail is tens of
/// kilobytes; 2 MiB is a generous ceiling above that, and a judgement rather
/// than a measured threshold.
pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

/// Tags in the document, counted as the scan defines them.
pub const MAX_TAGS: usize = 20_000;

/// Attributes (tokens after the tag name) in any one tag.
pub const MAX_ATTRS_PER_TAG: usize = 64;

/// `td` and `th` cells in the document.
pub const MAX_CELLS: usize = 20_000;

/// Cells in the widest row, rows being delimited by `<tr` tags.
pub const MAX_ROW_CELLS: usize = 64;

/// Grid slots: the sum over cells of colspan times rowspan, after clamping.
/// html2text makes placeholder cells for spans, so this bounds the work that
/// spans cost even when the cell count is small. It is a judgement: an ordinary
/// table has about one slot per cell.
pub const MAX_GRID_SLOTS: u64 = 200_000;

/// Largest `colspan` or `rowspan` html2text is given. Spans above it are
/// rewritten to this value, inside tags only.
pub const MAX_SPAN: u64 = 100;

/// The HTML to hand to a parser, or `None` when the document is over budget and
/// must not be parsed at all (callers use their fallback).
///
/// Spans inside tags are clamped to [`MAX_SPAN`]; text and attribute values
/// other than colspan and rowspan are never changed. A document within budget
/// with no spans to clamp comes back borrowed and unchanged.
pub fn bounded_html(html: &str) -> Option<Cow<'_, str>> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    let found = scan(html, true)?;
    Some(apply_edits(html, &found.edits))
}

/// True when the document is within budget and may be parsed. Callers that
/// parse for a reason other than rendering (links, images, quotes, validation)
/// check this before parsing.
pub fn within_budget(html: &str) -> bool {
    html.len() <= MAX_HTML_BYTES && scan(html, true).is_some()
}

/// `html` with every colspan and rowspan inside a tag clamped to [`MAX_SPAN`],
/// whatever the document's size. Outbound's own tests use it; the send path only
/// needs `bounded_html`.
#[cfg(test)]
pub fn clamp_table_spans(html: &str) -> Cow<'_, str> {
    match scan(html, false) {
        Some(found) => apply_edits(html, &found.edits),
        None => Cow::Borrowed(html),
    }
}

/// A run of ASCII digits, inside a tag, to be replaced by [`MAX_SPAN`].
struct Edit {
    start: usize,
    end: usize,
}

#[derive(Default)]
struct Scan {
    edits: Vec<Edit>,
}

/// Scans the whole document in one pass. With `enforce`, the first budget
/// breach returns `None`; without it the scan only gathers the span edits.
fn scan(html: &str, enforce: bool) -> Option<Scan> {
    let bytes = html.as_bytes();
    let mut found = Scan::default();
    let mut counts = Counts::default();
    let mut pos = 0;
    while let Some(offset) = bytes[pos..].iter().position(|&byte| byte == b'<') {
        let open = pos + offset;
        pos = open + 1;
        let opens_tag = matches!(
            bytes.get(open + 1),
            Some(byte) if byte.is_ascii_alphabetic() || *byte == b'/' || *byte == b'!'
        );
        if !opens_tag {
            continue;
        }
        let close = bytes[open + 1..]
            .iter()
            .position(|&byte| byte == b'>')
            .map_or(bytes.len(), |offset| open + 1 + offset);
        counts.tags += 1;
        if enforce && counts.tags > MAX_TAGS {
            return None;
        }
        inspect_tag(
            html,
            open + 1,
            close,
            enforce,
            &mut counts,
            &mut found.edits,
        )?;
        pos = close;
    }
    Some(found)
}

#[derive(Default)]
struct Counts {
    tags: usize,
    cells: usize,
    row_cells: usize,
    grid_slots: u64,
}

/// Inspects the tag whose contents are `html[start..end]`.
fn inspect_tag(
    html: &str,
    start: usize,
    end: usize,
    enforce: bool,
    counts: &mut Counts,
    edits: &mut Vec<Edit>,
) -> Option<()> {
    let bytes = html.as_bytes();
    let closing = bytes.get(start) == Some(&b'/');
    let name_start = skip_separators(bytes, start, end);
    let name_end = bytes[name_start..end]
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'-')
        .map_or(end, |offset| name_start + offset);
    let name = html[name_start..name_end].to_ascii_lowercase();

    let mut colspan = 1u64;
    let mut rowspan = 1u64;
    let mut attributes = 0usize;
    // Quote parity over the bytes walked so far. A span-looking token is only
    // trusted outside quotes, so the scan never rewrites another attribute's
    // value.
    let mut quoted = Quotes::default();
    let mut walked = start;
    let mut index = name_end;
    loop {
        index = skip_separators(bytes, index, end);
        if index >= end {
            break;
        }
        quoted.advance(&bytes[walked..index]);
        walked = index;
        let token_end = bytes[index..end]
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'/')
            .map_or(end, |offset| index + offset);
        attributes += 1;
        if enforce && attributes > MAX_ATTRS_PER_TAG {
            return None;
        }
        match span_attribute(html, index, &quoted, enforce, edits)? {
            Some(SpanKind::Col(value)) => colspan = value.min(MAX_SPAN),
            Some(SpanKind::Row(value)) => rowspan = value.min(MAX_SPAN),
            None => {}
        }
        index = token_end;
    }

    if !closing && name == "tr" {
        counts.row_cells = 0;
    }
    if !closing && (name == "td" || name == "th") {
        counts.cells += 1;
        counts.row_cells += 1;
        counts.grid_slots = counts
            .grid_slots
            .saturating_add(colspan.saturating_mul(rowspan));
        if enforce
            && (counts.cells > MAX_CELLS
                || counts.row_cells > MAX_ROW_CELLS
                || counts.grid_slots > MAX_GRID_SLOTS)
        {
            return None;
        }
    }
    Some(())
}

/// Parity of double and single quotes walked so far. Odd means inside a
/// quoted value, as far as a byte scan can tell.
#[derive(Default)]
struct Quotes {
    double: bool,
    single: bool,
}

impl Quotes {
    fn advance(&mut self, bytes: &[u8]) {
        for byte in bytes {
            match byte {
                b'"' => self.double = !self.double,
                b'\'' => self.single = !self.single,
                _ => {}
            }
        }
    }

    fn inside(&self) -> bool {
        self.double || self.single
    }
}

enum SpanKind {
    Col(u64),
    Row(u64),
}

/// If the token at `start` is a `colspan` or `rowspan` attribute outside quotes,
/// its value, and the digit run to rewrite when it is over [`MAX_SPAN`].
///
/// A span whose value has an entity and no digits is over budget: the entity
/// can decode to a large number this scan cannot see. A span-looking token
/// inside quotes is over budget too, since the scan cannot tell whether it is
/// an attribute. Without `enforce` both are simply ignored.
fn span_attribute(
    html: &str,
    start: usize,
    quoted: &Quotes,
    enforce: bool,
    edits: &mut Vec<Edit>,
) -> Option<Option<SpanKind>> {
    let bytes = html.as_bytes();
    let kind_of = |name: &[u8]| {
        if name.eq_ignore_ascii_case(b"colspan") {
            Some(true)
        } else if name.eq_ignore_ascii_case(b"rowspan") {
            Some(false)
        } else {
            None
        }
    };
    let Some(is_col) = bytes.get(start..start + 7).and_then(kind_of) else {
        return Some(None);
    };
    if quoted.inside() {
        return if enforce { None } else { Some(None) };
    }
    // The name is followed by optional whitespace, `=`, optional whitespace and
    // an optional quote; the value is the digits after that.
    let mut value_start = start + 7;
    while value_start < bytes.len() && bytes[value_start].is_ascii_whitespace() {
        value_start += 1;
    }
    if bytes.get(value_start) != Some(&b'=') {
        return Some(None);
    }
    value_start += 1;
    while value_start < bytes.len() && bytes[value_start].is_ascii_whitespace() {
        value_start += 1;
    }
    if matches!(bytes.get(value_start), Some(b'"' | b'\'')) {
        value_start += 1;
    }
    let digits_end = bytes[value_start..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map_or(bytes.len(), |offset| value_start + offset);
    let value = if digits_end == value_start {
        if enforce && bytes.get(value_start) == Some(&b'&') {
            return None;
        }
        1
    } else {
        let value = html[value_start..digits_end]
            .parse::<u64>()
            .unwrap_or(u64::MAX);
        if value > MAX_SPAN {
            edits.push(Edit {
                start: value_start,
                end: digits_end,
            });
        }
        value
    };
    Some(Some(if is_col {
        SpanKind::Col(value)
    } else {
        SpanKind::Row(value)
    }))
}

fn skip_separators(bytes: &[u8], mut index: usize, end: usize) -> usize {
    while index < end && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
        index += 1;
    }
    index
}

fn apply_edits<'a>(html: &'a str, edits: &[Edit]) -> Cow<'a, str> {
    if edits.is_empty() {
        return Cow::Borrowed(html);
    }
    let replacement = MAX_SPAN.to_string();
    let mut out = String::with_capacity(html.len());
    let mut position = 0;
    for edit in edits {
        out.push_str(&html[position..edit.start]);
        out.push_str(&replacement);
        position = edit.end;
    }
    out.push_str(&html[position..]);
    Cow::Owned(out)
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
    fn a_wide_table_is_over_budget() {
        assert!(!within_budget(&wide_table()));
    }

    #[test]
    fn a_comment_cannot_hide_a_wide_table() {
        let html = wide_table().replacen("<tr>", "<!-- </table> --><tr>", 1);
        assert!(!within_budget(&html));
    }

    #[test]
    fn a_tag_with_100000_attributes_is_over_budget_without_parsing_it() {
        let mut html = String::from("<td");
        for n in 0..100_000 {
            html.push_str(&format!(" a{n}"));
        }
        html.push_str(">x</td>");
        assert!(!within_budget(&html));
    }

    #[test]
    fn foster_parenting_table_of_brs_is_over_budget() {
        let html = format!("<table>{}</table>", "<br>".repeat(200_000));
        assert!(!within_budget(&html));
    }

    #[test]
    fn a_100000_deep_div_nest_is_over_budget() {
        let html = format!("{}x{}", "<div>".repeat(100_000), "</div>".repeat(100_000));
        assert!(!within_budget(&html));
    }

    #[test]
    fn a_list_with_optional_end_tags_is_within_budget() {
        // Depth is not a budget: this list has 300 items and no unclosed depth
        // cap trips.
        let items = "<li>Item<br>details".repeat(300);
        let html = format!("<ol>{items}</ol>");
        assert!(within_budget(&html));
    }

    #[test]
    fn rowspan_in_text_is_not_rewritten() {
        let html = "use rowspan=365. for the rows";
        assert_eq!(bounded_html(html).as_deref(), Some(html));
    }

    #[test]
    fn rowspan_inside_a_tag_is_clamped() {
        let html = "<td rowspan=365>x</td>";
        assert_eq!(
            bounded_html(html).as_deref(),
            Some("<td rowspan=100>x</td>")
        );
    }

    #[test]
    fn a_span_with_an_entity_and_no_digits_is_over_budget() {
        // `&#53;` decodes to `5`, so the value is 50000 to the parser.
        let html = r#"<table><tr><td colspan="&#53;0000">x</td></tr></table>"#;
        assert!(!within_budget(html));
    }

    #[test]
    fn grid_slots_over_the_cap_are_over_budget() {
        // 2,000 cells of rowspan 100 is 200,000 slots: exactly at the cap.
        let at_cap: String = (0..2_000)
            .map(|_| r#"<tr><td rowspan="100">x</td></tr>"#)
            .collect();
        assert!(within_budget(&format!("<table>{at_cap}</table>")));
        let over: String = (0..2_001)
            .map(|_| r#"<tr><td rowspan="100">x</td></tr>"#)
            .collect();
        assert!(!within_budget(&format!("<table>{over}</table>")));
    }

    #[test]
    fn an_oversized_document_is_over_budget() {
        let html = format!("<p>{}</p>", "word ".repeat(MAX_HTML_BYTES / 5 + 1));
        assert!(bounded_html(&html).is_none());
        assert!(!within_budget(&html));
    }
}
