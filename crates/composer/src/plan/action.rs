//! Which action a request means, from the draft's state (blueprint 34, "What
//! you get depends on what you've written"), and whether the selection it
//! carries can be used.

use mxr_protocol::{AssistActionData, TextRangeData};
use std::ops::Range;

/// The most text writing help takes. The user's own words are never cut to fit
/// the prompt, so they are bounded up front instead.
pub const MAX_TEXT_BYTES: usize = 12_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub action: AssistActionData,
    /// The part to change; only an `EditSelection` carries one.
    pub selection: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("the selection {start}..{end} ends before it starts")]
    SelectionBackwards { start: u32, end: u32 },
    #[error("the selection {start}..{end} runs past the end of the text, which is {len} bytes")]
    SelectionPastEnd { start: u32, end: u32, len: usize },
    #[error(
        "the selection offset {offset} falls inside a character; \
         selections are byte offsets that sit between whole characters"
    )]
    SelectionNotOnBoundary { offset: u32 },
    #[error("select the part to change first, or pick another action")]
    NothingSelected,
    #[error(
        "there is no text to {verb}; type something first, or ask for the whole message to be \
         written"
    )]
    NothingTyped { verb: &'static str },
    #[error(
        "the text is {len} bytes, over the {MAX_TEXT_BYTES} byte limit writing help takes; \
         shorten it or work on part of it"
    )]
    TextTooLong { len: usize },
}

/// Settle the action and check the selection. An explicit action wins; with
/// none, an empty draft is written, typed text is finished, and typed text
/// with a selection is edited. Polish happens only when asked for.
pub fn resolve(
    explicit: Option<AssistActionData>,
    text: &str,
    selection: Option<TextRangeData>,
) -> Result<Resolved, PlanError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(PlanError::TextTooLong { len: text.len() });
    }
    let selection = selection.map(|range| validate(text, range)).transpose()?;
    // A cursor with nothing selected, or only blank space, is no selection.
    let selection = selection.filter(|range| !text[range.clone()].trim().is_empty());
    let typed = !text.trim().is_empty();
    let action = explicit.unwrap_or_else(|| match (typed, selection.is_some()) {
        (false, _) => AssistActionData::Write,
        (true, false) => AssistActionData::Finish,
        (true, true) => AssistActionData::EditSelection,
    });
    match action {
        AssistActionData::Write => Ok(Resolved {
            action,
            selection: None,
        }),
        AssistActionData::Finish | AssistActionData::Polish if !typed => {
            Err(PlanError::NothingTyped {
                verb: if action == AssistActionData::Finish {
                    "finish"
                } else {
                    "polish"
                },
            })
        }
        AssistActionData::Finish | AssistActionData::Polish => Ok(Resolved {
            action,
            selection: None,
        }),
        AssistActionData::EditSelection => match selection {
            Some(selection) => Ok(Resolved {
                action,
                selection: Some(selection),
            }),
            None => Err(PlanError::NothingSelected),
        },
    }
}

fn validate(text: &str, range: TextRangeData) -> Result<Range<usize>, PlanError> {
    let TextRangeData { start, end } = range;
    if start > end {
        return Err(PlanError::SelectionBackwards { start, end });
    }
    let (from, to) = (start as usize, end as usize);
    if to > text.len() {
        return Err(PlanError::SelectionPastEnd {
            start,
            end,
            len: text.len(),
        });
    }
    for offset in [start, end] {
        if !text.is_char_boundary(offset as usize) {
            return Err(PlanError::SelectionNotOnBoundary { offset });
        }
    }
    Ok(from..to)
}

/// Words to aim for. Writing starts from the voice's usual length; finishing
/// must hold what is typed plus what it still needs.
pub fn target_words(base: u32, action: AssistActionData, typed_words: u32) -> u32 {
    match action {
        AssistActionData::Write => base,
        AssistActionData::Finish => base.max(typed_words.saturating_add(20)),
        AssistActionData::Polish | AssistActionData::EditSelection => {
            typed_words.saturating_add(20)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: u32, end: u32) -> Option<TextRangeData> {
        Some(TextRangeData { start, end })
    }

    #[test]
    fn the_state_of_the_draft_picks_the_action() {
        let pick = |text, selection| resolve(None, text, selection).unwrap().action;
        assert_eq!(pick("", None), AssistActionData::Write);
        assert_eq!(pick("  \n ", None), AssistActionData::Write);
        assert_eq!(pick("Thursday works.", None), AssistActionData::Finish);
        assert_eq!(
            pick("Thursday works. Numbers by wed.", range(16, 31)),
            AssistActionData::EditSelection
        );
    }

    #[test]
    fn a_cursor_or_blank_selection_is_no_selection() {
        let finish = resolve(None, "Thursday works.", range(4, 4)).unwrap();
        assert_eq!(finish.action, AssistActionData::Finish);
        assert_eq!(finish.selection, None);
        let blank = resolve(None, "One.  Two.", range(4, 6)).unwrap();
        assert_eq!(blank.action, AssistActionData::Finish);
    }

    #[test]
    fn an_explicit_action_wins() {
        let polish = resolve(
            Some(AssistActionData::Polish),
            "Thursday works.",
            range(0, 8),
        )
        .unwrap();
        assert_eq!(polish.action, AssistActionData::Polish);
        assert_eq!(polish.selection, None, "polish works on the whole text");
        let write = resolve(Some(AssistActionData::Write), "Thursday works.", None).unwrap();
        assert_eq!(write.action, AssistActionData::Write);
    }

    #[test]
    fn an_action_that_needs_text_or_a_selection_says_so() {
        assert_eq!(
            resolve(Some(AssistActionData::Finish), "", None).unwrap_err(),
            PlanError::NothingTyped { verb: "finish" }
        );
        assert_eq!(
            resolve(Some(AssistActionData::Polish), " ", None).unwrap_err(),
            PlanError::NothingTyped { verb: "polish" }
        );
        assert_eq!(
            resolve(Some(AssistActionData::EditSelection), "Hi there.", None).unwrap_err(),
            PlanError::NothingSelected
        );
    }

    #[test]
    fn a_selection_off_a_char_boundary_is_a_clear_error() {
        // "é" is two bytes: offset 1 is inside it.
        let error = resolve(None, "é works", range(1, 3)).unwrap_err();
        assert_eq!(error, PlanError::SelectionNotOnBoundary { offset: 1 });
        assert!(error.to_string().contains("inside a character"));
        let error = resolve(None, "works é", range(0, 7)).unwrap_err();
        assert_eq!(error, PlanError::SelectionNotOnBoundary { offset: 7 });
    }

    #[test]
    fn a_selection_outside_the_text_is_refused() {
        assert_eq!(
            resolve(None, "Hi.", range(2, 1)).unwrap_err(),
            PlanError::SelectionBackwards { start: 2, end: 1 }
        );
        assert_eq!(
            resolve(None, "Hi.", range(0, 9)).unwrap_err(),
            PlanError::SelectionPastEnd {
                start: 0,
                end: 9,
                len: 3
            }
        );
        // Checked even when the action would not read it.
        assert!(resolve(Some(AssistActionData::Write), "Hi.", range(0, 9)).is_err());
    }

    #[test]
    fn text_over_the_limit_is_refused_not_cut() {
        let long = "word ".repeat(MAX_TEXT_BYTES);
        assert!(matches!(
            resolve(None, &long, None),
            Err(PlanError::TextTooLong { .. })
        ));
    }

    #[test]
    fn finishing_aims_past_what_is_typed() {
        assert_eq!(target_words(70, AssistActionData::Write, 0), 70);
        assert_eq!(target_words(70, AssistActionData::Finish, 10), 70);
        assert_eq!(target_words(70, AssistActionData::Finish, 120), 140);
        assert_eq!(target_words(70, AssistActionData::Polish, 50), 70);
    }
}
