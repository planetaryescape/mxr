//! Parsing the `:2,SRF` flag suffix from Maildir filenames.
//!
//! Per Courier's Maildir++ extension, filenames have the form:
//!
//! ```text
//! <unique><sep>2,<flag-chars>
//! ```
//!
//! where `<sep>` is `:` on POSIX and `-` (configurable) on Windows.
//! The `2,` is the "info version" tag. Flag characters are a subset
//! of `PRSTDF` (passed, replied, seen, trashed, draft, flagged).
//!
//! Dovecot extends this with keyword characters `a-z` (mapped to
//! user-defined flags). We round-trip them as-is — the caller's flags
//! field has no slot for keywords, but readers preserve the suffix
//! so a write-back doesn't lose them. v0.2.0 may add a `keywords`
//! field; v0.1.0 silently drops them.

use crate::raw_message::Flags;

/// Parse `unique`, base info-version, and flags from a filename.
///
/// Returns `None` if the filename has no separator (the message has
/// never been moved to `cur/` and bears no flag suffix). Returns
/// `Some((unique, Flags::empty()))` for files without an explicit flag
/// section.
pub(super) fn parse_filename(name: &str, separator: char) -> (String, Flags) {
    match name.find(separator) {
        None => (name.to_string(), Flags::empty()),
        Some(pos) => {
            let unique = &name[..pos];
            let info = &name[pos + 1..];
            let flags = if let Some(chars) = info.strip_prefix("2,") {
                parse_flag_chars(chars)
            } else {
                Flags::empty()
            };
            (unique.to_string(), flags)
        }
    }
}

fn parse_flag_chars(chars: &str) -> Flags {
    let mut f = Flags::empty();
    for c in chars.chars() {
        match c {
            'P' => f |= Flags::PASSED,
            'R' => f |= Flags::REPLIED,
            'S' => f |= Flags::SEEN,
            'T' => f |= Flags::TRASHED | Flags::DELETED,
            'D' => f |= Flags::DRAFT,
            'F' => f |= Flags::FLAGGED,
            // Dovecot keyword characters and any other unknown letters
            // are silently dropped in v0.1.0.
            _ => {}
        }
    }
    f
}

/// Inverse of [`parse_flag_chars`]: convert a [`Flags`] back into the
/// `:2,SRF`-style suffix. Returns the flag char sequence in canonical
/// alphabetical order (so writes are deterministic).
pub(super) fn format_flag_chars(flags: Flags) -> String {
    let mut out = String::new();
    // Canonical order per the Maildir++ doc: alphabetical.
    if flags.contains(Flags::DRAFT) {
        out.push('D');
    }
    if flags.contains(Flags::FLAGGED) {
        out.push('F');
    }
    if flags.contains(Flags::PASSED) {
        out.push('P');
    }
    if flags.contains(Flags::REPLIED) {
        out.push('R');
    }
    if flags.contains(Flags::SEEN) {
        out.push('S');
    }
    if flags.contains(Flags::TRASHED) || flags.contains(Flags::DELETED) {
        out.push('T');
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn no_separator_yields_empty_flags() {
        let (unique, flags) = parse_filename("1234.M567P89.host", ':');
        assert_eq!(unique, "1234.M567P89.host");
        assert!(flags.is_empty());
    }

    #[test]
    fn parses_seen_replied_flags() {
        let (unique, flags) = parse_filename("1234.M567P89.host:2,RS", ':');
        assert_eq!(unique, "1234.M567P89.host");
        assert_eq!(flags, Flags::SEEN | Flags::REPLIED);
    }

    #[test]
    fn ignores_unknown_chars() {
        let (_, flags) = parse_filename("u:2,Sa", ':');
        assert_eq!(flags, Flags::SEEN);
    }

    #[test]
    fn t_maps_to_both_trashed_and_deleted() {
        let (_, flags) = parse_filename("u:2,T", ':');
        assert!(flags.contains(Flags::TRASHED));
        assert!(flags.contains(Flags::DELETED));
    }

    #[test]
    fn windows_separator() {
        let (unique, flags) = parse_filename("u-2,S", '-');
        assert_eq!(unique, "u");
        assert_eq!(flags, Flags::SEEN);
    }

    #[test]
    fn format_flag_chars_canonical_order() {
        let f = Flags::FLAGGED | Flags::SEEN | Flags::REPLIED;
        assert_eq!(format_flag_chars(f), "FRS");
    }

    #[test]
    fn format_then_parse_roundtrip() {
        let f = Flags::DRAFT | Flags::SEEN | Flags::FLAGGED;
        let chars = format_flag_chars(f);
        let parsed = parse_flag_chars(&chars);
        assert_eq!(parsed, f);
    }
}
