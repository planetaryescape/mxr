//! The labelled drafting corpus (blueprint 34, "Quality is measured on a
//! labelled drafting corpus"): forty cases in near-miss pairs, typed text
//! the largest group. Each case is a short thread, a few emails the user
//! sent as their voice, what the user typed, an instruction, and what a
//! good reply had to do. The composer's check stage grades drafts against
//! it; the daemon builds the prompt for each case.
//!
//! Every person and every sentence here is synthetic. The first case of a
//! pair is the base; its partner names it in `twin_of` and says what differs.
//! `recorded` holds model outputs, good and bad, so the graders can be
//! snapshot-tested with no model in CI.

mod pairs_a;
mod pairs_b;

/// The user the voice belongs to: the seeded personal demo account.
pub const ME_NAME: &str = "Alex Demo";
pub const ME_EMAIL: &str = "alex@demo.mxr.local";

/// One message of the thread. `target` marks the message the reply answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message {
    pub from_me: bool,
    /// "Name <address>" of the person the message is from, or of the user.
    pub who: &'static str,
    pub when: &'static str,
    pub text: &'static str,
    pub target: bool,
}

/// An email the user sent, which is what the draft's voice is matched to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SentEmail {
    pub to: &'static str,
    /// What the person wrote that this email answers, when it answers something.
    pub answering: Option<&'static str>,
    pub text: &'static str,
}

/// What the user had typed into the draft when they asked for help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftState {
    Empty,
    Typed(&'static str),
    /// Typed text with a selected byte range the user wants changed.
    Selection {
        typed: &'static str,
        start: usize,
        end: usize,
    },
}

impl DraftState {
    /// Everything the user typed, selected or not.
    pub const fn typed(&self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Typed(text) => text,
            Self::Selection { typed, .. } => typed,
        }
    }

    /// The part of the typed text that must survive: all of it, minus any
    /// selected range the user asked to change.
    pub fn kept_text(&self) -> String {
        match self {
            Self::Selection { typed, start, end } => {
                format!("{}{}", &typed[..*start], &typed[*end..])
            }
            other => other.typed().to_string(),
        }
    }
}

/// A question from the target message and words that answer it. Any one of
/// the words is enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub question: &'static str,
    pub any_of: &'static [&'static str],
}

/// What a good draft had to do for this case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expect {
    pub answers: &'static [Answer],
    /// Facts the draft leaves as `[[?: …]]`, each named by a word the gap has.
    pub gaps: &'static [&'static str],
    /// Numbers the case allows though no input carries them.
    pub allowed_numbers: &'static [&'static str],
    /// Inclusive word band for the reply.
    pub words: (u32, u32),
}

/// A model output recorded for this case. Good ones show what passes;
/// bad ones are named for the defect they carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recording {
    pub name: &'static str,
    /// The raw output, before clean-up.
    pub text: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    pub id: &'static str,
    pub description: &'static str,
    /// The base case this one is a near miss of, if any.
    pub twin_of: Option<&'static str>,
    /// The one fact that differs from the base; empty for a base case.
    pub differs_by: &'static str,
    pub thread: &'static [Message],
    pub voice: &'static [SentEmail],
    pub draft: DraftState,
    pub instruction: &'static str,
    pub expect: Expect,
    pub recorded: &'static [Recording],
}

/// Every case, pairs in order.
pub fn cases() -> impl Iterator<Item = &'static Case> {
    pairs_a::CASES.iter().chain(pairs_b::CASES.iter())
}

/// The case with this id.
pub fn case(id: &str) -> Option<&'static Case> {
    cases().find(|case| case.id == id)
}

pub(crate) const fn them(who: &'static str, when: &'static str, text: &'static str) -> Message {
    Message {
        from_me: false,
        who,
        when,
        text,
        target: false,
    }
}

pub(crate) const fn target(who: &'static str, when: &'static str, text: &'static str) -> Message {
    Message {
        from_me: false,
        who,
        when,
        text,
        target: true,
    }
}

pub(crate) const fn mine(when: &'static str, text: &'static str) -> Message {
    Message {
        from_me: true,
        who: "Alex Demo <alex@demo.mxr.local>",
        when,
        text,
        target: false,
    }
}

pub(crate) const fn expect(
    answers: &'static [Answer],
    gaps: &'static [&'static str],
    words: (u32, u32),
) -> Expect {
    Expect {
        answers,
        gaps,
        allowed_numbers: &[],
        words,
    }
}

pub(crate) const SAM: &str = "Sam Rivers <sam@example.com>";
pub(crate) const PRIYA: &str = "Priya Nair <priya@example.org>";
pub(crate) const JONAS: &str = "Jonas Berg <jonas@example.de>";

/// Signs with a first name, short and plain.
pub(crate) const VOICE_ALEX: &[SentEmail] = &[
    SentEmail {
        to: "sam@example.com",
        answering: Some("Can we pick a time next week?"),
        text: "Hi Sam,\n\nYes, next week should work. Tuesday or Thursday are both open for me.\n\nAlex",
    },
    SentEmail {
        to: "priya@example.org",
        answering: None,
        text: "Hi Priya,\n\nThanks, got it. I'll sort the slides tonight.\n\nAlex",
    },
    SentEmail {
        to: "jonas@example.de",
        answering: None,
        text: "Hi Jonas,\n\nSounds good, see you then.\n\nAlex",
    },
];

/// Signs with an initial, so a draft in this voice should sign "A".
pub(crate) const VOICE_INITIAL: &[SentEmail] = &[
    SentEmail {
        to: "sam@example.com",
        answering: None,
        text: "Hi Sam,\n\nTuesday works. See you there.\n\nA",
    },
    SentEmail {
        to: "priya@example.org",
        answering: None,
        text: "Hi Priya,\n\nNo problem, I'll send it over later.\n\nA",
    },
];

/// Uses em dashes freely.
pub(crate) const VOICE_DASHES: &[SentEmail] = &[
    SentEmail {
        to: "sam@example.com",
        answering: None,
        text: "Hi Sam,\n\nYes \u{2014} Tuesday works for me. Let's do it.\n\nAlex",
    },
    SentEmail {
        to: "priya@example.org",
        answering: None,
        text: "Hi Priya,\n\nGreat \u{2014} thanks for the update.\n\nAlex",
    },
];

/// The user writing German to a German colleague.
pub(crate) const VOICE_DE: &[SentEmail] = &[SentEmail {
    to: "jonas@example.de",
    answering: Some("Können wir nächste Woche telefonieren?"),
    text: "Hallo Jonas,\n\nja, nächste Woche geht bei mir. Dienstag passt.\n\nAlex",
}];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn there_are_forty_cases_in_near_miss_pairs() {
        let all: Vec<&Case> = cases().collect();
        assert_eq!(all.len(), 40);
        let bases = all.iter().filter(|case| case.twin_of.is_none()).count();
        assert_eq!(bases, 20, "one base per pair");
    }

    #[test]
    fn typed_text_is_the_largest_group() {
        let typed = cases()
            .filter(|case| !matches!(case.draft, DraftState::Empty))
            .count();
        let empty = cases().count() - typed;
        assert!(typed > empty, "typed {typed}, empty {empty}");
    }

    #[test]
    fn ids_are_unique_and_every_twin_names_a_base_case() {
        let mut seen = HashSet::new();
        for case in cases() {
            assert!(seen.insert(case.id), "duplicate id {}", case.id);
            if let Some(base) = case.twin_of {
                let base = self::case(base).expect("the base case exists");
                assert!(base.twin_of.is_none(), "{} twins a twin", case.id);
                assert!(!case.differs_by.is_empty(), "{} says what differs", case.id);
            } else {
                assert!(case.differs_by.is_empty(), "{} is a base", case.id);
            }
        }
    }

    #[test]
    fn every_case_has_a_target_and_a_voice_and_a_sane_word_band() {
        for case in cases() {
            assert_eq!(
                case.thread.iter().filter(|m| m.target).count(),
                1,
                "{} needs exactly one target",
                case.id
            );
            assert!(!case.voice.is_empty(), "{} has no voice", case.id);
            let (min, max) = case.expect.words;
            assert!(min > 0 && min < max, "{} word band", case.id);
            for answer in case.expect.answers {
                assert!(!answer.any_of.is_empty(), "{} answer", case.id);
            }
        }
    }

    #[test]
    fn a_selection_is_a_real_range_inside_the_typed_text() {
        for case in cases() {
            if let DraftState::Selection { typed, start, end } = case.draft {
                assert!(start < end && end <= typed.len(), "{}", case.id);
                assert!(typed.is_char_boundary(start) && typed.is_char_boundary(end));
                assert!(!case.draft.kept_text().contains(&typed[start..end]));
            }
        }
    }

    #[test]
    fn every_bad_class_has_a_recording_and_a_good_one_to_compare_with() {
        let mut bad_names: Vec<&str> = Vec::new();
        for case in cases() {
            let has_good = case.recorded.iter().any(|r| r.name == "good");
            let bad: Vec<&Recording> = case.recorded.iter().filter(|r| r.name != "good").collect();
            assert!(
                bad.is_empty() || has_good,
                "{} has bad outputs but no good one",
                case.id
            );
            bad_names.extend(bad.iter().map(|r| r.name));
        }
        for class in [
            "bad: subject line",
            "bad: recap of thread",
            "bad: dropped typed sentence",
            "bad: invented price",
            "bad: duplicate signature",
            "bad: reversed typed sentence",
            "bad: preamble",
            "bad: now for no",
        ] {
            assert!(bad_names.contains(&class), "no recording for {class}");
        }
    }
}
