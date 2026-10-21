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

pub fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Curly quotes and apostrophes compare equal to straight ones; models
/// swap them freely. The mapping is one char for one char, so a match in
/// the folded text is the same span in the unfolded one.
pub fn fold_quotes(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
        other => other,
    }
}

/// `phrase` as it is written in `text`, ignoring how whitespace is laid out,
/// curly versus straight quote marks, and letter case: the original slice,
/// or `None` when the text doesn't say it. Model output is only shown when
/// it passes this.
pub fn find_phrase_ignoring_case<'a>(text: &'a str, phrase: &str) -> Option<&'a str> {
    let fold = |c: char| fold_quotes(c.to_lowercase().next().unwrap_or(c));
    let wanted: Vec<char> = collapse_whitespace(phrase).chars().map(fold).collect();
    if wanted.is_empty() {
        return None;
    }
    find_normalized(text, &wanted, fold).map(|span| &text[span])
}

/// Model text found in `source`, as the source writes it (whitespace
/// collapsed for display), or `None` when it isn't there or is longer than
/// `max_chars`. Trailing punctuation the model added is ignored.
pub fn find_verbatim(source: &str, model_text: &str, max_chars: usize) -> Option<String> {
    let wanted = model_text.trim().trim_end_matches(['.', ',', ';', '!']);
    let slice = find_phrase_ignoring_case(source, wanted)?;
    let text = collapse_whitespace(slice);
    (text.chars().count() <= max_chars).then_some(text)
}

/// Byte range in `text` whose whitespace-collapsed, quote-folded form equals
/// `wanted` (already collapsed and folded).
pub fn find_normalized(
    text: &str,
    wanted: &[char],
    fold: impl Fn(char) -> char,
) -> Option<std::ops::Range<usize>> {
    // Each normalized char with the byte range of the original it stands for.
    let mut normalized: Vec<(char, usize, usize)> = Vec::new();
    for (index, c) in text.char_indices() {
        let end = index + c.len_utf8();
        if c.is_whitespace() {
            match normalized.last_mut() {
                Some((' ', _, last_end)) => *last_end = end,
                _ => normalized.push((' ', index, end)),
            }
        } else {
            normalized.push((fold(c), index, end));
        }
    }
    let start = normalized
        .windows(wanted.len())
        .position(|window| window.iter().map(|(c, _, _)| *c).eq(wanted.iter().copied()))?;
    let first = normalized[start];
    let last = normalized[start + wanted.len() - 1];
    Some(first.1..last.2)
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
