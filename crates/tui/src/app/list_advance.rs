//! Where the selection goes when the open row leaves its list: the next
//! row still there, else the previous one, else nothing. The web client
//! has the same rule (`apps/web/src/lib/listAdvance.ts`).

/// The row to select after `removed` left the list. `before` is the list
/// as it was, in display order; `still_there` says whether a row is in the
/// list now. `None` when `removed` wasn't in `before` or nothing near it is
/// left.
pub(crate) fn next_after_removal<'a, T: PartialEq>(
    before: &'a [T],
    removed: &T,
    still_there: impl Fn(&T) -> bool,
) -> Option<&'a T> {
    let at = before.iter().position(|id| id == removed)?;
    let kept = |id: &&T| *id != removed && still_there(id);
    before[at + 1..]
        .iter()
        .find(kept)
        .or_else(|| before[..at].iter().rev().find(kept))
}

#[cfg(test)]
mod tests {
    use super::next_after_removal;

    #[test]
    fn the_next_row_then_the_previous_then_none() {
        let before = ["a", "b", "c"];
        assert_eq!(next_after_removal(&before, &"b", |_| true), Some(&"c"));
        assert_eq!(next_after_removal(&before, &"c", |_| true), Some(&"b"));
        assert_eq!(next_after_removal(&before, &"a", |id| *id == "a"), None);
    }

    #[test]
    fn rows_that_left_too_are_skipped() {
        let before = ["a", "b", "c", "d"];
        assert_eq!(
            next_after_removal(&before, &"b", |id| *id != "c"),
            Some(&"d")
        );
        assert_eq!(
            next_after_removal(&before, &"c", |id| *id == "a"),
            Some(&"a")
        );
    }

    #[test]
    fn a_row_that_was_never_listed_moves_nothing() {
        assert_eq!(next_after_removal(&["a"], &"z", |_| true), None);
    }
}
