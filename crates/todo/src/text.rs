//! Small text helpers the detectors and the daemon's labels share.

/// The largest char boundary at or before `index`, so a byte offset into
/// mail text never splits a character.
pub fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// At most `max` bytes of `value`, cut on a char boundary.
pub fn truncate(value: &str, max: usize) -> &str {
    &value[..floor_char_boundary(value, max)]
}

/// At most `max` characters, whitespace collapsed, with an ellipsis when
/// cut.
pub fn clip(value: &str, max: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// "Spotify" from "spotify".
pub fn capitalise(value: &str) -> String {
    let mut chars = value.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// "renew car insurance" from "Renew car insurance", keeping acronyms
/// ("MOT", "TV licence") as written.
pub fn lower_first(value: &str) -> String {
    let value = value.trim();
    let mut chars = value.chars();
    match (chars.next(), chars.clone().next()) {
        (Some(first), Some(second)) if first.is_uppercase() && second.is_uppercase() => {
            value.to_string()
        }
        (Some(first), _) => first.to_lowercase().chain(chars).collect(),
        (None, _) => String::new(),
    }
}

/// A display name without the tracking id some senders glue onto it:
/// "TV Licensing" from "TV Licensing2026-09-27887d70…" or
/// "TV Licensing349212". Only a run glued straight onto a letter, starting
/// with a digit, made of digits, hex letters and dashes, at least six long
/// with at least four digits, counts as an id; "Studio 54" and "R2D2" keep
/// theirs.
pub fn strip_glued_id(name: &str) -> &str {
    let name = name.trim();
    let id_char = |c: char| c.is_ascii_digit() || matches!(c, 'a'..='f' | 'A'..='F' | '-');
    let mut start = name.len();
    for (index, c) in name.char_indices().rev() {
        if !id_char(c) {
            break;
        }
        start = index;
    }
    // The run may begin with hex letters that belong to the word ("Cafe"),
    // so the id starts at its first digit.
    let Some(offset) = name[start..].find(|c: char| c.is_ascii_digit()) else {
        return name;
    };
    let start = start + offset;
    let id = &name[start..];
    let glued = name[..start]
        .chars()
        .next_back()
        .is_some_and(char::is_alphabetic);
    let digits = id.chars().filter(char::is_ascii_digit).count();
    if glued && id.len() >= 6 && digits >= 4 {
        name[..start].trim_end()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_on_char_boundaries_and_keeps_acronyms() {
        assert_eq!(truncate("£142", 1), "");
        assert_eq!(truncate("£142", 3), "£1");
        assert_eq!(clip("a  b   c", 10), "a b c");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(lower_first("Renew car insurance"), "renew car insurance");
        assert_eq!(lower_first("MOT due"), "MOT due");
        assert_eq!(capitalise("spotify"), "Spotify");
    }

    #[test]
    fn a_tracking_id_glued_onto_a_name_is_dropped() {
        assert_eq!(
            strip_glued_id("TV Licensing2026-09-27887d708f6e134fe8bd5c9e0584954c5b"),
            "TV Licensing"
        );
        assert_eq!(strip_glued_id("TV Licensing349212"), "TV Licensing");
        assert_eq!(strip_glued_id("Acme Billing 2026"), "Acme Billing 2026");
        assert_eq!(strip_glued_id("Studio54"), "Studio54");
        assert_eq!(strip_glued_id("R2D2"), "R2D2");
        assert_eq!(strip_glued_id("Cafe"), "Cafe");
        assert_eq!(strip_glued_id("Priya Shah"), "Priya Shah");
        assert_eq!(strip_glued_id("123456"), "123456");
    }
}
