//! Small text utilities shared across crates.

/// Truncate `s` to at most `max_bytes` bytes without panicking inside a
/// multi-byte UTF-8 character.
///
/// `String::truncate` asserts the cut lands on a char boundary, so using it
/// with a byte budget panics as soon as the budget falls inside an emoji or
/// accented character — email bodies hit this constantly. This walks the cut
/// back to the nearest boundary instead.
pub fn truncate_to_char_boundary(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut cut = max_bytes;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
}

#[cfg(test)]
mod tests {
    use super::truncate_to_char_boundary;

    #[test]
    fn no_op_when_within_budget() {
        let mut s = "hello".to_string();
        truncate_to_char_boundary(&mut s, 10);
        assert_eq!(s, "hello");
    }

    #[test]
    fn cuts_ascii_exactly_at_budget() {
        let mut s = "hello world".to_string();
        truncate_to_char_boundary(&mut s, 5);
        assert_eq!(s, "hello");
    }

    #[test]
    fn backs_off_mid_codepoint_cut() {
        // "é" is two bytes; a budget of 3 lands mid-character.
        let mut s = "aaéé".to_string();
        truncate_to_char_boundary(&mut s, 3);
        assert_eq!(s, "aa");

        // Four-byte emoji; every interior offset must be safe.
        let base = "x🦀y".to_string();
        for budget in 0..=base.len() {
            let mut s = base.clone();
            truncate_to_char_boundary(&mut s, budget);
            assert!(s.len() <= budget);
            assert!(base.starts_with(&s));
        }
    }

    #[test]
    fn zero_budget_empties_the_string() {
        let mut s = "🦀".to_string();
        truncate_to_char_boundary(&mut s, 0);
        assert_eq!(s, "");
    }
}
