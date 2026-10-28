use sha2::{Digest, Sha256};

/// The hash a proposal is made against: hex SHA-256 of the user's text with
/// the blank space at both ends ignored and Windows line endings read as `\n`.
/// A client that trims what it sends, an editor that adds a final newline and
/// a file saved with CRLF all hash the same text.
pub fn text_hash(text: &str) -> String {
    let text = text.trim().replace("\r\n", "\n");
    base16ct::lower::encode_string(&Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_ignores_blank_space_at_both_ends_and_line_ending_style_only() {
        assert_eq!(text_hash("Hello\n"), text_hash("Hello"));
        assert_eq!(text_hash("One.\r\nTwo.\r\n"), text_hash("One.\nTwo."));
        assert_ne!(text_hash("Hello"), text_hash("hello"));
        assert_eq!(text_hash("\n\n Hello"), text_hash("Hello"));
        assert_eq!(text_hash("").len(), 64);
    }
}
