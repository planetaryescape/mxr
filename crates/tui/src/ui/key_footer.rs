//! The key line under a mode. Help is how keys are found (blueprint 35,
//! teaching), so when the line is wider than its space the entries that may
//! go are dropped first, last listed first: each footer lists its keys in
//! the order help does, most used first. The entries marked to keep (the
//! place's primary verb, undo, help) stay. Only when those alone overflow is
//! the line cut.

use crate::ui::sanitize::truncate;

/// One footer entry, "key verb", and whether it must stay.
pub(crate) struct FooterKey {
    pub text: String,
    pub keep: bool,
}

impl FooterKey {
    pub(crate) fn keep(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            keep: true,
        }
    }

    pub(crate) fn optional(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            keep: false,
        }
    }
}

/// The entries joined by `separator` in at most `width` characters,
/// dropping optional entries from the end until they fit.
pub(crate) fn fit(entries: &[FooterKey], separator: &str, width: usize) -> String {
    let mut shown: Vec<&FooterKey> = entries
        .iter()
        .filter(|entry| !entry.text.is_empty())
        .collect();
    let joined = |shown: &[&FooterKey]| {
        shown
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join(separator)
    };
    while joined(&shown).chars().count() > width {
        let Some(at) = shown.iter().rposition(|entry| !entry.keep) else {
            break;
        };
        shown.remove(at);
    }
    truncate(&joined(&shown), width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<FooterKey> {
        vec![
            FooterKey::keep("\u{21b5} open"),
            FooterKey::optional("e done"),
            FooterKey::optional("a reply all"),
            FooterKey::keep("u undo"),
            FooterKey::optional("f forward"),
            FooterKey::keep("? keys"),
        ]
    }

    #[test]
    fn everything_shows_when_it_fits() {
        assert_eq!(
            fit(&entries(), "  ", 200),
            "\u{21b5} open  e done  a reply all  u undo  f forward  ? keys"
        );
    }

    #[test]
    fn the_last_optional_entries_go_first_and_the_kept_ones_stay() {
        assert_eq!(
            fit(&entries(), "  ", 40),
            "\u{21b5} open  e done  u undo  ? keys"
        );
        assert_eq!(fit(&entries(), "  ", 29), "\u{21b5} open  u undo  ? keys");
    }

    #[test]
    fn only_kept_entries_that_overflow_are_cut() {
        assert_eq!(fit(&entries(), "  ", 12), "\u{21b5} open  u u\u{2026}");
    }
}
