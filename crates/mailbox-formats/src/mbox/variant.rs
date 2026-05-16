//! Per-variant escape and unescape logic for mbox body lines.
//!
//! These are pure functions operating on `&[u8]` (one line at a time)
//! so they can be unit-tested in isolation.

use super::MboxVariant;

/// Should the writer escape body lines for this variant?
pub(super) fn writes_escape(variant: MboxVariant) -> bool {
    matches!(variant, MboxVariant::Mboxrd)
}

/// Should the reader unescape body lines for this variant?
pub(super) fn reads_unescape(variant: MboxVariant) -> bool {
    matches!(variant, MboxVariant::Mboxrd)
}

/// Does this variant use `Content-Length:` for body framing?
pub(super) fn uses_content_length(variant: MboxVariant) -> bool {
    matches!(variant, MboxVariant::Mboxcl | MboxVariant::Mboxcl2)
}

/// Apply mboxrd-style write escaping to one body line.
///
/// Per the Mboxrd convention: prefix any line of the form `>*From ` with
/// an extra `>`. This makes the body unambiguous when reading: lines
/// starting with `From ` are message separators; lines starting with
/// `>From ` are escaped body content.
///
/// For other variants this is a no-op (the line is written verbatim).
pub(super) fn escape_body_line<'a>(
    line: &'a [u8],
    variant: MboxVariant,
) -> std::borrow::Cow<'a, [u8]> {
    if !writes_escape(variant) {
        return std::borrow::Cow::Borrowed(line);
    }
    if !matches_from_pattern(line) {
        return std::borrow::Cow::Borrowed(line);
    }
    let mut out = Vec::with_capacity(line.len() + 1);
    out.push(b'>');
    out.extend_from_slice(line);
    std::borrow::Cow::Owned(out)
}

/// Reverse of [`escape_body_line`] for the reader.
pub(super) fn unescape_body_line<'a>(
    line: &'a [u8],
    variant: MboxVariant,
) -> std::borrow::Cow<'a, [u8]> {
    if !reads_unescape(variant) {
        return std::borrow::Cow::Borrowed(line);
    }
    if !matches_escaped_from_pattern(line) {
        return std::borrow::Cow::Borrowed(line);
    }
    std::borrow::Cow::Borrowed(&line[1..])
}

/// Is this line of the form `>*From `?
///
/// The body-line escape rule for Mboxrd applies to any line consisting
/// of zero or more `>` characters followed by literal `From `.
fn matches_from_pattern(line: &[u8]) -> bool {
    let trimmed = strip_leading_gt(line);
    trimmed.starts_with(b"From ")
}

/// Is this line of the form `>+From `? (One or more `>`, then `From `.)
fn matches_escaped_from_pattern(line: &[u8]) -> bool {
    if !line.starts_with(b">") {
        return false;
    }
    matches_from_pattern(line)
}

fn strip_leading_gt(mut s: &[u8]) -> &[u8] {
    while let Some(b'>') = s.first() {
        s = &s[1..];
    }
    s
}

/// Detect the variant by sniffing a sample of message bytes.
///
/// Returns:
/// - `Some(Mboxcl)` if any sampled message has a `Content-Length:` header,
/// - `Some(Mboxrd)` if any body line looks like `>+From ` (escaped),
/// - `Some(Mboxo)` if neither signal appears.
///
/// Used by [`MboxVariant::Auto`][super::MboxVariant].
pub(super) fn sniff_variant(sample: &[u8]) -> Option<MboxVariant> {
    if sample.is_empty() {
        return None;
    }
    let mut saw_content_length = false;
    let mut saw_escaped_from = false;
    // Read line-by-line via slice splits.
    let mut in_headers = false;
    let mut just_saw_blank = true;
    for line in sample.split(|b| *b == b'\n') {
        let line = trim_cr(line);
        if line.starts_with(b"From ") && just_saw_blank {
            in_headers = true;
            just_saw_blank = false;
            continue;
        }
        if in_headers {
            if line.is_empty() {
                in_headers = false;
                just_saw_blank = true;
                continue;
            }
            // Case-insensitive prefix match on "Content-Length:"
            if line.len() >= 15 && line[..15].eq_ignore_ascii_case(b"Content-Length:") {
                saw_content_length = true;
            }
        } else if matches_escaped_from_pattern(line) {
            saw_escaped_from = true;
        }
        just_saw_blank = line.is_empty();
    }
    if saw_content_length {
        Some(MboxVariant::Mboxcl)
    } else if saw_escaped_from {
        Some(MboxVariant::Mboxrd)
    } else {
        Some(MboxVariant::Mboxo)
    }
}

fn trim_cr(line: &[u8]) -> &[u8] {
    if let Some((&b'\r', rest)) = line.split_last() {
        let _ = &b'\r';
        rest
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn mboxrd_escape_plain_from_line() {
        let line = b"From the depths";
        let escaped = escape_body_line(line, MboxVariant::Mboxrd);
        assert_eq!(escaped.as_ref(), b">From the depths");
    }

    #[test]
    fn mboxrd_escape_already_escaped_from_line() {
        let line = b">From the depths";
        let escaped = escape_body_line(line, MboxVariant::Mboxrd);
        assert_eq!(escaped.as_ref(), b">>From the depths");
    }

    #[test]
    fn mboxrd_no_escape_for_mid_line_from() {
        let line = b"hello From bob";
        let escaped = escape_body_line(line, MboxVariant::Mboxrd);
        assert_eq!(escaped.as_ref(), b"hello From bob");
    }

    #[test]
    fn mboxo_never_escapes() {
        let line = b"From the depths";
        let escaped = escape_body_line(line, MboxVariant::Mboxo);
        assert_eq!(escaped.as_ref(), b"From the depths");
    }

    #[test]
    fn mboxrd_unescape_round_trip() {
        let original = b">From bob";
        let escaped = escape_body_line(original, MboxVariant::Mboxrd);
        let unescaped = unescape_body_line(&escaped, MboxVariant::Mboxrd);
        assert_eq!(unescaped.as_ref(), original);
    }

    #[test]
    fn mboxrd_unescape_unescaped_line_is_noop() {
        let line = b"normal line";
        let unescaped = unescape_body_line(line, MboxVariant::Mboxrd);
        assert_eq!(unescaped.as_ref(), line);
    }

    #[test]
    fn sniff_empty_returns_none() {
        assert_eq!(sniff_variant(b""), None);
    }

    #[test]
    fn sniff_no_signals_returns_mboxo() {
        let sample = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       From: alice@example.com\r\n\
                       Subject: hi\r\n\r\n\
                       body\r\n\r\n";
        assert_eq!(sniff_variant(sample), Some(MboxVariant::Mboxo));
    }

    #[test]
    fn sniff_content_length_returns_mboxcl() {
        let sample = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       From: alice@example.com\r\n\
                       Content-Length: 5\r\n\r\n\
                       body\r\n\r\n";
        assert_eq!(sniff_variant(sample), Some(MboxVariant::Mboxcl));
    }

    #[test]
    fn sniff_escaped_from_returns_mboxrd() {
        let sample = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
                       From: alice@example.com\r\n\r\n\
                       >From the start\r\n\r\n";
        assert_eq!(sniff_variant(sample), Some(MboxVariant::Mboxrd));
    }
}
