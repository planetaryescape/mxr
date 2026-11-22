//! Split the user's writable text from a signature or forwarded message.

/// The line that opens the forwarded part of a body (`mxr-compose` writes it).
pub const FORWARD_HEADER: &str = "---------- Forwarded message ----------";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyRegions<'a> {
    /// The blank space before the text, as it was written.
    pub lead: &'a str,
    /// The user's text, without blank space at either end.
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
    let text = before.trim();
    if text.is_empty() {
        return BodyRegions {
            lead: "",
            text: "",
            gap: before,
            rest,
        };
    }
    let lead_len = before.len() - before.trim_start().len();
    BodyRegions {
        lead: &before[..lead_len],
        text,
        gap: &before[lead_len + text.len()..],
        rest,
    }
}

impl BodyRegions<'_> {
    /// The body with `text` in place of the user's text, the blank space around
    /// it written back as it was stored: the lead, the text, the original gap
    /// (the body's trailing blank space when nothing follows) and the rest.
    pub fn with_text(&self, text: &str) -> String {
        let gap = if self.gap.is_empty() && !self.rest.is_empty() {
            "\n\n"
        } else {
            self.gap
        };
        format!("{}{}{gap}{}", self.lead, text.trim(), self.rest)
    }
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
    fn replacing_the_text_writes_back_the_original_trailing_blank_space() {
        let regions = split_body("Keep cat safe.  \n");
        assert_eq!(regions.text, "Keep cat safe.");
        assert_eq!(regions.with_text("Keep cot safe."), "Keep cot safe.  \n");
        assert_eq!(
            regions.with_text("Keep cot safe.  \n"),
            "Keep cot safe.  \n"
        );
        assert_eq!(regions.with_text("\nKeep cot safe."), "Keep cot safe.  \n");
        let signed = split_body("Hi  \n\n-- \nBK");
        assert_eq!(signed.with_text("Hello"), "Hello  \n\n-- \nBK");
    }

    #[test]
    fn a_body_that_starts_with_its_signature_has_no_text() {
        let regions = split_body("-- \nAlex");
        assert_eq!(regions.text, "");
        assert_eq!(regions.rest, "-- \nAlex");
        assert_eq!(regions.with_text("Hi."), "Hi.\n\n-- \nAlex");
    }
}
