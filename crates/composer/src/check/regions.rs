//! The regions of a draft body (blueprint 34, "One shared model"). Writing help
//! reads and proposes only the user's text. The signature block and the
//! forward header stay as they are: they never go to the model and never come
//! back changed.

use sha2::{Digest, Sha256};

/// The line that opens the forwarded part of a body (`mxr-compose` writes it).
pub const FORWARD_HEADER: &str = "---------- Forwarded message ----------";

/// A body split into the user's text and what follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyRegions<'a> {
    /// The user's text, without the blank space after it.
    pub text: &'a str,
    /// The blank space between the text and the rest, as it was written.
    pub gap: &'a str,
    /// The signature block and forward header, from the first of them to the
    /// end of the body. Empty when the body has neither.
    pub rest: &'a str,
}

/// Split `body` where the signature (`-- ` on a line of its own, RFC 3676) or
/// the forward header starts, whichever comes first.
pub fn split_body(body: &str) -> BodyRegions<'_> {
    let mut offset = 0;
    let mut cut = body.len();
    for line in body.split_inclusive('\n') {
        let bare = line.trim_end();
        if bare == "--" || bare.starts_with(FORWARD_HEADER) {
            cut = offset;
            break;
        }
        offset += line.len();
    }
    let (before, rest) = body.split_at(cut);
    let text = before.trim_end();
    BodyRegions {
        text,
        gap: &before[text.len()..],
        rest,
    }
}

impl BodyRegions<'_> {
    /// The body with `text` in place of the user's text.
    pub fn with_text(&self, text: &str) -> String {
        let text = text.trim_end();
        if self.rest.is_empty() {
            return text.to_string();
        }
        // A body that began with its signature has no blank space to keep.
        let gap = if self.gap.is_empty() {
            "\n\n"
        } else {
            self.gap
        };
        format!("{text}{gap}{}", self.rest)
    }
}

/// The hash a proposal is made against: hex SHA-256 of the user's text with
/// the blank space after it ignored and Windows line endings read as `\n`, so
/// a trailing newline from a file or an editor, or a file saved with CRLF,
/// never makes a draft look changed.
pub fn text_hash(text: &str) -> String {
    let text = text.trim_end().replace("\r\n", "\n");
    base16ct::lower::encode_string(&Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_ends_where_the_signature_begins() {
        let regions = split_body("Thursday works.\n\n-- \nAlex\nFoundry");
        assert_eq!(regions.text, "Thursday works.");
        assert_eq!(regions.gap, "\n\n");
        assert_eq!(regions.rest, "-- \nAlex\nFoundry");
    }

    #[test]
    fn the_forward_header_ends_the_text_too() {
        let body = "Fyi.\n\n-- \nAlex\n\n---------- Forwarded message ----------\nFrom: a";
        assert_eq!(split_body(body).text, "Fyi.");
        let forward_only = "Fyi.\n\n---------- Forwarded message ----------";
        assert_eq!(split_body(forward_only).rest, FORWARD_HEADER);
    }

    #[test]
    fn a_body_with_neither_is_all_text() {
        let regions = split_body("Just this.\n");
        assert_eq!(regions.text, "Just this.");
        assert_eq!(regions.rest, "");
        assert_eq!(regions.with_text("New."), "New.");
    }

    #[test]
    fn a_dash_inside_a_line_is_not_a_signature() {
        let body = "Two -- no, three things.\n--- not a rule";
        assert_eq!(split_body(body).text, body);
    }

    #[test]
    fn a_body_that_starts_with_its_signature_has_no_text() {
        let regions = split_body("-- \nAlex");
        assert_eq!(regions.text, "");
        assert_eq!(regions.rest, "-- \nAlex");
        assert_eq!(regions.with_text("Hi."), "Hi.\n\n-- \nAlex");
    }

    #[test]
    fn replacing_the_text_keeps_the_signature_and_the_spacing() {
        let body = "Old.\n-- \nAlex";
        assert_eq!(
            split_body(body).with_text("New one.\n"),
            "New one.\n-- \nAlex"
        );
    }

    #[test]
    fn the_hash_ignores_trailing_blank_space_and_line_ending_style_only() {
        assert_eq!(text_hash("Hello\n"), text_hash("Hello"));
        assert_eq!(text_hash("One.\r\nTwo.\r\n"), text_hash("One.\nTwo."));
        assert_ne!(text_hash("Hello"), text_hash("hello"));
        assert_ne!(text_hash(" Hello"), text_hash("Hello"));
        assert_eq!(text_hash("").len(), 64);
    }
}
