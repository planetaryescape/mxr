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
}
