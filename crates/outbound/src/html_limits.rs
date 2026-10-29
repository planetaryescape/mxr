//! Copy of `mxr_reader::html_limits`. mxr-outbound depends only on mxr-core,
//! so the bounds are kept in step by hand: change both files together.
//!
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::ops::Range;

/// HTML larger than this skips html2text and gets the tag-strip fallback.
/// Ordinary mail is tens of kilobytes; 2 MiB is a generous ceiling above that,
/// and a judgement rather than a measured threshold.
pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

/// A table whose widest row has more cells than this is flattened. No readable
/// email table is this wide, and the cost of a row grows with its width.
pub const MAX_TABLE_COLUMNS: usize = 64;

/// A table whose rows times widest-row cell count exceeds this is flattened.
/// It bounds the cells in a table of ordinary width (a long, narrow table is
/// fine; a 3-column table of 20,000 rows is not).
pub const MAX_TABLE_CELLS: usize = 50_000;

static TABLE_TAG_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)<(/?)(table|tr|td|th)\b[^>]*>").expect("table tag regex should compile")
});

static FLATTEN_TAG_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?i)<(/?)(tr|td|th|table|tbody|thead|tfoot|colgroup|col)\b[^>]*>")
        .expect("table flatten regex should compile")
});

/// The HTML to give html2text, or `None` when the document is over
/// [`MAX_HTML_BYTES`] and the caller should use its tag-strip fallback.
///
/// Tables over the complexity limits are flattened to inline text (a row
/// becomes a line break, a cell becomes text followed by a separator); normal
/// tables, and documents with none, come back borrowed and unchanged.
pub fn bounded_html(html: &str) -> Option<Cow<'_, str>> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    let ranges = oversized_table_ranges(html);
    if ranges.is_empty() {
        return Some(Cow::Borrowed(html));
    }
    Some(Cow::Owned(flatten_ranges(html, &ranges)))
}

#[derive(Default)]
struct OpenTable {
    start: usize,
    rows: usize,
    cells_in_row: usize,
    widest_row: usize,
}

impl OpenTable {
    fn finish_row(&mut self) {
        self.widest_row = self.widest_row.max(self.cells_in_row);
        self.cells_in_row = 0;
    }

    fn is_oversized(&self) -> bool {
        self.widest_row > MAX_TABLE_COLUMNS
            || self.rows.saturating_mul(self.widest_row) > MAX_TABLE_CELLS
    }
}

/// Byte ranges of the outermost tables over the limits. Cells count toward
/// the innermost open table. An unclosed table runs to the end of the input,
/// as html2text reads it.
fn oversized_table_ranges(html: &str) -> Vec<Range<usize>> {
    let mut open: Vec<OpenTable> = Vec::new();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let close = |table: OpenTable, end: usize, ranges: &mut Vec<Range<usize>>| {
        let mut table = table;
        table.finish_row();
        if table.is_oversized() {
            ranges.push(table.start..end);
        }
    };
    for caps in TABLE_TAG_RE.captures_iter(html) {
        let Some(whole) = caps.get(0) else { continue };
        let closing = &caps[1] == "/";
        match (caps[2].to_ascii_lowercase().as_str(), closing) {
            ("table", false) => open.push(OpenTable {
                start: whole.start(),
                ..OpenTable::default()
            }),
            ("table", true) => {
                if let Some(table) = open.pop() {
                    close(table, whole.end(), &mut ranges);
                }
            }
            ("tr", false) => {
                if let Some(table) = open.last_mut() {
                    table.finish_row();
                    table.rows += 1;
                }
            }
            ("td" | "th", false) => {
                if let Some(table) = open.last_mut() {
                    table.cells_in_row += 1;
                }
            }
            _ => {}
        }
    }
    while let Some(table) = open.pop() {
        close(table, html.len(), &mut ranges);
    }
    // Keep only the outermost flagged tables: a flattened outer table already
    // covers the inner ones.
    ranges.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
    let mut outermost: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match outermost.last() {
            Some(last) if range.start < last.end => {}
            _ => outermost.push(range),
        }
    }
    outermost
}

/// Replaces each range (sorted, non-overlapping) with its flattened text.
fn flatten_ranges(html: &str, ranges: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(html.len());
    let mut pos = 0;
    for range in ranges {
        out.push_str(&html[pos..range.start]);
        out.push_str(&flatten_table(&html[range.clone()]));
        pos = range.end;
    }
    out.push_str(&html[pos..]);
    out
}

/// A table's markup as inline text: each row becomes a line break, each cell
/// its text followed by a separator, and the table's own wrapper tags go.
fn flatten_table(segment: &str) -> String {
    FLATTEN_TAG_RE
        .replace_all(segment, |caps: &Captures<'_>| {
            let closing = &caps[1] == "/";
            match caps[2].to_ascii_lowercase().as_str() {
                "tr" => "<br>".to_string(),
                "td" | "th" if closing => " | ".to_string(),
                "td" | "th" => " ".to_string(),
                _ => String::new(),
            }
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adversarial table from the wide-table report: one 20,000-cell row
    /// then 20,000 single-cell rows, about 600 KB of input.
    fn adversarial_table() -> String {
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
    fn the_adversarial_table_is_flattened_to_bounded_text() {
        let html = adversarial_table();
        let started = std::time::Instant::now();
        let bounded = bounded_html(&html).expect("under the size cap");
        assert!(!bounded.contains("<table"), "table wrapper survived");
        assert!(bounded.contains("c0") && bounded.contains("c19999"));
        assert!(bounded.contains("r0") && bounded.contains("r19999"));
        assert!(
            bounded.len() < 4 * html.len(),
            "{} bytes out for {} in",
            bounded.len(),
            html.len()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn a_table_over_the_row_cell_limit_is_flattened_even_if_narrow() {
        let mut html = String::from("<table>");
        for n in 0..(MAX_TABLE_CELLS / 3 + 1) {
            html.push_str(&format!("<tr><td>a{n}</td><td>b</td><td>c</td></tr>"));
        }
        html.push_str("</table>");
        let bounded = bounded_html(&html).expect("under the size cap");
        assert!(!bounded.contains("<table"));
    }

    #[test]
    fn an_oversized_document_gets_no_render() {
        let html = format!("<p>{}</p>", "word ".repeat(MAX_HTML_BYTES / 5 + 1));
        assert!(html.len() > MAX_HTML_BYTES);
        assert!(bounded_html(&html).is_none());
    }

    #[test]
    fn a_flattened_table_keeps_its_cell_text_in_order() {
        let html = "<table><tr><td>one</td><td>two</td></tr><tr><td>three</td></tr></table>";
        let flat = flatten_table(html);
        let one = flat.find("one").unwrap_or(usize::MAX);
        let two = flat.find("two").unwrap_or(usize::MAX);
        let three = flat.find("three").unwrap_or(usize::MAX);
        assert!(one < two && two < three, "got {flat:?}");
        assert!(flat.contains("<br>"));
    }
}
