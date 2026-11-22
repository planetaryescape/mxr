
use crate::ui::sanitize::truncate;

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
