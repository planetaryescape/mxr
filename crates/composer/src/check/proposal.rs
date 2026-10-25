//! Check a model's answer and turn it into a proposal (blueprint 34, "Check"):
//! take out regions that are not the model's to write, keep an edit inside its
//! selection, turn figures nobody supplied into gaps, find the gaps, report
//! which of the user's sentences moved, and diff from what they typed.

use super::split_body;
use super::text::{invented_spans, sentences};
use mxr_protocol::{AssistActionData, GapData, TextChangeData, TextChangeKindData};
use similar::{ChangeTag, DiffTag, TextDiff};
use std::collections::HashSet;
use std::ops::Range;

const OPEN_GAP: &str = "[[?:";

/// What the check stage reads.
#[derive(Debug, Clone)]
pub struct ProposalInput<'a> {
    pub action: AssistActionData,
    /// The user's text as the request carried it.
    pub typed: &'a str,
    /// The part to change, for `EditSelection`; a byte range of `typed`.
    pub selection: Option<Range<usize>>,
    /// What the model wrote, after clean-up.
    pub output: &'a str,
    /// Everything the model could have taken a figure from besides the typed
    /// text: the conversation it was shown, the instruction, today's date.
    pub known: &'a str,
}

/// The proposal's text and what the check found in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub text: String,
    pub changes: Vec<TextChangeData>,
    pub gaps: Vec<GapData>,
    /// Which of the user's sentences were changed or dropped, by position.
    pub yours_changed: Vec<u32>,
    /// What the check had to do to the model's answer, for the activity log.
    pub fixes: Fixes,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fixes {
    /// The model wrote a signature or forward header; it was removed.
    pub regions_removed: bool,
    /// The model changed text outside the selection; it was put back.
    pub outside_reverted: bool,
    /// Figures nobody supplied that became gaps.
    pub figures_to_gaps: u32,
}

pub fn check_proposal(input: &ProposalInput<'_>) -> Checked {
    let mut fixes = Fixes::default();

    let regions = split_body(input.output);
    fixes.regions_removed = !regions.rest.is_empty();
    let mut text = regions.text.to_string();

    // Where the model may have written. Everything outside it is the user's
    // and no later pass touches it.
    let mut editable = None;
    if input.action == AssistActionData::EditSelection {
        if let Some(selection) = input.selection.clone() {
            let kept = keep_edit_inside(input.typed, selection, &text);
            fixes.outside_reverted = kept.reverted;
            editable = Some(kept.editable);
            text = kept.text;
        }
    }

    let known = [input.typed, input.known].join("\n");
    let (rewritten, figures) = figures_to_gaps(&text, &known, editable);
    text = rewritten;
    fixes.figures_to_gaps = figures;

    Checked {
        // An edit keeps what is outside its selection byte for byte, blank
        // space included, so the diff starts from the text as typed.
        changes: word_changes(
            if input.action == AssistActionData::EditSelection {
                input.typed
            } else {
                input.typed.trim_end()
            },
            &text,
        ),
        gaps: gap_spans(&text),
        yours_changed: sentences_changed(input.typed, &text),
        text,
        fixes,
    }
}

/// The user's sentences (counted as `check` counts them: split at `.`, `!`,
/// `?` and line breaks, empty ones skipped) that `text` no longer has as
/// whole sentences.
fn sentences_changed(typed: &str, text: &str) -> Vec<u32> {
    let now: HashSet<String> = sentences(text).into_iter().collect();
    sentences(typed)
        .iter()
        .enumerate()
        .filter(|(_, sentence)| !now.contains(*sentence))
        .map(|(index, _)| u32::try_from(index).unwrap_or(u32::MAX))
        .collect()
}

/// Every `[[?: …]]` in `text` with its byte range. A marker left open ends
/// where the next one starts or at the end of its line, whichever comes first.
fn gap_spans(text: &str) -> Vec<GapData> {
    let mut gaps = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(OPEN_GAP) {
        let start = from + found;
        let inner_start = start + OPEN_GAP.len();
        let rest = &text[inner_start..];
        let close = rest.find("]]");
        let reopened = rest.find(OPEN_GAP);
        let (inner_end, end) = match close {
            Some(close) if reopened.is_none_or(|reopened| close < reopened) => {
                (inner_start + close, inner_start + close + 2)
            }
            _ => {
                let stop = [reopened, rest.find('\n')]
                    .into_iter()
                    .flatten()
                    .min()
                    .unwrap_or(rest.len());
                (inner_start + stop, inner_start + stop)
            }
        };
        gaps.push(GapData {
            label: text[inner_start..inner_end].trim().to_string(),
            start: u32::try_from(start).unwrap_or(u32::MAX),
            end: u32::try_from(end).unwrap_or(u32::MAX),
        });
        from = end.max(inner_start);
    }
    gaps
}

/// Word-level changes from `from` to `to`, runs of the same kind merged.
fn word_changes(from: &str, to: &str) -> Vec<TextChangeData> {
    let diff = TextDiff::from_words(from, to);
    let mut out: Vec<TextChangeData> = Vec::new();
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Equal => TextChangeKindData::Kept,
            ChangeTag::Insert => TextChangeKindData::Added,
            ChangeTag::Delete => TextChangeKindData::Removed,
        };
        match out.last_mut() {
            Some(last) if last.kind == kind => last.text.push_str(change.value()),
            _ => out.push(TextChangeData {
                kind,
                text: change.value().to_string(),
            }),
        }
    }
    out
}

/// The text after the selection check, and where in it the model was allowed
/// to write.
struct KeptEdit {
    text: String,
    editable: Range<usize>,
    reverted: bool,
}

/// Keep every byte outside `selection` as it was: the result starts with
/// `original[..selection.start]` and ends with `original[selection.end..]`,
/// whatever the model returned.
///
/// A model that kept both ends gets its answer as it is. Otherwise the answer
/// is diffed word by word against the original and a change is kept only when
/// the words it replaces lie wholly inside the selection; the rest is put
/// back. A selection that cuts a word in half therefore keeps that word as it
/// was unless the model left both ends alone. Blank space at either end of the
/// text counts as outside the selection and is kept too.
fn keep_edit_inside(original: &str, selection: Range<usize>, output: &str) -> KeptEdit {
    let prefix = &original[..selection.start];
    let suffix = &original[selection.end..];
    // The model's answer arrives trimmed, so blank space at the very ends is
    // not something it can be held to: it is put back as it was.
    let prefix_core = prefix.trim_start();
    let suffix_core = suffix.trim_end();
    let lead = &prefix[..prefix.len() - prefix_core.len()];
    let trail = &suffix[suffix_core.len()..];
    if output.len() >= prefix_core.len() + suffix_core.len()
        && output.starts_with(prefix_core)
        && output.ends_with(suffix_core)
    {
        let text = format!("{lead}{output}{trail}");
        return KeptEdit {
            editable: prefix.len()..text.len() - suffix.len(),
            text,
            reverted: false,
        };
    }

    let diff = TextDiff::from_words(original, output);
    let old = diff.old_slices();
    let new = diff.new_slices();
    let mut starts = Vec::with_capacity(old.len() + 1);
    let mut at = 0;
    for token in old {
        starts.push(at);
        at += token.len();
    }
    starts.push(at);

    let mut text = String::with_capacity(output.len());
    let mut reverted = false;
    for op in diff.ops() {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        let (from, to) = (starts[old_range.start], starts[old_range.end]);
        // An insertion at either edge is inside: it adds to the selection
        // without touching a byte outside it.
        let inside = selection.start <= from && to <= selection.end;
        if tag == DiffTag::Equal || !inside {
            old[old_range].iter().for_each(|token| text.push_str(token));
            reverted |= tag != DiffTag::Equal;
        } else {
            new[new_range].iter().for_each(|token| text.push_str(token));
        }
    }
    // The ends were never rewritten, so this holds; it is the guarantee, so a
    // surprise falls back to the user's own text.
    if !(text.starts_with(prefix) && text.ends_with(suffix)) {
        return KeptEdit {
            editable: selection,
            text: original.to_string(),
            reverted: true,
        };
    }
    KeptEdit {
        editable: prefix.len()..text.len() - suffix.len(),
        text,
        reverted,
    }
}

/// Replace each figure no input carries with a gap that shows the figure the
/// model guessed ("[[?: amount, the draft said £40]]"). A figure the model
/// already put in a gap of its own stays, and so does a figure outside
/// `editable` (the part of the text the model was allowed to write).
fn figures_to_gaps(text: &str, known: &str, editable: Option<Range<usize>>) -> (String, u32) {
    let existing = gap_spans(text);
    let inside_gap = |range: &Range<usize>| {
        existing
            .iter()
            .any(|gap| range.start < gap.end as usize && (gap.start as usize) < range.end)
    };
    let spans: Vec<(Range<usize>, String)> = invented_spans(text, known)
        .into_iter()
        .map(|span| {
            let range = widen(text, span.range.clone());
            let kind = figure_kind(&text[range.clone()], &span.token);
            (range, kind)
        })
        .filter(|(range, _)| !inside_gap(range))
        .filter(|(range, _)| {
            editable
                .as_ref()
                .is_none_or(|editable| editable.start <= range.start && range.end <= editable.end)
        })
        .collect();
    let mut out = text.to_string();
    for (range, kind) in spans.iter().rev() {
        let shown = text[range.clone()].to_string();
        out.replace_range(
            range.clone(),
            &format!("[[?: {kind}, the draft said {shown}]]"),
        );
    }
    (out, u32::try_from(spans.len()).unwrap_or(u32::MAX))
}

/// A figure with its currency sign or percent sign, which tokens drop.
fn widen(text: &str, range: Range<usize>) -> Range<usize> {
    let start = text[..range.start]
        .chars()
        .next_back()
        .filter(|c| matches!(c, '£' | '$' | '€' | '¥'))
        .map_or(range.start, |c| range.start - c.len_utf8());
    let end = if text[range.end..].starts_with('%') {
        range.end + 1
    } else {
        range.end
    };
    start..end
}

fn figure_kind(shown: &str, token: &str) -> String {
    let lower = token.to_lowercase();
    let kind = if shown.starts_with(['£', '$', '€', '¥']) {
        "amount"
    } else if shown.ends_with('%') {
        "percentage"
    } else if lower.ends_with("am") || lower.ends_with("pm") || token.contains(':') {
        "time"
    } else if ["st", "nd", "rd", "th"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        "date"
    } else {
        "number"
    };
    kind.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(
        action: AssistActionData,
        typed: &str,
        selection: Option<Range<usize>>,
        output: &str,
        known: &str,
    ) -> Checked {
        check_proposal(&ProposalInput {
            action,
            typed,
            selection,
            output,
            known,
        })
    }

    fn finish(typed: &str, output: &str, known: &str) -> Checked {
        check(AssistActionData::Finish, typed, None, output, known)
    }

    #[test]
    fn finish_that_keeps_every_sentence_reports_none_changed() {
        let typed = "Thursday works.\nNumbers by wed.";
        let output = "Hi Sam,\n\nThursday works.\nNumbers by wed.\n\nAlex";
        let checked = finish(typed, output, "Can we meet Thursday?");
        assert_eq!(checked.text, output);
        assert!(checked.yours_changed.is_empty());
        assert!(checked.gaps.is_empty());
        // The diff shows what was added around the user's words.
        let added: Vec<&str> = checked
            .changes
            .iter()
            .filter(|change| change.kind == TextChangeKindData::Added)
            .map(|change| change.text.as_str())
            .collect();
        assert_eq!(added, ["Hi Sam,\n\n", "\n\nAlex"]);
        let kept: String = checked
            .changes
            .iter()
            .filter(|change| change.kind != TextChangeKindData::Added)
            .map(|change| change.text.as_str())
            .collect();
        assert_eq!(kept, typed, "kept and removed runs rebuild the typed text");
    }

    #[test]
    fn finish_lists_the_sentences_it_changed_or_dropped() {
        let typed = "Thursday works. Numbers by wed. Bring the deck.";
        let output = "Thursday works, I think. Numbers by wed.";
        let checked = finish(typed, output, "");
        assert_eq!(checked.yours_changed, [0, 2]);
        assert!(checked
            .changes
            .iter()
            .any(|change| change.kind == TextChangeKindData::Removed));
    }

    #[test]
    fn a_sentence_turned_into_its_opposite_is_one_the_user_must_look_at() {
        let checked = finish(
            "I can't make Thursday. Numbers by wed.",
            "Hi,\n\nI can make Thursday. Numbers by wed.",
            "",
        );
        assert_eq!(checked.yours_changed, [0]);
    }

    #[test]
    fn a_selection_in_multibyte_text_edits_without_splitting_a_character() {
        let typed = "Café works. numbers by wed ✓ Thanks é.";
        let start = typed.find("numbers").unwrap();
        let end = typed.find(" Thanks").unwrap();
        let output = "Café is fine. I'll send the numbers by Wednesday ✓ Thanks é.";
        let checked = check(
            AssistActionData::EditSelection,
            typed,
            Some(start..end),
            output,
            "",
        );
        assert!(checked.text.starts_with("Café works. "), "{}", checked.text);
        assert!(checked.text.ends_with(" Thanks é."), "{}", checked.text);
        assert!(checked.text.contains("Wednesday"), "{}", checked.text);
        for gap in &checked.gaps {
            assert!(checked.text.is_char_boundary(gap.start as usize));
            assert!(checked.text.is_char_boundary(gap.end as usize));
        }
    }

    #[test]
    fn a_signature_or_forward_header_the_model_wrote_is_removed() {
        let output =
            "Thursday works.\n\n-- \nAlex Demo\n\n---------- Forwarded message ----------\nFrom: x";
        let checked = finish("Thursday works.", output, "");
        assert_eq!(checked.text, "Thursday works.");
        assert!(checked.fixes.regions_removed);
        assert!(!checked.text.contains("Alex Demo"));
    }

    #[test]
    fn a_figure_nobody_supplied_becomes_a_gap_that_shows_the_guess() {
        let output = "The fee is £40 and we start at 3 pm on the 14th.";
        let checked = finish("", output, "Can you do the 14th at 3pm?");
        assert_eq!(
            checked.text,
            "The fee is [[?: amount, the draft said £40]] and we start at 3 pm on the 14th."
        );
        assert_eq!(checked.fixes.figures_to_gaps, 1);
        assert_eq!(checked.gaps.len(), 1);
        let gap = &checked.gaps[0];
        assert_eq!(gap.label, "amount, the draft said £40");
        assert_eq!(
            &checked.text[gap.start as usize..gap.end as usize],
            "[[?: amount, the draft said £40]]"
        );
    }

    #[test]
    fn every_occurrence_of_a_guessed_figure_is_replaced_and_known_ones_stay() {
        let output = "It is $45, yes $45. Table for 4 at 7pm.";
        let checked = finish("", output, "Table for 4 at 7pm please");
        assert_eq!(checked.fixes.figures_to_gaps, 2);
        assert_eq!(
            checked
                .text
                .matches("[[?: amount, the draft said $45]]")
                .count(),
            2
        );
        assert!(checked.text.contains("Table for 4 at 7pm."));
    }

    #[test]
    fn a_figure_the_user_typed_is_not_invented() {
        let checked = finish("Budget is £1,200.", "Hi,\n\nBudget is £1,200.\n\nAlex", "");
        assert!(checked.gaps.is_empty(), "{:?}", checked.text);
    }

    #[test]
    fn a_figure_inside_the_models_own_gap_is_left_alone() {
        let output = "Price: [[?: the price, they said 3 options]] and nothing else.";
        let checked = finish("", output, "");
        assert_eq!(checked.fixes.figures_to_gaps, 0);
        assert_eq!(checked.text, output);
        assert_eq!(checked.gaps.len(), 1);
    }

    #[test]
    fn gap_ranges_are_byte_offsets_after_multibyte_text() {
        let text = "Café open at [[?: the time]]. Bye [[?: your name]]";
        let gaps = gap_spans(text);
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].label, "the time");
        assert_eq!(
            &text[gaps[0].start as usize..gaps[0].end as usize],
            "[[?: the time]]"
        );
        assert_eq!(&text[gaps[1].start as usize..], "[[?: your name]]");
    }

    #[test]
    fn a_marker_left_open_stops_where_the_next_one_starts() {
        let text = "Meet [[?: the time\nThanks [[?: your name]]";
        let gaps = gap_spans(text);
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].label, "the time");
        assert_eq!(gaps[1].label, "your name");
        let same_line = gap_spans("a [[?: one b [[?: two]]");
        assert_eq!(
            same_line
                .iter()
                .map(|gap| gap.label.as_str())
                .collect::<Vec<_>>(),
            ["one b", "two"]
        );
    }

    #[test]
    fn a_marker_left_open_runs_to_the_end_of_its_line() {
        let text = "Meet at [[?: the time\nThanks";
        let gaps = gap_spans(text);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].label, "the time");
        assert_eq!(&text[gaps[0].end as usize..], "\nThanks");
    }

    #[test]
    fn an_edit_outside_the_selection_is_put_back() {
        let typed = "Thursday works. Numbers by wed. Thanks.";
        // Selection: "Numbers by wed."
        let selection = 16..31;
        let output = "Thursday is fine. I will send the numbers by Wednesday. Thanks a lot.";
        let checked = check(
            AssistActionData::EditSelection,
            typed,
            Some(selection),
            output,
            "",
        );
        assert!(checked.fixes.outside_reverted);
        assert!(
            checked.text.starts_with("Thursday works. "),
            "{}",
            checked.text
        );
        assert!(checked.text.ends_with(" Thanks."), "{}", checked.text);
        // Where the model's edit and its outside change run together, the
        // whole run goes back: what is outside stays exactly as it was.
        assert!(checked.text.contains("Wednesday."), "{}", checked.text);
        assert_eq!(checked.yours_changed, [1]);
    }

    #[test]
    fn an_edit_that_stays_inside_the_selection_is_kept_whole() {
        let typed = "Thursday works.\nnumbers by wed\nThanks.";
        let selection = 16..30;
        let output = "Thursday works.\nI'll send the numbers by Wednesday.\nThanks.";
        let checked = check(
            AssistActionData::EditSelection,
            typed,
            Some(selection),
            output,
            "",
        );
        assert!(!checked.fixes.outside_reverted);
        assert_eq!(checked.text, output);
    }

    fn edit(typed: &str, selection: Range<usize>, output: &str) -> Checked {
        check(
            AssistActionData::EditSelection,
            typed,
            Some(selection),
            output,
            "",
        )
    }

    #[test]
    fn bytes_outside_the_selection_stay_byte_for_byte() {
        // Selecting only the `a` of `cat`: `cat` must not become `dog`.
        let typed = "Keep cat safe.";
        let checked = edit(typed, 6..7, "Keep dog safe.");
        assert_eq!(checked.text, typed);
        assert!(checked.fixes.outside_reverted);
        // A model that changed only the selected byte is believed.
        let checked = edit(typed, 6..7, "Keep cot safe.");
        assert_eq!(checked.text, "Keep cot safe.");
        assert!(!checked.fixes.outside_reverted);
    }

    #[test]
    fn trailing_and_leading_blank_space_outside_the_selection_is_kept_byte_for_byte() {
        let typed = "\n  Keep cat safe.  \n";
        let (start, end) = (typed.find('a').unwrap(), typed.find('a').unwrap() + 1);
        // The model's answer arrives trimmed; an honest one-byte edit is kept.
        let checked = edit(typed, start..end, "Keep cot safe.");
        assert_eq!(checked.text, "\n  Keep cot safe.  \n");
        assert!(!checked.fixes.outside_reverted);
        // A change to the neighbouring word is put back with the blank space.
        let checked = edit(typed, start..end, "Keep dog safe.");
        assert_eq!(checked.text, typed);
        // So is a model that dropped everything.
        let checked = edit(typed, start..end, "x");
        assert_eq!(checked.text, typed);
        // The diff starts from the text as typed.
        assert!(checked
            .changes
            .iter()
            .all(|c| c.kind == TextChangeKindData::Kept));
    }

    #[test]
    fn whatever_the_model_returns_both_ends_of_the_text_survive() {
        let typed = "Hello there. Numbers by wed. Bye now.";
        let (start, end) = (13, 28);
        for output in [
            "",
            "Totally different text.",
            "Hello. Numbers by Wednesday. Bye.",
            "Hello there.  Numbers by Wednesday!  Bye now.",
            "Bye now.",
            "Hello there. Numbers by wed. Bye now. And more.",
        ] {
            let checked = edit(typed, start..end, output);
            assert!(
                checked.text.starts_with(&typed[..start]),
                "{output:?} -> {:?}",
                checked.text
            );
            assert!(
                checked.text.ends_with(&typed[end..]),
                "{output:?} -> {:?}",
                checked.text
            );
        }
    }

    #[test]
    fn a_figure_outside_the_selection_is_never_rewritten() {
        // `3` is known as part of `3pm`, and the model dropped the unit that
        // was selected; the figure pass must leave the rest alone.
        let typed = "Meet at 3 pm tomorrow.";
        let checked = edit(typed, 10..12, "Meet at 3 tomorrow.");
        assert!(!checked.text.contains("[[?:"), "{}", checked.text);
        assert!(checked.text.starts_with("Meet at 3 "), "{}", checked.text);
        assert!(checked.text.ends_with(" tomorrow."), "{}", checked.text);

        // The pass itself only reaches into the editable range.
        let text = "Meet at 4 tomorrow. It costs £40.";
        let editable = text.find("It").unwrap()..text.len();
        let (out, count) = figures_to_gaps(text, "", Some(editable));
        assert_eq!(count, 1);
        assert!(out.starts_with("Meet at 4 tomorrow."), "{out}");
        assert!(out.contains("[[?: amount, the draft said £40]]"), "{out}");
    }

    #[test]
    fn a_time_makes_its_hour_known() {
        let checked = finish("", "See you at 3.", "meet at 3pm?");
        assert!(checked.gaps.is_empty(), "{}", checked.text);
        let checked = finish("", "See you at 2pm.", "meet at 12pm?");
        assert_eq!(checked.gaps.len(), 1);
    }

    #[test]
    fn the_diff_of_nothing_typed_is_all_added() {
        let checked = check(AssistActionData::Write, "", None, "Hi Sam, yes.", "");
        assert_eq!(
            checked.changes,
            [TextChangeData {
                kind: TextChangeKindData::Added,
                text: "Hi Sam, yes.".into()
            }]
        );
        assert!(checked.yours_changed.is_empty());
    }

    #[test]
    fn a_figure_the_conversation_carries_is_known_whichever_way_it_is_written() {
        let checked = finish("", "See you at 3 pm.", "meet at 3pm?");
        assert!(checked.gaps.is_empty());
    }
}
