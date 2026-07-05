use std::borrow::Cow;

/// Remove terminal-dangerous control characters from mail-controlled text.
///
/// Keeps `\n` (U+000A) and `\t` (U+0009); strips all other C0 controls
/// (U+0000–U+001F), DEL (U+007F), and C1 controls (U+0080–U+009F).
/// Printable content — ASCII, wide chars, emoji, and combining marks —
/// passes through untouched.
///
/// Returns `Cow::Borrowed` when nothing needs stripping so the common
/// (safe) path is allocation-free.
pub(crate) fn strip_control_chars(input: &str) -> Cow<'_, str> {
    // Fast path: scan for any ASCII-range byte that needs stripping before
    // allocating. We check only bytes < 0x80 here because multi-byte UTF-8
    // sequences use bytes 0x80..=0xFF as continuation/lead bytes — those
    // are safe and must not trigger stripping. C1 controls (U+0080–U+009F)
    // are encoded as two-byte sequences in valid UTF-8; we catch them in
    // the char-level slow path below.
    let needs_strip = input.bytes().any(|b| {
        matches!(b,
            0x00..=0x08   // C0: NUL–BS (skip \t 0x09)
            | 0x0B..=0x0C // C0: VT, FF (skip \n 0x0A)
            | 0x0E..=0x1F // C0: SO–US
            | 0x7F        // DEL
        )
    }) || input.chars().any(|c| matches!(c as u32, 0x80..=0x9F));

    if !needs_strip {
        return Cow::Borrowed(input);
    }

    // Slow path: rebuild keeping only safe chars.
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        let cp = c as u32;
        let keep = match cp {
            0x09 | 0x0A => true,         // tab, newline — always keep
            0x00..=0x1F => false,        // C0 controls (excl. tab/LF above)
            0x7F => false,               // DEL
            0x80..=0x9F => false,        // C1 controls
            _ => true,
        };
        if keep {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_plain_text_is_borrowed() {
        let s = "Hello, world!\tTabbed\nNewline 你好 🎉 café";
        let result = strip_control_chars(s);
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "plain text should be returned as Cow::Borrowed"
        );
        assert_eq!(result.as_ref(), s);
    }

    #[test]
    fn ansi_csi_stripped() {
        let input = "\x1b[31mred\x1b[0m text";
        let result = strip_control_chars(input);
        assert!(!result.contains('\x1b'), "CSI ESC must be stripped");
        assert!(result.contains("red"), "printable text must survive");
        assert!(result.contains("text"), "printable text must survive");
    }

    #[test]
    fn osc_with_bel_terminator_stripped() {
        let input = "\x1b]0;title\x07 normal";
        let result = strip_control_chars(input);
        assert!(!result.contains('\x1b'), "OSC ESC must be stripped");
        // BEL (0x07) is also a C0 control and must be stripped
        assert!(!result.as_bytes().contains(&0x07), "BEL must be stripped");
        assert!(result.contains("normal"), "surrounding text must survive");
    }

    #[test]
    fn osc_with_st_terminator_stripped() {
        // ST = ESC \ (0x1b 0x5c)
        let input = "\x1b]8;;http://x\x1b\\ after";
        let result = strip_control_chars(input);
        assert!(!result.contains('\x1b'), "OSC ESC must be stripped");
        assert!(result.contains("after"), "trailing text must survive");
    }

    #[test]
    fn bare_esc_stripped() {
        let input = "before\x1bafter";
        let result = strip_control_chars(input);
        assert!(!result.contains('\x1b'));
        assert_eq!(result.as_ref(), "beforeafter");
    }

    #[test]
    fn c1_control_bytes_stripped() {
        // U+009B is CSI in C1; encoded as 0xC2 0x9B in UTF-8
        let input = "before\u{009b}after";
        let result = strip_control_chars(input);
        assert!(!result.contains('\u{009b}'), "C1 CSI must be stripped");
        assert_eq!(result.as_ref(), "beforeafter");
    }

    #[test]
    fn del_stripped() {
        let input = "be\x7fore";
        let result = strip_control_chars(input);
        assert!(!result.as_bytes().contains(&0x7f), "DEL must be stripped");
        assert_eq!(result.as_ref(), "beore");
    }

    #[test]
    fn tabs_and_newlines_preserved() {
        let s = "col1\tcol2\nline2";
        let result = strip_control_chars(s);
        assert!(matches!(result, Cow::Borrowed(_)), "tab+newline text is Cow::Borrowed");
        assert_eq!(result.as_ref(), s);
    }

    #[test]
    fn wide_chars_emoji_combining_preserved() {
        let s = "CJK: 你好世界 emoji: 🎉🦀 combining: café naïve";
        let result = strip_control_chars(s);
        assert!(matches!(result, Cow::Borrowed(_)), "rich unicode should be Cow::Borrowed");
        assert_eq!(result.as_ref(), s);
    }
}
